//! Stylus Trace Studio CLI
//!
//! A performance profiling tool for Arbitrum Stylus transactions.
//! Generates flamegraphs and detailed profiles from transaction traces.

use anyhow::Result;
use clap::{Parser, Subcommand};
use env_logger::Env;
use log::error;
use std::path::PathBuf;

mod aggregator;
mod commands;
mod flamegraph;
mod output;
mod parser;
mod rpc;
mod utils;

use commands::{execute_capture, validate_args, CaptureArgs};
use flamegraph::{FlamegraphConfig, FlamegraphPalette};
use utils::config::SCHEMA_VERSION;

/// Stylus Trace Studio - Performance profiling for Arbitrum Stylus
#[derive(Parser, Debug)]
#[command(name = "stylus-trace")]
#[command(version, about, long_about = None)]
struct Cli {
    /// Subcommand to execute
    #[command(subcommand)]
    command: Commands,
    
    /// Enable verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,
}

/// Available commands
#[derive(Subcommand, Debug)]
enum Commands {
    /// Capture and profile a transaction
    Capture {
        /// RPC endpoint URL
        #[arg(short, long, default_value = "http://localhost:8547")]
        rpc: String,
        
        /// Transaction hash to profile
        #[arg(short, long)]
        tx: String,
        
        /// Output path for JSON profile
        #[arg(short, long, default_value = "profile.json")]
        output: PathBuf,
        
        /// Output path for SVG flamegraph (optional)
        #[arg(short, long)]
        flamegraph: Option<PathBuf>,
        
        /// Number of top hot paths to include
        #[arg(long, default_value = "20")]
        top_paths: usize,
        
        /// Flamegraph title
        #[arg(long)]
        title: Option<String>,
        
        /// Flamegraph color palette (hot, mem, io, java, consistent)
        #[arg(long, default_value = "hot")]
        palette: String,
        
        /// Flamegraph width in pixels
        #[arg(long, default_value = "1200")]
        width: usize,
        
        /// Print text summary to stdout
        #[arg(long)]
        summary: bool,
    },
    
    /// Validate a profile JSON file
    Validate {
        /// Path to profile JSON file
        #[arg(short, long)]
        file: PathBuf,
    },
    
    /// Display schema information
    Schema {
        /// Show full schema details
        #[arg(long)]
        show: bool,
    },
    
    /// Display version information
    Version,
}

fn main() -> Result<()> {  // Add return type
    // Parse CLI arguments
    let cli = Cli::parse();
    
    // Setup logging
    let log_level = if cli.verbose { "debug" } else { "info" };
    env_logger::Builder::from_env(Env::default().default_filter_or(log_level)).init();
    
    // Execute command
    match cli.command {
        Commands::Capture {
            rpc,
            tx,
            output,
            flamegraph,
            top_paths,
            title,
            palette,
            width,
            summary,
        } => {
            // Parse palette
            let palette_enum = parse_palette(&palette);
            
            // Create flamegraph config
            let fg_config = if flamegraph.is_some() {
                let mut config = FlamegraphConfig::new();
                
                if let Some(title_str) = title {
                    config = config.with_title(title_str);
                }
                
                config = config.with_palette(palette_enum).with_width(width);
                
                Some(config)
            } else {
                None
            };
            
            // Create capture args
            let args = CaptureArgs {
                rpc_url: rpc,
                transaction_hash: tx,
                output_json: output,
                output_svg: flamegraph,
                top_paths,
                flamegraph_config: fg_config,
                print_summary: summary,
            };
            
            // Validate args first
            validate_args(&args)?;
            
            // Execute capture
            execute_capture(args)?;
        }
        
        Commands::Validate { file } => {
            validate_profile_file(file)?;
        }
        
        Commands::Schema { show } => {
            display_schema(show);
        }
        
        Commands::Version => {
            display_version();
        }
    }
    
    Ok(())  // Return Ok
}

/// Parse palette string to enum
///
/// **Private** - internal helper
fn parse_palette(palette_str: &str) -> FlamegraphPalette {
    match palette_str.to_lowercase().as_str() {
        "hot" => FlamegraphPalette::Hot,
        "mem" => FlamegraphPalette::Mem,
        "io" => FlamegraphPalette::Io,
        "java" => FlamegraphPalette::Java,
        "consistent" => FlamegraphPalette::Consistent,
        _ => {
            eprintln!("Warning: Unknown palette '{}', using 'hot'", palette_str);
            FlamegraphPalette::Hot
        }
    }
}

/// Validate a profile JSON file
///
/// **Private** - internal command implementation
fn validate_profile_file(file_path: PathBuf) -> Result<()> {
    use output::read_profile;
    
    println!("Validating profile: {}", file_path.display());
    
    let profile = read_profile(&file_path)?;
    
    println!("✓ Valid profile JSON");
    println!("  Version: {}", profile.version);
    println!("  Transaction: {}", profile.transaction_hash);
    println!("  Total Gas: {}", profile.total_gas);
    println!("  HostIO Calls: {}", profile.hostio_summary.total_calls);
    println!("  Hot Paths: {}", profile.hot_paths.len());
    
    Ok(())
}

/// Display schema information
///
/// **Private** - internal command implementation
fn display_schema(show_details: bool) {
    println!("Stylus Trace Studio Profile Schema");
    println!("Current Version: {}", SCHEMA_VERSION);
    println!();
    
    if show_details {
        println!("Schema Structure:");
        println!("  version: string          - Schema version (e.g., '1.0.0')");
        println!("  transaction_hash: string - Transaction hash");
        println!("  total_gas: number        - Total gas used");
        println!("  hostio_summary: object   - HostIO event statistics");
        println!("    total_calls: number    - Total HostIO calls");
        println!("    by_type: object        - Breakdown by HostIO type");
        println!("    total_hostio_gas: number - Gas consumed by HostIO");
        println!("  hot_paths: array         - Top gas-consuming execution paths");
        println!("    stack: string          - Stack trace");
        println!("    gas: number            - Gas consumed");
        println!("    percentage: number     - Percentage of total gas");
        println!("    source_hint: object?   - Source location (if available)");
        println!("  generated_at: string     - ISO 8601 timestamp");
    } else {
        println!("Use --show for detailed schema information");
    }
}

/// Display version information
///
/// **Private** - internal command implementation
fn display_version() {
    println!("Stylus Trace Studio v{}", env!("CARGO_PKG_VERSION"));
    println!("Profile Schema: v{}", SCHEMA_VERSION);
    println!();
    println!("A performance profiling tool for Arbitrum Stylus transactions.");
    println!("https://github.com/your-org/stylus-trace-studio");
}