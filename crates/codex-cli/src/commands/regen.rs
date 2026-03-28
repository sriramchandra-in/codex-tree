/// The `regen` command — full rebuild of the knowledge tree.
///
/// Unlike `init`, `regen` expects `.codex-tree/` to already exist. It reads
/// the current `version.json` to preserve the generation lineage, increments
/// `generation`, resets `delta_count` to 0, and replaces all on-disk state
/// with a fresh parse of the repository.
use std::fs;
use std::path::Path;

use colored::Colorize;

use codex_parser::parser::parse_directory;
use codex_parser::registry::ParserRegistry;
use codex_parser::serializer::{compute_stats, write_tree};
use codex_parser::types::{TreeVersion, TreeVersionNumber};

use crate::error::{CliError, Result};
use crate::git;
use crate::utils::{calculate_dir_size, current_utc_iso8601, find_git_root, format_size};

pub fn run(
    path: &Path,
    no_intent: bool,
    no_claude: bool,
    no_cursor: bool,
    _languages: Option<&str>,
    verbose: bool,
    quiet: bool,
) -> Result<()> {
    // ── 1. Resolve path and find git root ─────────────────────────────────────
    let abs_path = fs::canonicalize(path).map_err(|e| {
        std::io::Error::new(
            e.kind(),
            format!("cannot access '{}': {}", path.display(), e),
        )
    })?;

    let git_root = find_git_root(&abs_path).ok_or_else(|| CliError::NoGitRepo {
        path: abs_path.clone(),
    })?;

    let codex_tree_dir = git_root.join(".codex-tree");
    if !codex_tree_dir.exists() {
        return Err(CliError::TreeNotFound {
            path: codex_tree_dir,
        });
    }

    // ── 2. Read current version to get generation number ──────────────────────
    let version_path = codex_tree_dir.join("version.json");
    let old_version: TreeVersion = serde_json::from_str(&fs::read_to_string(&version_path)?)?;
    let next_generation = old_version.tree_version.generation + 1;

    // ── 3. Remove old tree contents ───────────────────────────────────────────
    fs::remove_dir_all(&codex_tree_dir)?;

    // ── 4. Parse ──────────────────────────────────────────────────────────────
    let registry = ParserRegistry::with_defaults();
    let exclude_dirs = [
        ".git",
        "target",
        ".codex-tree",
        "node_modules",
        "__pycache__",
        ".venv",
        "venv",
        "vendor",
        "dist",
        "build",
    ];

    if !quiet {
        println!("{}", "  Rebuilding knowledge tree...".dimmed());
    }

    let (tree, modules) = parse_directory(&git_root, &registry, &exclude_dirs)?;

    if modules.is_empty() {
        return Err(CliError::NoSourceFiles {
            path: git_root.clone(),
            languages: "rust".to_string(),
        });
    }

    // ── 5. Build version with incremented generation ──────────────────────────
    let stats = compute_stats(&tree, &modules);
    let mut version = TreeVersion {
        format_version: "0.1.0".to_string(),
        tree_version: TreeVersionNumber {
            generation: next_generation,
            delta_count: 0,
        },
        generated_at: current_utc_iso8601(),
        generator: format!("codex-tree {}", env!("CARGO_PKG_VERSION")),
        source_commit: None,
        source_commit_date: None,
        stats,
    };

    if let Ok(head) = git::get_head_commit(&git_root) {
        version.source_commit_date = git::get_commit_date(&git_root, &head).ok();
        version.source_commit = Some(head);
    }

    // ── 6. Write ──────────────────────────────────────────────────────────────
    write_tree(&codex_tree_dir, &tree, &modules, &version)?;

    // ── 6a. Intent layer ─────────────────────────────────────────────────
    let intent_output = if !no_intent {
        run_intent_analysis(&codex_tree_dir, &modules, quiet).ok()
    } else {
        None
    };

    // ── 6b. Claude optimization layer ────────────────────────────────────
    if !no_claude {
        generate_claude_layer(
            &codex_tree_dir,
            &tree,
            &modules,
            &version,
            intent_output.as_ref(),
            quiet,
        );
    }

    if !no_cursor {
        generate_cursor_layer(
            &codex_tree_dir,
            &tree,
            &modules,
            &version,
            intent_output.as_ref(),
            quiet,
        );
    }

    // ── 7. Summary ────────────────────────────────────────────────────────────
    if !quiet {
        let tree_size = calculate_dir_size(&codex_tree_dir).unwrap_or(0);
        println!();
        println!(
            "  {} {}",
            "codex-tree regen".green().bold(),
            "✓".green().bold()
        );
        println!();
        println!(
            "  {:<18} {} (generation {})",
            "Tree version:".dimmed(),
            version.tree_version,
            next_generation
        );
        println!("  {:<18} {}", "Files parsed:".dimmed(), modules.len());
        println!(
            "  {:<18} {}",
            "Symbols found:".dimmed(),
            version.stats.total_symbols
        );
        println!(
            "  {:<18} {}",
            "Languages:".dimmed(),
            version.stats.languages.join(", ")
        );
        println!("  {:<18} {}", "Tree size:".dimmed(), format_size(tree_size));

        if verbose {
            println!();
            println!("  {}", "Per-file details:".dimmed());
            for module in &modules {
                println!(
                    "    {}  {} symbols  {} imports",
                    module.path.cyan(),
                    module.symbols.len(),
                    module.imports.len(),
                );
            }
        }
        println!();
    }

    Ok(())
}

// ── Analyzer helpers ──────────────────────────────────────────────────────────

/// Run intent analysis via Claude API. Returns None with a warning on failure.
fn run_intent_analysis(
    codex_tree_dir: &std::path::Path,
    modules: &[codex_parser::types::ModuleIndex],
    quiet: bool,
) -> Result<codex_analyzer::types::IntentOutput> {
    use codex_analyzer::analyzer::{write_intent, Analyzer};
    use codex_analyzer::client::ClaudeClient;

    let client = ClaudeClient::from_env().map_err(|e| {
        if !quiet {
            eprintln!(
                "  {} ANTHROPIC_API_KEY not set. Skipping intent layer.",
                "Warning:".yellow().bold()
            );
        }
        CliError::Analyzer(e)
    })?;

    if !quiet {
        println!("{}", "  Generating intent layer...".dimmed());
    }

    let rt = tokio::runtime::Runtime::new()?;
    let output = rt.block_on(async {
        let mut analyzer = Analyzer::new(client);
        analyzer.analyze(codex_tree_dir, modules).await
    })?;

    write_intent(codex_tree_dir, &output)?;

    Ok(output)
}

/// Generate the Claude optimization layer (L1/L2/L3 markdown files).
fn generate_claude_layer(
    codex_tree_dir: &std::path::Path,
    tree: &codex_parser::types::TreeStructure,
    modules: &[codex_parser::types::ModuleIndex],
    version: &codex_parser::types::TreeVersion,
    intent: Option<&codex_analyzer::types::IntentOutput>,
    quiet: bool,
) {
    use codex_analyzer::claude_layer;

    if !quiet {
        println!("{}", "  Generating Claude layer...".dimmed());
    }

    let l1 = claude_layer::generate_l1(tree, modules, version);
    let l2 = claude_layer::generate_l2(tree, modules, version, intent);
    let l3 = claude_layer::generate_l3(tree, modules, version, intent);

    if let Err(e) = claude_layer::write_claude_layer(codex_tree_dir, &l1, &l2, &l3) {
        if !quiet {
            eprintln!(
                "  {} Failed to write Claude layer: {}",
                "Warning:".yellow().bold(),
                e
            );
        }
    }
}

fn generate_cursor_layer(
    codex_tree_dir: &std::path::Path,
    tree: &codex_parser::types::TreeStructure,
    modules: &[codex_parser::types::ModuleIndex],
    version: &codex_parser::types::TreeVersion,
    intent: Option<&codex_analyzer::types::IntentOutput>,
    quiet: bool,
) {
    use codex_analyzer::cursor_layer;

    if !quiet {
        println!("{}", "  Generating Cursor layer...".dimmed());
    }

    let l1 = cursor_layer::generate_cursor_l1(tree, modules, version);
    let l2 = cursor_layer::generate_cursor_l2(tree, modules, version, intent);
    let l3 = cursor_layer::generate_cursor_l3(tree, modules, version, intent);

    if let Err(e) = cursor_layer::write_cursor_layer(codex_tree_dir, &l1, &l2, &l3) {
        if !quiet {
            eprintln!(
                "  {} Failed to write Cursor layer: {}",
                "Warning:".yellow().bold(),
                e
            );
        }
    }
}
