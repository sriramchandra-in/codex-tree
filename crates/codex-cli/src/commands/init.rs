use std::path::Path;

use colored::Colorize;

use codex_parser::parser::parse_directory;
use codex_parser::registry::ParserRegistry;
use codex_parser::serializer::{compute_stats, create_initial_version, write_tree};

use crate::error::{CliError, Result};
use crate::git;
use crate::utils::{calculate_dir_size, find_git_root, format_size};

// ── Public entry point ────────────────────────────────────────────────────────

/// Boolean flags for `codex-tree init` (keeps `run` under Clippy's argument limit).
pub struct InitOptions {
    pub no_intent: bool,
    pub no_claude: bool,
    pub no_cursor: bool,
    pub dry_run: bool,
    pub verbose: bool,
    pub quiet: bool,
}

pub fn run(path: &Path, options: InitOptions, _languages: Option<&str>) -> Result<()> {
    let InitOptions {
        no_intent,
        no_claude,
        no_cursor,
        dry_run,
        verbose,
        quiet,
    } = options;
    // ── 1. Resolve absolute path ──────────────────────────────────────────────
    let abs_path = std::fs::canonicalize(path).map_err(|e| {
        // Provide a friendlier error if the path simply doesn't exist.
        std::io::Error::new(
            e.kind(),
            format!("cannot access '{}': {}", path.display(), e),
        )
    })?;

    // ── 2. Find git root ──────────────────────────────────────────────────────
    let git_root = find_git_root(&abs_path).ok_or_else(|| CliError::NoGitRepo {
        path: abs_path.clone(),
    })?;

    // ── 3. Check .codex-tree/ doesn't already exist ───────────────────────────
    let output_dir = git_root.join(".codex-tree");
    if output_dir.exists() {
        return Err(CliError::TreeAlreadyExists {
            path: output_dir.clone(),
        });
    }

    // ── 4. Build parser registry ──────────────────────────────────────────────
    let registry = ParserRegistry::with_defaults();

    // ── 5. Excluded directories ───────────────────────────────────────────────
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
        println!("{}", "  Scanning source files...".dimmed());
    }

    // ── 6. Parse ──────────────────────────────────────────────────────────────
    let (tree, modules) = parse_directory(&git_root, &registry, &exclude_dirs)?;

    // ── 7. Require at least one parsed file ───────────────────────────────────
    if modules.is_empty() {
        return Err(CliError::NoSourceFiles {
            path: git_root.clone(),
            languages: "rust".to_string(),
        });
    }

    // ── 8. Stats + version ───────────────────────────────────────────────────
    let stats = compute_stats(&tree, &modules);
    let mut version = create_initial_version(&stats);

    // Fill in the git commit so `update` can diff against it.
    if let Ok(head) = git::get_head_commit(&git_root) {
        version.source_commit_date = git::get_commit_date(&git_root, &head).ok();
        version.source_commit = Some(head);
    }

    // ── 9. Dry-run early exit ─────────────────────────────────────────────────
    if dry_run {
        println!("{}", "  codex-tree init (dry run)".green().bold());
        println!();
        println!("  Would write to:   {}", output_dir.display());
        println!("  Files found:      {}", modules.len());
        println!(
            "  Symbols found:    {}",
            modules.iter().map(|m| m.symbols.len()).sum::<usize>()
        );
        println!("  Languages:        {}", stats.languages.join(", "));
        println!();
        println!("  No files written (--dry-run).");
        return Ok(());
    }

    // ── 10. Write tree ────────────────────────────────────────────────────────
    write_tree(&output_dir, &tree, &modules, &version)?;

    // ── 10a. Intent layer ────────────────────────────────────────────────
    let intent_output = if !no_intent {
        run_intent_analysis(&output_dir, &modules, quiet).ok()
    } else {
        None
    };

    // ── 10b. Claude optimization layer ───────────────────────────────────
    if !no_claude {
        generate_claude_layer(
            &output_dir,
            &tree,
            &modules,
            &version,
            intent_output.as_ref(),
            quiet,
        );
    }

    if !no_cursor {
        generate_cursor_layer(
            &output_dir,
            &tree,
            &modules,
            &version,
            intent_output.as_ref(),
            quiet,
        );
    }

    // ── 11. Compute on-disk size ──────────────────────────────────────────────
    let tree_size = calculate_dir_size(&output_dir).unwrap_or(0);

    // ── 12. Summary output ────────────────────────────────────────────────────
    if !quiet {
        println!();
        println!(
            "  {} {}",
            "codex-tree init".green().bold(),
            "✓".green().bold()
        );
        println!();
        println!(
            "  {:<18} {}",
            "Tree version:".dimmed(),
            version.tree_version
        );
        println!("  {:<18} {}", "Files parsed:".dimmed(), modules.len());
        println!(
            "  {:<18} {}",
            "Symbols found:".dimmed(),
            stats.total_symbols
        );
        println!(
            "  {:<18} {}",
            "Languages:".dimmed(),
            stats.languages.join(", ")
        );
        println!("  {:<18} {}", "Tree size:".dimmed(), format_size(tree_size));
        println!("  {:<18} {}", "Location:".dimmed(), output_dir.display());
        println!();

        if verbose {
            println!("  {}", "Per-file details:".dimmed());
            for module in &modules {
                println!(
                    "    {}  {} symbols  {} imports",
                    module.path.cyan(),
                    module.symbols.len(),
                    module.imports.len(),
                );
            }
            println!();
        }
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
        crate::error::CliError::Analyzer(e)
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

/// Generate the Cursor optimization layer (L1/L2/L3 markdown under `cursor/`).
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
