//! SVG flamegraph generation using the inferno library.
//!
//! Converts collapsed stacks into interactive SVG flamegraphs.
//! The inferno crate handles all the heavy lifting (layout, colors, interactivity).

use crate::aggregator::stack_builder::CollapsedStack;
use crate::utils::error::FlamegraphError;
use inferno::flamegraph::{self, Options, Palette};
use log::{debug, info};
use std::io::{BufWriter, Cursor};
use std::str::FromStr; 
/// Flamegraph configuration
///
/// **Public** - allows customization of flamegraph appearance
#[derive(Debug, Clone)]
pub struct FlamegraphConfig {
    /// Title displayed at the top of the flamegraph
    pub title: String,
    
    /// What the "weight" represents (e.g., "gas", "samples", "time")
    pub count_name: String,
    
    /// Color palette to use
    pub palette: FlamegraphPalette,
    
    /// Minimum width in pixels to show a frame
    pub min_width: f64,
    
    /// Image width in pixels
    pub image_width: Option<usize>,
    
    /// Reverse stack order (root at bottom vs top)
    pub reverse: bool,
}

/// Color palettes for flamegraph
///
/// **Public** - user can choose color scheme
#[derive(Debug, Clone, Copy)]
pub enum FlamegraphPalette {
    /// Hot colors (red/orange) - emphasizes "hot" paths
    Hot,
    
    /// Memory colors (green) - good for allocation profiles
    Mem,
    
    /// IO colors (blue) - good for I/O operations
    Io,
    
    /// Java colors (green/aqua) - traditional Java profiler colors
    Java,
    
    /// Consistent colors based on function name hash
    Consistent,
}

impl Default for FlamegraphConfig {
    fn default() -> Self {
        Self {
            title: "Stylus Transaction Profile".to_string(),
            count_name: "gas".to_string(),
            palette: FlamegraphPalette::Hot,
            min_width: 0.1,
            image_width: Some(1200),
            reverse: false,
        }
    }
}

impl FlamegraphConfig {
    /// Create a new config with default values
    ///
    /// **Public** - constructor
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Set custom title
    ///
    /// **Public** - builder pattern
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }
    
    /// Set color palette
    ///
    /// **Public** - builder pattern
    pub fn with_palette(mut self, palette: FlamegraphPalette) -> Self {
        self.palette = palette;
        self
    }
    
    /// Set image width
    ///
    /// **Public** - builder pattern
    pub fn with_width(mut self, width: usize) -> Self {
        self.image_width = Some(width);
        self
    }
}

/// Generate SVG flamegraph from collapsed stacks
///
/// **Public** - main entry point for flamegraph generation
///
/// # Arguments
/// * `stacks` - Collapsed stacks from aggregator
/// * `config` - Flamegraph configuration (optional)
///
/// # Returns
/// SVG content as a UTF-8 string
///
/// # Errors
/// * `FlamegraphError::EmptyStacks` - No stacks to visualize
/// * `FlamegraphError::GenerationFailed` - Inferno failed to generate SVG
///
/// # Example
/// ```ignore
/// let stacks = build_collapsed_stacks(&parsed_trace);
/// let config = FlamegraphConfig::default();
/// let svg = generate_flamegraph(&stacks, Some(&config))?;
/// ```
pub fn generate_flamegraph(
    stacks: &[CollapsedStack],
    config: Option<&FlamegraphConfig>,
) -> Result<String, FlamegraphError> {
    if stacks.is_empty() {
        return Err(FlamegraphError::EmptyStacks);
    }
    
    let config = config.cloned().unwrap_or_default();
    
    info!("Generating flamegraph with {} stacks", stacks.len());
    debug!("Flamegraph config: {:?}", config);
    
    // Convert stacks to collapsed format (one line per stack)
    let collapsed_input = stacks_to_collapsed_format(stacks);
    
    // Create inferno options
    let mut options = create_inferno_options(&config);
    
    // Prepare input/output buffers
    let input_reader = Cursor::new(collapsed_input.as_bytes());
    let mut output_buffer = Vec::new();
    
    // Generate flamegraph using inferno
    flamegraph::from_reader(
        &mut options,
        input_reader,
        BufWriter::new(&mut output_buffer),
    )
    .map_err(|e| FlamegraphError::GenerationFailed(format!("Inferno error: {}", e)))?;
    
    // Convert output to UTF-8 string
    let svg_content = String::from_utf8(output_buffer)
        .map_err(|e| FlamegraphError::GenerationFailed(format!("Invalid UTF-8: {}", e)))?;
    
    info!("Flamegraph generated successfully ({} bytes)", svg_content.len());
    
    Ok(svg_content)
}

/// Convert CollapsedStack vector to collapsed format string
///
/// **Private** - internal conversion
///
/// Format: one line per stack
/// "stack_trace weight\n"
fn stacks_to_collapsed_format(stacks: &[CollapsedStack]) -> String {
    stacks
        .iter()
        .map(|stack| stack.to_line())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Create inferno Options from our config
///
/// **Private** - internal conversion
fn create_inferno_options(config: &FlamegraphConfig) -> Options<'static> {
    let mut options = Options::default();
    
    // Set title
    options.title = config.title.clone();
    
    // Set count name (appears in tooltips)
    options.count_name = config.count_name.clone();
    
    // FIX: Inferno 0.11 uses `colors` field, not `palette`
    // Set color scheme using the `colors` field
    options.colors = match config.palette {
        FlamegraphPalette::Hot => Palette::from_str("hot").unwrap_or_default(),
        FlamegraphPalette::Mem => Palette::from_str("mem").unwrap_or_default(),
        FlamegraphPalette::Io => Palette::from_str("io").unwrap_or_default(),
        FlamegraphPalette::Java => Palette::from_str("java").unwrap_or_default(),
        FlamegraphPalette::Consistent => Palette::from_str("aqua").unwrap_or_default(),
    };
    // Set minimum width
    options.min_width = config.min_width;
    
    // FIX: image_width expects Option<usize>
    options.image_width = config.image_width;
    
    // Set reverse (false = root at bottom, true = root at top)
    options.reverse_stack_order = config.reverse;
    
    // Enable name attributes for better tooltips
    options.negate_differentials = false;
    options.factor = 1.0;
    
    // Subtitle with metadata
    options.subtitle = Some("Generated by Stylus Trace Studio".to_string());
    
    options
}

/// Generate a minimal text-based representation (for debugging)
///
/// **Public** - useful for tests and debugging without SVG
///
/// # Arguments
/// * `stacks` - Collapsed stacks
/// * `max_lines` - Maximum lines to output
///
/// # Returns
/// Human-readable text representation
pub fn generate_text_summary(stacks: &[CollapsedStack], max_lines: usize) -> String {
    let mut lines = Vec::new();
    
    lines.push("Top Gas Consumers:".to_string());
    lines.push("─".repeat(80));
    
    for (i, stack) in stacks.iter().take(max_lines).enumerate() {
        let line = format!(
            "{:>3}. {:>10} gas | {}",
            i + 1,
            stack.weight,
            stack.stack
        );
        lines.push(line);
    }
    
    if stacks.len() > max_lines {
        lines.push(format!("... and {} more stacks", stacks.len() - max_lines));
    }
    
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stacks_to_collapsed_format() {
        let stacks = vec![
            CollapsedStack::new("main;execute".to_string(), 5000),
            CollapsedStack::new("main;storage".to_string(), 3000),
        ];
        
        let collapsed = stacks_to_collapsed_format(&stacks);
        
        assert_eq!(collapsed, "main;execute 5000\nmain;storage 3000");
    }

    #[test]
    fn test_generate_flamegraph_empty_stacks() {
        let stacks: Vec<CollapsedStack> = vec![];
        let result = generate_flamegraph(&stacks, None);
        
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), FlamegraphError::EmptyStacks));
    }

    #[test]
    fn test_generate_flamegraph_with_stacks() {
        let stacks = vec![
            CollapsedStack::new("main".to_string(), 1000),
            CollapsedStack::new("main;execute".to_string(), 500),
        ];
        
        let result = generate_flamegraph(&stacks, None);
        
        // Should generate valid SVG
        assert!(result.is_ok());
        let svg = result.unwrap();
        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
        assert!(svg.contains("main"));
    }

    #[test]
    fn test_flamegraph_config_builder() {
        let config = FlamegraphConfig::new()
            .with_title("Custom Title")
            .with_palette(FlamegraphPalette::Mem)
            .with_width(1600);
        
        assert_eq!(config.title, "Custom Title");
        assert!(matches!(config.palette, FlamegraphPalette::Mem));
        assert_eq!(config.image_width, Some(1600));
    }

    #[test]
    fn test_generate_text_summary() {
        let stacks = vec![
            CollapsedStack::new("main;execute".to_string(), 5000),
            CollapsedStack::new("main;storage".to_string(), 3000),
            CollapsedStack::new("main;compute".to_string(), 2000),
        ];
        
        let summary = generate_text_summary(&stacks, 2);
        
        assert!(summary.contains("5000"));
        assert!(summary.contains("main;execute"));
        assert!(summary.contains("and 1 more stacks"));
    }
}