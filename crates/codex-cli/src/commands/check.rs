/// The `check` command — staleness safety valve.
///
/// Compares the tree's recorded state (source_commit + content hashes) against
/// the current working tree. Reports which files are clean (tree is trustworthy)
/// and which are stale (AI should read raw source instead).
///
/// Exit codes:
///   0 — all files are clean, tree is up to date
///   1 — (with --fail-if-stale) at least one file is stale
use std::fs;
use std::path::Path;

use colored::Colorize;
use sha2::{Digest, Sha256};

use codex_parser::types::TreeVersion;

use crate::error::{CliError, Result};
use crate::git;
use crate::utils::{find_git_root, read_all_modules};

// ── Output types ─────────────────────────────────────────────────────────────

#[derive(serde::Serialize)]
struct CheckReport {
    tree_version: String,
    source_commit: Option<String>,
    head_commit: String,
    commits_behind: Option<usize>,
    stale_files: Vec<StaleFile>,
    clean_files: usize,
    is_stale: bool,
}

#[derive(serde::Serialize)]
struct StaleFile {
    path: String,
    reason: StaleReason,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum StaleReason {
    /// File has been modified since the tree was generated.
    Modified,
    /// File exists on disk but has no module index in the tree.
    Untracked,
    /// Module index exists in the tree but file is deleted from disk.
    Deleted,
}

impl std::fmt::Display for StaleReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StaleReason::Modified => write!(f, "modified"),
            StaleReason::Untracked => write!(f, "untracked"),
            StaleReason::Deleted => write!(f, "deleted"),
        }
    }
}

// ── Public entry point ───────────────────────────────────────────────────────

pub fn run(
    path: &Path,
    format: &str,
    fail_if_stale: bool,
    _verbose: bool,
    quiet: bool,
) -> Result<()> {
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

    // ── Read version.json ────────────────────────────────────────────────────
    let version: TreeVersion =
        serde_json::from_str(&fs::read_to_string(codex_tree_dir.join("version.json"))?)?;

    let head_commit = git::get_head_commit(&git_root)?;

    // ── Count commits behind ─────────────────────────────────────────────────
    let commits_behind = version.source_commit.as_ref().and_then(|sc| {
        if sc == &head_commit {
            Some(0)
        } else {
            count_commits_between(&git_root, sc, &head_commit).ok()
        }
    });

    // ── Check each module for staleness ──────────────────────────────────────
    let modules = read_all_modules(&codex_tree_dir)?;
    let mut stale_files: Vec<StaleFile> = Vec::new();
    let mut clean_count: usize = 0;

    for module in &modules {
        let src_path = git_root.join(&module.path);

        if !src_path.exists() {
            stale_files.push(StaleFile {
                path: module.path.clone(),
                reason: StaleReason::Deleted,
            });
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
            stale_files.push(StaleFile {
                path: module.path.clone(),
                reason: StaleReason::Modified,
            });
        } else {
            clean_count += 1;
        }
    }

    // ── Check for new files not in the tree ──────────────────────────────────
    // Use git's uncommitted changes to find newly added files.
    if let Ok(uncommitted) = git::get_uncommitted_changes(&git_root) {
        let known_paths: std::collections::HashSet<&str> =
            modules.iter().map(|m| m.path.as_str()).collect();

        for file in uncommitted {
            if !known_paths.contains(file.as_str())
                && !stale_files.iter().any(|s| s.path == file)
                && has_supported_extension(&file)
            {
                stale_files.push(StaleFile {
                    path: file,
                    reason: StaleReason::Untracked,
                });
            }
        }
    }

    let is_stale = !stale_files.is_empty();

    // ── Build report ─────────────────────────────────────────────────────────
    let report = CheckReport {
        tree_version: version.tree_version.to_string(),
        source_commit: version.source_commit.clone(),
        head_commit: head_commit.clone(),
        commits_behind,
        stale_files,
        clean_files: clean_count,
        is_stale,
    };

    // ── Output ───────────────────────────────────────────────────────────────
    if format == "json" {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else if !quiet {
        println!();
        if report.is_stale {
            println!(
                "  {} {}",
                "codex-tree check".yellow().bold(),
                "stale".yellow().bold()
            );
        } else {
            println!(
                "  {} {}",
                "codex-tree check".green().bold(),
                "clean".green().bold()
            );
        }
        println!();
        println!("  {:<22} {}", "Tree version:".dimmed(), report.tree_version);
        if let Some(ref sc) = report.source_commit {
            println!(
                "  {:<22} {}",
                "Tree commit:".dimmed(),
                &sc[..sc.len().min(12)]
            );
        }
        println!(
            "  {:<22} {}",
            "HEAD commit:".dimmed(),
            &report.head_commit[..report.head_commit.len().min(12)]
        );
        if let Some(behind) = report.commits_behind {
            println!("  {:<22} {}", "Commits behind:".dimmed(), behind);
        }
        println!("  {:<22} {}", "Clean files:".dimmed(), report.clean_files);
        println!(
            "  {:<22} {}",
            "Stale files:".dimmed(),
            report.stale_files.len()
        );

        if !report.stale_files.is_empty() {
            println!();
            println!("  {}", "Stale files:".yellow());
            for sf in &report.stale_files {
                println!("    {} ({})", sf.path.cyan(), sf.reason);
            }
        }
        println!();
    }

    // ── Exit code ────────────────────────────────────────────────────────────
    if fail_if_stale && is_stale {
        std::process::exit(1);
    }

    Ok(())
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Count commits between two SHAs using `git rev-list --count`.
fn count_commits_between(repo_root: &Path, from: &str, to: &str) -> Result<usize> {
    let output = std::process::Command::new("git")
        .current_dir(repo_root)
        .args(["rev-list", "--count", &format!("{}..{}", from, to)])
        .output()
        .map_err(|e| CliError::Git {
            message: format!("failed to count commits: {e}"),
            hint: "Ensure git history is available.".to_string(),
        })?;

    if !output.status.success() {
        return Err(CliError::Git {
            message: "rev-list failed".to_string(),
            hint: "The source_commit in version.json may not be reachable.".to_string(),
        });
    }

    let count_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(count_str.parse::<usize>().unwrap_or(0))
}

/// Quick check if a file path has a supported extension (currently just .rs).
fn has_supported_extension(path: &str) -> bool {
    path.ends_with(".rs")
}
