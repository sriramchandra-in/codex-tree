use std::path::{Path, PathBuf};

use colored::Colorize;

use codex_parser::parser::parse_directory;
use codex_parser::registry::ParserRegistry;
use codex_parser::serializer::{compute_stats, create_initial_version, write_tree};

use crate::error::{CliError, Result};

// ── Public entry point ────────────────────────────────────────────────────────

pub fn run(
    path: &Path,
    _no_intent: bool,
    _no_claude: bool,
    _languages: Option<&str>,
    dry_run: bool,
    verbose: bool,
    quiet: bool,
) -> Result<()> {
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
    let version = create_initial_version(&stats);

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

    // ── 11. Compute on-disk size ──────────────────────────────────────────────
    let tree_size = calculate_dir_size(&output_dir).unwrap_or(0);

    // ── 12. Summary output ────────────────────────────────────────────────────
    if !quiet {
        println!();
        println!("  {} {}", "codex-tree init".green().bold(), "✓".green().bold());
        println!();
        println!(
            "  {:<18} {}",
            "Tree version:".dimmed(),
            version.tree_version
        );
        println!(
            "  {:<18} {}",
            "Files parsed:".dimmed(),
            modules.len()
        );
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
        println!(
            "  {:<18} {}",
            "Tree size:".dimmed(),
            format_size(tree_size)
        );
        println!(
            "  {:<18} {}",
            "Location:".dimmed(),
            output_dir.display()
        );
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

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Walk up the directory tree from `start` looking for a `.git` directory.
/// Returns the directory that *contains* `.git` (i.e. the repo root).
fn find_git_root(start: &Path) -> Option<PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        if current.join(".git").exists() {
            return Some(current);
        }
        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => return None,
        }
    }
}

/// Format a byte count as a human-readable string: B, KB, or MB.
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1_024;
    const MB: u64 = 1_024 * KB;
    if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Recursively sum the sizes of every file under `path`.
fn calculate_dir_size(path: &Path) -> std::io::Result<u64> {
    let mut total: u64 = 0;
    for entry in walkdir::WalkDir::new(path) {
        let entry = entry.map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
        })?;
        if entry.file_type().is_file() {
            total += entry.metadata()?.len();
        }
    }
    Ok(total)
}
