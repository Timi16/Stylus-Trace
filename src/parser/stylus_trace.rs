//! Main trace parser for stylusTracer output.
//!
//! Parses raw JSON from debug_traceTransaction into structured data.
//! Handles schema validation and extraction of execution steps.

use super::hostio::{extract_hostio_events, HostIoStats};
use super::schema::Profile;
use crate::utils::error::ParseError;
use crate::utils::config::SCHEMA_VERSION;
use log::{debug, warn};
use serde::{Deserialize, Serialize};

/// Raw execution step from stylusTracer
///
/// This represents a single step in the WASM execution.
/// The exact fields depend on the stylusTracer implementation.
#[derive(Debug, Clone, Deserialize)]
pub struct ExecutionStep {
    /// Program counter / instruction pointer
    #[serde(default)]
    pub pc: u64,  
    
    /// Gas remaining at this step
    #[serde(default)]
    pub gas: u64,
    
    /// Gas cost of this operation
    /// FIXED: Handle both camelCase and snake_case
    #[serde(default, alias = "gasCost")]
    pub gas_cost: u64,  
    
    /// Operation name (if available)
    #[serde(default)]
    pub op: Option<String>, 
    
    /// Stack depth
    #[serde(default)]
    pub depth: u32,  
    
    /// Function name (if debug symbols present)
    #[serde(default)]
    pub function: Option<String>, 
}

/// Parsed trace data (internal representation)
///
/// **Private** - only used during parsing, not exposed
#[derive(Debug, Clone)]
pub struct ParsedTrace {
    pub transaction_hash: String,
    pub total_gas_used: u64,
    pub execution_steps: Vec<ExecutionStep>,
    pub hostio_stats: HostIoStats,
}

/// Parse raw trace JSON from stylusTracer
///
/// **Public** - main entry point for parsing
///
/// # Arguments
/// * `tx_hash` - Transaction hash being profiled
/// * `raw_trace` - Raw JSON from debug_traceTransaction
///
/// # Returns
/// Parsed trace data ready for aggregation
///
/// # Errors
/// * `ParseError::JsonError` - Invalid JSON structure
/// * `ParseError::InvalidFormat` - Missing required fields
/// * `ParseError::UnsupportedVersion` - Incompatible trace format
pub fn parse_trace(
    tx_hash: &str,
    raw_trace: &serde_json::Value,
) -> Result<ParsedTrace, ParseError> {
    debug!("Parsing trace for transaction: {}", tx_hash);
    
    // Handle different trace formats
    let trace_obj = match raw_trace {
        // Format 1: Direct object with structLogs/gasUsed
        serde_json::Value::Object(obj) => obj.clone(),
        
        // Format 2: Array of structLogs (wrap it)
        serde_json::Value::Array(logs) => {
            warn!("Trace is array format, wrapping as structLogs");
            let mut wrapper = serde_json::Map::new();
            wrapper.insert("structLogs".to_string(), raw_trace.clone());
            wrapper.insert("gasUsed".to_string(), serde_json::json!(0));
            wrapper
        }
        
        // Format 3: Invalid
        _ => {
            return Err(ParseError::InvalidFormat(
                "Trace must be a JSON object or array".to_string()
            ));
        }
    };
    
    // Extract total gas used
    let total_gas_used = extract_total_gas(&trace_obj)?;
    
    // Extract execution steps
    let execution_steps = extract_execution_steps(&trace_obj)?;
    
    debug!("Parsed {} execution steps", execution_steps.len());
    
    // Extract HostIO statistics
    let hostio_stats = extract_hostio_events(raw_trace);
    
    debug!(
        "Found {} HostIO calls consuming {} gas",
        hostio_stats.total_calls(),
        hostio_stats.total_gas()
    );
    
    Ok(ParsedTrace {
        transaction_hash: tx_hash.to_string(),
        total_gas_used,
        execution_steps,
        hostio_stats,
    })
}

/// Extract total gas used from trace
///
/// **Private** - internal extraction logic
fn extract_total_gas(trace_obj: &serde_json::Map<String, serde_json::Value>) -> Result<u64, ParseError> {
    // Try multiple possible field names (trace format may vary)
    let gas_fields = ["gasUsed", "gas_used", "totalGas", "total_gas"];
    
    for field in &gas_fields {
        if let Some(gas_value) = trace_obj.get(*field) {
            if let Some(gas) = gas_value.as_u64() {
                return Ok(gas);
            }
            // Try parsing from string (some RPCs return hex strings)
            if let Some(gas_str) = gas_value.as_str() {
                if let Ok(gas) = parse_gas_value(gas_str) {
                    return Ok(gas);
                }
            }
        }
    }
    
    // If no gas field found, try to calculate from steps
    warn!("Gas field not found in trace, will calculate from steps");
    Ok(0) // Will be calculated later from steps
}

/// Extract execution steps from trace
///
/// **Private** - internal extraction logic
fn extract_execution_steps(
    trace_obj: &serde_json::Map<String, serde_json::Value>,
) -> Result<Vec<ExecutionStep>, ParseError> {
    // Try multiple possible field names
    let step_fields = ["structLogs", "struct_logs", "steps", "trace"];
    
    for field in &step_fields {
        if let Some(steps_value) = trace_obj.get(*field) {
            if let Some(steps_array) = steps_value.as_array() {
                return parse_steps_array(steps_array);
            }
        }
    }
    
    // No steps found - this might be valid for very simple transactions
    warn!("No execution steps found in trace");
    Ok(Vec::new())
}

/// Parse array of execution steps
///
/// **Private** - internal parsing logic
fn parse_steps_array(steps_array: &[serde_json::Value]) -> Result<Vec<ExecutionStep>, ParseError> {
    let mut steps = Vec::with_capacity(steps_array.len());
    
    for (index, step_value) in steps_array.iter().enumerate() {
        match serde_json::from_value::<ExecutionStep>(step_value.clone()) {
            Ok(step) => steps.push(step),
            Err(e) => {
                // Log but don't fail - some steps may be malformed
                warn!("Failed to parse step {}: {}", index, e);
            }
        }
    }
    
    if steps.is_empty() && !steps_array.is_empty() {
        return Err(ParseError::InvalidFormat(
            "All execution steps failed to parse".to_string()
        ));
    }
    
    Ok(steps)
}

/// Parse gas value from hex string or decimal
///
/// **Private** - internal utility
fn parse_gas_value(value: &str) -> Result<u64, ParseError> {
    // Handle hex values (0x prefix)
    if let Some(hex_str) = value.strip_prefix("0x") {
        u64::from_str_radix(hex_str, 16)
            .map_err(|e| ParseError::InvalidFormat(format!("Invalid hex gas value: {}", e)))
    } else {
        // Try parsing as decimal
        value.parse::<u64>()
            .map_err(|e| ParseError::InvalidFormat(format!("Invalid decimal gas value: {}", e)))
    }
}

/// Convert parsed trace to output profile format
///
/// **Public** - used by commands to create final output
///
/// # Arguments
/// * `parsed_trace` - Parsed trace data
/// * `hot_paths` - Pre-calculated hot paths (from aggregator)
///
/// # Returns
/// Profile ready for JSON serialization
pub fn to_profile(
    parsed_trace: &ParsedTrace,
    hot_paths: Vec<super::schema::HotPath>,
) -> Profile {
    use chrono::Utc;
    
    Profile {
        version: SCHEMA_VERSION.to_string(),
        transaction_hash: parsed_trace.transaction_hash.clone(),
        total_gas: parsed_trace.total_gas_used,
        hostio_summary: super::schema::HostIoSummary {
            total_calls: parsed_trace.hostio_stats.total_calls(),
            by_type: parsed_trace.hostio_stats.to_map(),
            total_hostio_gas: parsed_trace.hostio_stats.total_gas(),
        },
        hot_paths,
        generated_at: Utc::now().to_rfc3339(),
    }
}

/// Validate that we can parse a trace (quick check)
///
/// **Public** - used by validate command
///
/// # Arguments
/// * `raw_trace` - Raw JSON to validate
///
/// # Returns
/// Ok if trace appears valid, Err with details if not
pub fn validate_trace_format(raw_trace: &serde_json::Value) -> Result<(), ParseError> {
    let trace_obj = raw_trace.as_object()
        .ok_or_else(|| ParseError::InvalidFormat("Expected JSON object".to_string()))?;
    
    // Check for at least one expected field
    let has_gas = trace_obj.contains_key("gasUsed") 
        || trace_obj.contains_key("gas_used")
        || trace_obj.contains_key("totalGas");
    
    let has_steps = trace_obj.contains_key("structLogs")
        || trace_obj.contains_key("steps")
        || trace_obj.contains_key("trace");
    
    if !has_gas && !has_steps {
        return Err(ParseError::InvalidFormat(
            "Trace does not contain expected fields (gas or steps)".to_string()
        ));
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_gas_value() {
        assert_eq!(parse_gas_value("1000").unwrap(), 1000);
        assert_eq!(parse_gas_value("0x3e8").unwrap(), 1000);
        assert!(parse_gas_value("invalid").is_err());
    }

    #[test]
    fn test_extract_total_gas() {
        let trace = json!({
            "gasUsed": 50000
        });
        
        let gas = extract_total_gas(trace.as_object().unwrap()).unwrap();
        assert_eq!(gas, 50000);
    }

    #[test]
    fn test_extract_total_gas_hex() {
        let trace = json!({
            "gasUsed": "0xc350"
        });
        
        let gas = extract_total_gas(trace.as_object().unwrap()).unwrap();
        assert_eq!(gas, 50000);
    }

    #[test]
    fn test_parse_trace_minimal() {
        let raw_trace = json!({
            "gasUsed": 100000,
            "structLogs": []
        });
        
        let parsed = parse_trace("0xabc123", &raw_trace).unwrap();
        assert_eq!(parsed.total_gas_used, 100000);
        assert_eq!(parsed.transaction_hash, "0xabc123");
    }

    #[test]
    fn test_validate_trace_format() {
        let valid_trace = json!({
            "gasUsed": 1000
        });
        assert!(validate_trace_format(&valid_trace).is_ok());
        
        let invalid_trace = json!({
            "random_field": "value"
        });
        assert!(validate_trace_format(&invalid_trace).is_err());
    }
    
    #[test]
    fn test_parse_camelcase_gas_cost() {
        let raw_trace = json!({
            "gasUsed": 100,
            "structLogs": [
                {
                    "pc": 0,
                    "op": "PUSH1",
                    "gas": 1000,
                    "gasCost": 3,
                    "depth": 1
                }
            ]
        });
        
        let parsed = parse_trace("0xtest", &raw_trace).unwrap();
        assert_eq!(parsed.execution_steps.len(), 1);
        assert_eq!(parsed.execution_steps[0].gas_cost, 3);
    }
}