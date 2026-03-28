use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::Result;
use crate::types::{ModuleIndex, TreeStats, TreeStructure, TreeVersion, TreeVersionNumber};

// ── Public API ────────────────────────────────────────────────────────────────

/// Write the full codex-tree output layout under `output_dir`.
///
/// Directory layout created:
/// ```text
/// {output_dir}/
///   version.json
///   tree.json
///   modules/{relative/path}/index.json   (one per ModuleIndex)
///   deltas/          (empty, reserved for future incremental deltas)
///   intent/          (empty, reserved for intent declarations)
///   claude/          (empty, reserved for Claude-specific metadata)
/// ```
pub fn write_tree(
    output_dir: &Path,
    tree: &TreeStructure,
    modules: &[ModuleIndex],
    version: &TreeVersion,
) -> Result<()> {
    // ── Top-level directories ─────────────────────────────────────────────────
    fs::create_dir_all(output_dir)?;
    fs::create_dir_all(output_dir.join("deltas"))?;
    fs::create_dir_all(output_dir.join("intent"))?;
    fs::create_dir_all(output_dir.join("claude"))?;

    // ── version.json ──────────────────────────────────────────────────────────
    let version_json = serde_json::to_string_pretty(version)?;
    fs::write(output_dir.join("version.json"), version_json)?;

    // ── tree.json ─────────────────────────────────────────────────────────────
    let tree_json = serde_json::to_string_pretty(tree)?;
    fs::write(output_dir.join("tree.json"), tree_json)?;

    // ── modules/{path}/index.json ─────────────────────────────────────────────
    let modules_dir = output_dir.join("modules");
    for module in modules {
        // Convert the module's repo-relative path to an OS path fragment so
        // that `src/lib.rs` becomes `modules/src/lib.rs/index.json`.
        let module_dir = modules_dir.join(&module.path);
        fs::create_dir_all(&module_dir)?;
        let module_json = serde_json::to_string_pretty(module)?;
        fs::write(module_dir.join("index.json"), module_json)?;
    }

    Ok(())
}

/// Construct the initial `TreeVersion` document for a fresh `codex-tree init`.
///
/// `generated_at` is set to the current UTC time formatted as ISO-8601.
/// `source_commit` is `None` — the CLI layer fills this in after resolving
/// the git HEAD commit.
pub fn create_initial_version(stats: &TreeStats) -> TreeVersion {
    TreeVersion {
        format_version: "0.1.0".to_string(),
        tree_version: TreeVersionNumber {
            generation: 1,
            delta_count: 0,
        },
        generated_at: current_utc_iso8601(),
        generator: format!("codex-tree {}", env!("CARGO_PKG_VERSION")),
        source_commit: None,
        source_commit_date: None,
        stats: stats.clone(),
    }
}

/// Derive aggregate `TreeStats` from an already-built tree + module list.
///
/// `delta_size_bytes` is set to `0`; the CLI layer can update this after
/// writing the output directory if it wants an accurate on-disk size.
pub fn compute_stats(tree: &TreeStructure, modules: &[ModuleIndex]) -> TreeStats {
    let total_files = modules.len();

    let total_symbols: usize = modules.iter().map(|m| m.symbols.len()).sum();

    // Line count proxy: end line of last symbol in each file.
    let total_lines_of_code: usize = modules
        .iter()
        .map(|m| m.symbols.iter().map(|s| s.span.end_line).max().unwrap_or(0))
        .sum();

    // Collect unique language IDs in sorted order.
    let mut language_set: HashSet<&str> = HashSet::new();
    for module in modules {
        language_set.insert(&module.language);
    }
    // Also honour entries already recorded in the tree (future-proofing).
    let _ = tree; // tree is available but language is already captured via modules
    let mut languages: Vec<String> = language_set.iter().map(|s| s.to_string()).collect();
    languages.sort();

    TreeStats {
        total_files,
        total_symbols,
        total_lines_of_code,
        languages,
        delta_size_bytes: 0,
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Return the current UTC time as an ISO-8601 string, e.g. `"2026-03-27T14:00:00Z"`.
///
/// We derive this from `std::time::SystemTime` to avoid adding a `chrono`
/// dependency to this crate.
fn current_utc_iso8601() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Calendar arithmetic from Unix timestamp (no leap-second correction).
    let (year, month, day, hour, minute, second) = unix_secs_to_datetime(secs);

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hour, minute, second
    )
}

/// Convert a Unix timestamp (seconds) to `(year, month, day, hour, min, sec)`.
///
/// Uses the proleptic Gregorian calendar algorithm so it works beyond 2038
/// without 32-bit overflow.
fn unix_secs_to_datetime(secs: u64) -> (u64, u64, u64, u64, u64, u64) {
    let second = secs % 60;
    let minutes = secs / 60;
    let minute = minutes % 60;
    let hours = minutes / 60;
    let hour = hours % 24;
    let days = hours / 24; // days since 1970-01-01

    // Julian Day Number for 1970-01-01 is 2440588.
    let jd = days + 2_440_588;

    // Richards' algorithm (from the "Calendrical Calculations" book).
    let f = jd + 1401 + (((4 * jd + 274_277) / 146_097) * 3) / 4 - 38;
    let e = 4 * f + 3;
    let g = (e % 1461) / 4;
    let h = 5 * g + 2;

    let day = (h % 153) / 5 + 1;
    let month = (h / 153 + 2) % 12 + 1;
    let year = e / 1461 - 4716 + (14 - month) / 12;

    (year, month, day, hour, minute, second)
}
