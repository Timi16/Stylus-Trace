use std::fs;
use std::path::PathBuf;
use stylus_trace_studio::*;
use tempfile::TempDir;

#[test]
fn test_parse_sample_trace() {
    // Load sample trace
    let trace_json = fs::read_to_string("tests/fixtures/sample_trace_1.json")
        .expect("Failed to read sample trace");
    
    let raw_trace: serde_json::Value = serde_json::from_str(&trace_json)
        .expect("Failed to parse JSON");
    
    // Parse trace
    let parsed = parser::parse_trace("0xtest123", &raw_trace)
        .expect("Failed to parse trace");
    
    // Verify parsing
    assert_eq!(parsed.transaction_hash, "0xtest123");
    assert!(parsed.total_gas_used > 0);
    assert!(parsed.execution_steps.len() > 0);
    
    println!("Trace parsed successfully");
    println!("   Gas used: {}", parsed.total_gas_used);
    println!("   Steps: {}", parsed.execution_steps.len());
}

#[test]
fn test_build_stacks_from_sample() {
    let trace_json = fs::read_to_string("tests/fixtures/sample_trace_1.json")
        .expect("Failed to read sample trace");
    
    let raw_trace: serde_json::Value = serde_json::from_str(&trace_json)
        .expect("Failed to parse JSON");
    
    let parsed = parser::parse_trace("0xtest123", &raw_trace)
        .expect("Failed to parse trace");
    
    // Build stacks
    let stacks = aggregator::build_collapsed_stacks(&parsed);
    
    assert!(!stacks.is_empty(), "Should have at least one stack");
    
    println!("Built {} stacks", stacks.len());
    for (i, stack) in stacks.iter().take(3).enumerate() {
        println!("   {}. {} gas: {}", i + 1, stack.weight, stack.stack);
    }
}

#[test]
fn test_generate_flamegraph_from_sample() {
    let trace_json = fs::read_to_string("tests/fixtures/sample_trace_1.json")
        .expect("Failed to read sample trace");
    
    let raw_trace: serde_json::Value = serde_json::from_str(&trace_json)
        .expect("Failed to parse JSON");
    
    let parsed = parser::parse_trace("0xtest123", &raw_trace)
        .expect("Failed to parse trace");
    
    let stacks = aggregator::build_collapsed_stacks(&parsed);
    
    // Generate flamegraph
    let svg = flamegraph::generate_flamegraph(&stacks, None)
        .expect("Failed to generate flamegraph");
    
    assert!(svg.contains("<svg"), "Should be valid SVG");
    assert!(svg.contains("</svg>"), "Should be complete SVG");
    
    println!("Flamegraph generated ({} bytes)", svg.len());
}

#[test]
fn test_full_capture_workflow() {
    let temp_dir = TempDir::new().unwrap();
    let output_json = temp_dir.path().join("profile.json");
    let output_svg = temp_dir.path().join("flamegraph.svg");
    
    // Load sample trace
    let trace_json = fs::read_to_string("tests/fixtures/sample_trace_1.json")
        .expect("Failed to read sample trace");
    
    let raw_trace: serde_json::Value = serde_json::from_str(&trace_json)
        .expect("Failed to parse JSON");
    
    // Parse
    let parsed = parser::parse_trace("0xtest123", &raw_trace)
        .expect("Failed to parse trace");
    
    // Build stacks
    let stacks = aggregator::build_collapsed_stacks(&parsed);
    
    // Calculate hot paths
    let hot_paths = aggregator::calculate_hot_paths(&stacks, parsed.total_gas_used, 10);
    
    // Generate flamegraph
    let svg = flamegraph::generate_flamegraph(&stacks, None)
        .expect("Failed to generate flamegraph");
    
    // Create profile
    let profile = parser::to_profile(&parsed, hot_paths);
    
    // Write outputs
    output::write_profile(&profile, &output_json)
        .expect("Failed to write profile");
    
    output::write_svg(&svg, &output_svg)
        .expect("Failed to write SVG");
    
    // Verify files exist
    assert!(output_json.exists(), "Profile JSON should exist");
    assert!(output_svg.exists(), "Flamegraph SVG should exist");
    
    println!("Full workflow completed");
    println!("   Profile: {}", output_json.display());
    println!("   Flamegraph: {}", output_svg.display());
}