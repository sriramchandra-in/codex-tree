use std::path::Path;

use clap::{Parser, Subcommand};
use colored::Colorize;

mod commands;
mod compaction;
mod error;
mod git;
mod utils;

/// codex-tree: Generate and maintain knowledge trees for AI codebase comprehension
#[derive(Parser)]
#[command(name = "codex-tree", version, about)]
struct Cli {
    /// Target directory (default: current directory)
    #[arg(long, global = true, default_value = ".")]
    path: String,

    /// Increase output verbosity
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Suppress non-error output
    #[arg(short, long, global = true)]
    quiet: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate a .codex-tree/ directory from scratch
    Init {
        /// Skip AI intent layer generation
        #[arg(long)]
        no_intent: bool,

        /// Skip Claude optimization layer generation
        #[arg(long)]
        no_claude: bool,

        /// Skip Cursor optimization layer generation
        #[arg(long)]
        no_cursor: bool,

        /// Comma-separated list of languages to parse
        #[arg(long)]
        languages: Option<String>,

        /// Show what would be generated without writing files
        #[arg(long)]
        dry_run: bool,
    },

    /// Apply incremental delta based on changes since last tree state
    Update {
        /// Skip AI intent analysis for changed files
        #[arg(long)]
        no_intent: bool,

        /// Skip Claude layer regeneration
        #[arg(long)]
        no_claude: bool,

        /// Skip Cursor layer regeneration
        #[arg(long)]
        no_cursor: bool,

        /// Skip auto-compaction even if thresholds are met
        #[arg(long)]
        no_compact: bool,
    },

    /// Full rebuild of the knowledge tree (new generation)
    Regen {
        /// Skip AI intent layer generation
        #[arg(long)]
        no_intent: bool,

        /// Skip Claude optimization layer generation
        #[arg(long)]
        no_claude: bool,

        /// Skip Cursor optimization layer generation
        #[arg(long)]
        no_cursor: bool,

        /// Comma-separated list of languages to parse
        #[arg(long)]
        languages: Option<String>,
    },

    /// Display benchmarked token savings and tree statistics
    Report {
        /// Output format: text or json
        #[arg(long, default_value = "text")]
        format: String,

        /// Run actual AI benchmark (with/without tree comparison)
        #[arg(long)]
        benchmark: bool,
    },

    /// Staleness safety valve — compare tree against working state
    Check {
        /// Output format: text or json
        #[arg(long, default_value = "text")]
        format: String,

        /// Exit with code 1 if any files are stale
        #[arg(long)]
        fail_if_stale: bool,
    },
}

fn main() {
    let cli = Cli::parse();

    let result = match &cli.command {
        Commands::Init {
            no_intent,
            no_claude,
            no_cursor,
            languages,
            dry_run,
        } => commands::init::run(
            Path::new(&cli.path),
            *no_intent,
            *no_claude,
            *no_cursor,
            languages.as_deref(),
            *dry_run,
            cli.verbose,
            cli.quiet,
        ),
        Commands::Update {
            no_intent,
            no_claude,
            no_cursor,
            no_compact,
        } => commands::update::run(
            Path::new(&cli.path),
            *no_intent,
            *no_claude,
            *no_cursor,
            *no_compact,
            cli.verbose,
            cli.quiet,
        ),
        Commands::Regen {
            no_intent,
            no_claude,
            no_cursor,
            languages,
        } => commands::regen::run(
            Path::new(&cli.path),
            *no_intent,
            *no_claude,
            *no_cursor,
            languages.as_deref(),
            cli.verbose,
            cli.quiet,
        ),
        Commands::Report { format, benchmark } => commands::report::run(
            Path::new(&cli.path),
            format,
            *benchmark,
            cli.verbose,
            cli.quiet,
        ),
        Commands::Check {
            format,
            fail_if_stale,
        } => commands::check::run(
            Path::new(&cli.path),
            format,
            *fail_if_stale,
            cli.verbose,
            cli.quiet,
        ),
    };

    if let Err(e) = result {
        eprintln!("{}: {}", "Error".red().bold(), e);
        std::process::exit(1);
    }
}
