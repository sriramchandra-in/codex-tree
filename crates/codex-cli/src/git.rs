/// Git integration utilities.
///
/// All operations shell out to the `git` binary found on `PATH`.  They take
/// `repo_root` as a `&Path` so they can be called from any working directory —
/// the `current_dir` is always set explicitly rather than relying on the
/// process-level cwd.
use std::path::Path;
use std::process::Command;

use crate::error::{CliError, Result};

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Run a git subcommand, return its trimmed stdout as a `String`.
///
/// Returns `Err(CliError::Git)` if the process fails or exits non-zero.
fn run_git(repo_root: &Path, args: &[&str], hint: &str) -> Result<String> {
    let output = Command::new("git")
        .current_dir(repo_root)
        .args(args)
        .output()
        .map_err(|e| CliError::Git {
            message: format!("failed to spawn git: {e}"),
            hint: hint.to_string(),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(CliError::Git {
            message: format!("git {} failed: {}", args.join(" "), stderr),
            hint: hint.to_string(),
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Return the full SHA-1 of the current HEAD commit.
pub fn get_head_commit(repo_root: &Path) -> Result<String> {
    run_git(
        repo_root,
        &["rev-parse", "HEAD"],
        "Ensure the repository has at least one commit.",
    )
}

/// Return the list of files changed between `since_commit` and HEAD.
///
/// Paths are repo-relative and use forward slashes.
pub fn get_changed_files(repo_root: &Path, since_commit: &str) -> Result<Vec<String>> {
    let range = format!("{}..HEAD", since_commit);
    let out = run_git(
        repo_root,
        &["diff", "--name-only", &range],
        "Ensure the commit SHA stored in version.json is reachable.",
    )?;

    Ok(out
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.replace('\\', "/"))
        .collect())
}

/// Return the list of files with uncommitted changes (modified or added).
///
/// Uses `git status --porcelain`; each output line starts with two-char status
/// codes.  We collect any file that appears as modified (`M`), added (`A`),
/// renamed (`R`), copied (`C`), or untracked (`?`).
pub fn get_uncommitted_changes(repo_root: &Path) -> Result<Vec<String>> {
    let out = run_git(
        repo_root,
        &["status", "--porcelain"],
        "Run inside a git repository with a valid working tree.",
    )?;

    let mut files = Vec::new();
    for line in out.lines() {
        if line.len() < 3 {
            continue;
        }
        let xy = &line[..2];
        let rest = line[3..].trim();

        // Status codes we care about: staged (index) + unstaged (working-tree).
        let index_status = xy.chars().next().unwrap_or(' ');
        let wt_status = xy.chars().nth(1).unwrap_or(' ');

        if matches!(index_status, 'M' | 'A' | 'R' | 'C' | 'D')
            || matches!(wt_status, 'M' | 'A' | 'D' | '?')
        {
            // For renames the path may be "old -> new"; take the new name.
            let path = if let Some(arrow_pos) = rest.find(" -> ") {
                &rest[arrow_pos + 4..]
            } else {
                rest
            };
            // Strip surrounding quotes that git adds for paths with spaces.
            let path = path.trim_matches('"').replace('\\', "/");
            if !path.is_empty() {
                files.push(path);
            }
        }
    }
    Ok(files)
}

/// Returns `true` if the repository is a shallow clone.
///
/// Git stores shallow-clone metadata in `.git/shallow`.
pub fn is_shallow_clone(repo_root: &Path) -> bool {
    repo_root.join(".git").join("shallow").exists()
}

/// Return the ISO-8601 commit date for the given commit SHA.
pub fn get_commit_date(repo_root: &Path, commit: &str) -> Result<String> {
    run_git(
        repo_root,
        &["show", "-s", "--format=%cI", commit],
        "Ensure the commit SHA is present in the local git history.",
    )
}
