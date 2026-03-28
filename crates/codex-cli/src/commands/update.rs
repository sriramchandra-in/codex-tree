/// The `update` command — apply an incremental delta from git changes.
///
/// Workflow:
/// 1. Find git root, verify `.codex-tree/` exists.
/// 2. Read `version.json` to get `source_commit`.
/// 3. Determine changed files via git diff (or full scan when no commit is recorded).
/// 4. For each changed file, produce a `DeltaOperation`.
/// 5. Write `deltas/{sequence}.json`.
/// 6. Update `version.json`.
/// 7. Optionally compact if thresholds are exceeded.
use std::fs;
use std::path::Path;

use colored::Colorize;

use codex_parser::diff::diff_modules;
use codex_parser::parser::parse_file;
use codex_parser::registry::ParserRegistry;
use codex_parser::serializer::compute_stats;
use codex_parser::types::{Delta, DeltaOperation, ModuleIndex, TreeStructure, TreeVersion};

use crate::compaction::{calculate_delta_size, compact, should_compact};
use crate::error::{CliError, Result};
use crate::git::{get_changed_files, get_commit_date, get_head_commit, get_uncommitted_changes};
use crate::utils::{find_git_root, read_all_modules, current_utc_iso8601};

// ── Public entry point ────────────────────────────────────────────────────────

pub fn run(
    path: &Path,
    no_intent: bool,
    no_claude: bool,
    no_compact: bool,
    verbose: bool,
    quiet: bool,
) -> Result<()> {
    // ── 1. Resolve path and find git root ─────────────────────────────────────
    let abs_path = std::fs::canonicalize(path).map_err(|e| {
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
            path: codex_tree_dir.clone(),
        });
    }

    // ── 2. Read version.json ──────────────────────────────────────────────────
    let version_path = codex_tree_dir.join("version.json");
    let version_json = fs::read_to_string(&version_path)?;
    let mut version: TreeVersion = serde_json::from_str(&version_json)?;

    // ── 3. Determine changed files ────────────────────────────────────────────
    let head_commit = get_head_commit(&git_root)?;

    let changed_files: Vec<String> = if let Some(ref source_commit) = version.source_commit {
        if source_commit == &head_commit {
            // Also check for uncommitted changes.
            get_uncommitted_changes(&git_root)?
        } else {
            // Committed changes since last tree state.
            let mut files = get_changed_files(&git_root, source_commit)?;
            // Also include uncommitted changes.
            let uncommitted = get_uncommitted_changes(&git_root)?;
            for f in uncommitted {
                if !files.contains(&f) {
                    files.push(f);
                }
            }
            files
        }
    } else {
        // No source_commit: we need to detect changes via content hash comparison.
        detect_changed_by_hash(&codex_tree_dir, &git_root)?
    };

    if changed_files.is_empty() {
        if !quiet {
            println!("  Tree is up to date.");
        }
        return Ok(());
    }

    if !quiet {
        println!("{}", "  Computing delta...".dimmed());
    }

    // ── 4. Build registry ─────────────────────────────────────────────────────
    let registry = ParserRegistry::with_defaults();

    // ── 5. For each changed file, produce a DeltaOperation ───────────────────
    let mut operations: Vec<DeltaOperation> = Vec::new();

    for rel_path in &changed_files {
        let abs_file = git_root.join(rel_path);

        if !abs_file.exists() {
            // File was deleted.
            operations.push(DeltaOperation::Remove {
                path: rel_path.clone(),
            });
            continue;
        }

        let source = match fs::read(&abs_file) {
            Ok(b) => b,
            Err(_) => continue,
        };

        // Attempt to parse the file.
        let new_module = match parse_file(Path::new(rel_path), &source, &registry) {
            Ok(mut m) => {
                m.path = rel_path.clone();
                m
            }
            Err(codex_parser::error::ParserError::UnsupportedLanguage(_)) => continue,
            Err(e) => return Err(e.into()),
        };

        // Check whether we have an old module index on disk.
        let old_index_path = codex_tree_dir.join("modules").join(rel_path).join("index.json");

        if old_index_path.exists() {
            // Modified file: diff old vs new.
            let old_json = fs::read_to_string(&old_index_path)?;
            let old_module: ModuleIndex = serde_json::from_str(&old_json)?;
            let op = diff_modules(&old_module, &new_module);
            // Update the module index on disk now (before potential compaction).
            update_module_on_disk(&codex_tree_dir, &new_module)?;
            operations.push(op);
        } else {
            // New file.
            update_module_on_disk(&codex_tree_dir, &new_module)?;
            operations.push(DeltaOperation::Add {
                path: rel_path.clone(),
                module_index: new_module,
            });
        }
    }

    if operations.is_empty() {
        if !quiet {
            println!("  Tree is up to date (no parseable files changed).");
        }
        return Ok(());
    }

    // ── 6. Build Delta struct ─────────────────────────────────────────────────
    let next_sequence = version.tree_version.delta_count + 1;
    let commit_date = get_commit_date(&git_root, &head_commit).ok();

    let delta = Delta {
        sequence: next_sequence,
        timestamp: current_utc_iso8601(),
        source_commit: Some(head_commit.clone()),
        changed_files: changed_files.clone(),
        operations,
    };

    // ── 7. Write delta file ───────────────────────────────────────────────────
    let deltas_dir = codex_tree_dir.join("deltas");
    fs::create_dir_all(&deltas_dir)?;
    let delta_path = deltas_dir.join(format!("{}.json", next_sequence));
    fs::write(&delta_path, serde_json::to_string_pretty(&delta)?)?;

    // ── 8. Update version.json ────────────────────────────────────────────────
    version.tree_version.delta_count = next_sequence;
    version.source_commit = Some(head_commit.clone());
    version.source_commit_date = commit_date;
    version.stats.delta_size_bytes = calculate_delta_size(&codex_tree_dir)?;
    // Refresh total_files count from tree.json.
    if let Ok(tree_json) = fs::read_to_string(codex_tree_dir.join("tree.json")) {
        if let Ok(tree) = serde_json::from_str::<TreeStructure>(&tree_json) {
            // Recompute stats from updated modules on disk.
            let all_modules = read_all_modules(&codex_tree_dir)?;
            let new_stats = compute_stats(&tree, &all_modules);
            version.stats.total_files = new_stats.total_files;
            version.stats.total_symbols = new_stats.total_symbols;
            version.stats.total_lines_of_code = new_stats.total_lines_of_code;
            version.stats.languages = new_stats.languages;
            // Keep delta_size_bytes as computed above.
        }
    }

    fs::write(&version_path, serde_json::to_string_pretty(&version)?)?;

    // ── 9. Compact if needed ──────────────────────────────────────────────────
    let did_compact = if !no_compact && should_compact(&version) {
        compact(&codex_tree_dir)?;
        true
    } else {
        false
    };

    // ── 9a. Intent layer ─────────────────────────────────────────────────
    let intent_output = if !no_intent {
        // For update, read the full tree to pass to the analyzer.
        let all_modules = read_all_modules(&codex_tree_dir)?;
        match run_intent_analysis(&codex_tree_dir, &all_modules, quiet) {
            Ok(output) => Some(output),
            Err(_) => None,
        }
    } else {
        None
    };

    // ── 9b. Claude optimization layer ────────────────────────────────────
    if !no_claude {
        if let Ok(tree_json) = fs::read_to_string(codex_tree_dir.join("tree.json")) {
            if let Ok(tree) = serde_json::from_str::<TreeStructure>(&tree_json) {
                generate_claude_layer(&codex_tree_dir, &tree, &read_all_modules(&codex_tree_dir)?, &version, intent_output.as_ref(), quiet);
            }
        }
    }

    // ── 10. Print summary ─────────────────────────────────────────────────────
    if !quiet {
        println!();
        println!(
            "  {} {}",
            "codex-tree update".green().bold(),
            "✓".green().bold()
        );
        println!();
        println!(
            "  {:<22} {}",
            "Tree version:".dimmed(),
            version.tree_version
        );
        println!(
            "  {:<22} {}",
            "Files changed:".dimmed(),
            changed_files.len()
        );
        println!(
            "  {:<22} {}",
            "Delta sequence:".dimmed(),
            next_sequence
        );
        if did_compact {
            println!("  {:<22} {}", "Compaction:".dimmed(), "ran (thresholds met)");
        }

        if verbose {
            println!();
            println!("  {}", "Changed files:".dimmed());
            for f in &changed_files {
                println!("    {}", f.cyan());
            }
        }
        println!();
    }

    Ok(())
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Write a `ModuleIndex` to `modules/{path}/index.json`.
fn update_module_on_disk(codex_tree_dir: &Path, module: &ModuleIndex) -> Result<()> {
    let module_dir = codex_tree_dir.join("modules").join(&module.path);
    fs::create_dir_all(&module_dir)?;
    fs::write(
        module_dir.join("index.json"),
        serde_json::to_string_pretty(module)?,
    )?;
    Ok(())
}

/// When there is no `source_commit` in version.json, detect changed files by
/// comparing the `content_hash` in each module index against the current file.
fn detect_changed_by_hash(codex_tree_dir: &Path, git_root: &Path) -> Result<Vec<String>> {
    use sha2::{Digest, Sha256};

    let modules_dir = codex_tree_dir.join("modules");
    let mut changed = Vec::new();

    if !modules_dir.exists() {
        return Ok(changed);
    }

    for entry in walkdir::WalkDir::new(&modules_dir) {
        let entry = entry.map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
        })?;

        if !entry.file_type().is_file() {
            continue;
        }
        if entry.file_name() != "index.json" {
            continue;
        }

        let content = fs::read_to_string(entry.path())?;
        let module: ModuleIndex = match serde_json::from_str(&content) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let src_path = git_root.join(&module.path);
        if !src_path.exists() {
            changed.push(module.path);
            continue;
        }

        let src_bytes = match fs::read(&src_path) {
            Ok(b) => b,
            Err(_) => continue,
        };

        let mut hasher = Sha256::new();
        hasher.update(&src_bytes);
        let current_hash = format!("sha256:{:x}", hasher.finalize());

        if current_hash != module.content_hash {
            changed.push(module.path);
        }
    }

    Ok(changed)
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

