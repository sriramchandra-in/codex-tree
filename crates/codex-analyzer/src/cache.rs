/// Content-hash-based caching for intent analysis results.
///
/// Caching avoids redundant API calls when the source code hasn't changed.
/// The cache key is derived from the SHA-256 of each module's `content_hash`,
/// so any file change produces a different key — analogous to a content-
/// addressed store in Scala/Nix terms.
///
/// Cache files live at:
///   `{codex_tree_dir}/intent/.cache/{key}.json`
use std::fs;
use std::path::Path;

use sha2::{Digest, Sha256};

use codex_parser::types::ModuleIndex;

use crate::types::IntentOutput;

// ── Cache key ─────────────────────────────────────────────────────────────────

/// Compute a deterministic cache key for a set of modules.
///
/// Algorithm:
/// 1. Collect all `content_hash` values.
/// 2. Sort them (so key is order-independent — module order may vary).
/// 3. Join with `|` as a separator.
/// 4. SHA-256 hash the joined string.
/// 5. Return lowercase hex.
///
/// In Scala: `modules.map(_.contentHash).sorted.mkString("|")` then hash.
pub fn cache_key(modules: &[ModuleIndex]) -> String {
    let mut hashes: Vec<&str> = modules.iter().map(|m| m.content_hash.as_str()).collect();
    hashes.sort_unstable();

    let combined = hashes.join("|");

    let mut hasher = Sha256::new();
    hasher.update(combined.as_bytes());
    let result = hasher.finalize();

    // Format each byte as two lowercase hex digits.
    result.iter().map(|b| format!("{:02x}", b)).collect()
}

// ── Load ──────────────────────────────────────────────────────────────────────

/// Try to load a cached `IntentOutput` for the given key.
///
/// Returns `Some(output)` on a cache hit, `None` on any failure (miss, corrupt
/// JSON, missing file) — the caller always has a safe fallback.
///
/// Path: `{codex_tree_dir}/intent/.cache/{key}.json`
pub fn load_cached(codex_tree_dir: &Path, key: &str) -> Option<IntentOutput> {
    let path = cache_path(codex_tree_dir, key);
    let bytes = fs::read(&path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

// ── Save ──────────────────────────────────────────────────────────────────────

/// Persist an `IntentOutput` to the cache.
///
/// Creates parent directories if they don't exist — analogous to
/// `Files.createDirectories` in Scala's `java.nio`.
///
/// Failures are propagated as `std::io::Error`; the caller typically ignores
/// cache write errors (a failed write just means next run is a cache miss).
///
/// Path: `{codex_tree_dir}/intent/.cache/{key}.json`
pub fn save_to_cache(
    codex_tree_dir: &Path,
    key: &str,
    output: &IntentOutput,
) -> std::io::Result<()> {
    let path = cache_path(codex_tree_dir, key);

    // Ensure parent directory exists.
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let json = serde_json::to_string_pretty(output)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    fs::write(&path, json)?;
    Ok(())
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Resolve the full filesystem path for a cache entry.
fn cache_path(codex_tree_dir: &Path, key: &str) -> std::path::PathBuf {
    codex_tree_dir
        .join("intent")
        .join(".cache")
        .join(format!("{}.json", key))
}
