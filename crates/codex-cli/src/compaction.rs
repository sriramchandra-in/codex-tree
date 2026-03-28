/// Compaction logic: merge accumulated deltas into the base tree files.
///
/// Compaction keeps the `.codex-tree/` directory from growing unboundedly by
/// folding all pending deltas into the canonical `modules/` and `tree.json`
/// state, then resetting the delta counter.
use std::fs;
use std::path::Path;

use codex_parser::types::{
    Delta, DeltaOperation, ModuleIndex, TreeEntry, TreeStructure, TreeVersion,
};

use crate::error::Result;

// ── Thresholds ────────────────────────────────────────────────────────────────

const COMPACT_DELTA_COUNT: u32 = 10;
const COMPACT_DELTA_SIZE_BYTES: u64 = 102_400; // 100 KB

// ── Public API ────────────────────────────────────────────────────────────────

/// Returns `true` if compaction should run.
///
/// Triggered when either threshold is exceeded (whichever comes first).
pub fn should_compact(version: &TreeVersion) -> bool {
    version.tree_version.delta_count >= COMPACT_DELTA_COUNT
        || version.stats.delta_size_bytes >= COMPACT_DELTA_SIZE_BYTES
}

/// Read and return all delta files in `deltas/`, sorted by sequence number.
pub fn read_deltas(codex_tree_dir: &Path) -> Result<Vec<Delta>> {
    let deltas_dir = codex_tree_dir.join("deltas");
    if !deltas_dir.exists() {
        return Ok(Vec::new());
    }

    let mut entries: Vec<(u32, Delta)> = Vec::new();

    for entry in fs::read_dir(&deltas_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }

        let content = fs::read_to_string(&path)?;
        let delta: Delta = serde_json::from_str(&content)?;
        entries.push((delta.sequence, delta));
    }

    entries.sort_by_key(|(seq, _)| *seq);
    Ok(entries.into_iter().map(|(_, d)| d).collect())
}

/// Sum the total file sizes of all JSON files inside `deltas/`.
pub fn calculate_delta_size(codex_tree_dir: &Path) -> Result<u64> {
    let deltas_dir = codex_tree_dir.join("deltas");
    if !deltas_dir.exists() {
        return Ok(0);
    }

    let mut total: u64 = 0;
    for entry in fs::read_dir(&deltas_dir)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        if meta.is_file() {
            total += meta.len();
        }
    }
    Ok(total)
}

/// Apply all pending deltas to the canonical tree and reset the delta state.
///
/// Steps:
/// 1. Read `version.json`
/// 2. Read all delta files from `deltas/` in sequence order
/// 3. Apply each operation to `modules/` and `tree.json`
/// 4. Delete all delta files
/// 5. Reset `delta_count` and `delta_size_bytes` in `version.json`
pub fn compact(codex_tree_dir: &Path) -> Result<()> {
    // ── 1. Read current version ───────────────────────────────────────────────
    let version_path = codex_tree_dir.join("version.json");
    let version_json = fs::read_to_string(&version_path)?;
    let mut version: TreeVersion = serde_json::from_str(&version_json)?;

    // ── 2. Read all deltas ────────────────────────────────────────────────────
    let deltas = read_deltas(codex_tree_dir)?;
    if deltas.is_empty() {
        return Ok(());
    }

    // ── 3. Read current tree.json ─────────────────────────────────────────────
    let tree_path = codex_tree_dir.join("tree.json");
    let tree_json = fs::read_to_string(&tree_path)?;
    let mut tree: TreeStructure = serde_json::from_str(&tree_json)?;

    // ── 4. Apply each delta operation ─────────────────────────────────────────
    for delta in &deltas {
        for op in &delta.operations {
            apply_operation(codex_tree_dir, op, &mut tree)?;
        }
    }

    // ── 5. Delete all delta files ─────────────────────────────────────────────
    let deltas_dir = codex_tree_dir.join("deltas");
    for entry in fs::read_dir(&deltas_dir)? {
        let entry = entry?;
        if entry.path().extension().and_then(|e| e.to_str()) == Some("json") {
            fs::remove_file(entry.path())?;
        }
    }

    // ── 6. Reset delta counters ───────────────────────────────────────────────
    version.tree_version.delta_count = 0;
    version.stats.delta_size_bytes = 0;

    // ── 7. Write updated files ────────────────────────────────────────────────
    fs::write(&version_path, serde_json::to_string_pretty(&version)?)?;
    fs::write(&tree_path, serde_json::to_string_pretty(&tree)?)?;

    Ok(())
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Apply a single `DeltaOperation` to the on-disk tree state.
fn apply_operation(
    codex_tree_dir: &Path,
    op: &DeltaOperation,
    tree: &mut TreeStructure,
) -> Result<()> {
    let modules_dir = codex_tree_dir.join("modules");

    match op {
        DeltaOperation::Add { path, module_index } => {
            // Write the new module index.
            let module_dir = modules_dir.join(path);
            fs::create_dir_all(&module_dir)?;
            fs::write(
                module_dir.join("index.json"),
                serde_json::to_string_pretty(module_index)?,
            )?;

            // Add to tree.json (replace if already present, else append).
            upsert_tree_entry(tree, module_index);
        }

        DeltaOperation::Modify { path, .. } => {
            // Read the current module index and update it.
            let index_path = modules_dir.join(path).join("index.json");
            if index_path.exists() {
                let content = fs::read_to_string(&index_path)?;
                let module: ModuleIndex = serde_json::from_str(&content)?;
                // The module itself is already updated on-disk by the update
                // command; here during compaction we just need to keep tree.json
                // consistent.
                upsert_tree_entry(tree, &module);
            }
        }

        DeltaOperation::Remove { path } => {
            // Remove the module directory.
            let module_dir = modules_dir.join(path);
            if module_dir.exists() {
                fs::remove_dir_all(module_dir)?;
            }

            // Remove from tree.json.
            tree.entries.retain(|entry| match entry {
                TreeEntry::File { path: p, .. } => p != path,
                TreeEntry::Directory { .. } => true,
            });
        }
    }

    Ok(())
}

/// Insert or replace the `TreeEntry::File` for a given `ModuleIndex`.
fn upsert_tree_entry(tree: &mut TreeStructure, module: &ModuleIndex) {
    let line_count = module
        .symbols
        .iter()
        .map(|s| s.span.end_line)
        .max()
        .unwrap_or(0);

    let new_entry = TreeEntry::File {
        path: module.path.clone(),
        language: module.language.clone(),
        symbol_count: module.symbols.len(),
        line_count,
        imports_count: module.imports.len(),
        exports_count: module.exports.len(),
    };

    // Replace an existing entry for the same path, or append.
    let existing = tree.entries.iter_mut().find(|e| match e {
        TreeEntry::File { path, .. } => *path == module.path,
        _ => false,
    });

    if let Some(slot) = existing {
        *slot = new_entry;
    } else {
        tree.entries.push(new_entry);
    }
}
