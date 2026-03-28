use std::fs;
use std::path::{Path, PathBuf};

use codex_parser::types::ModuleIndex;

use crate::error::Result;

/// Walk up the directory tree from `start` looking for a `.git` directory.
/// Returns the directory that *contains* `.git` (i.e. the repo root).
pub fn find_git_root(start: &Path) -> Option<PathBuf> {
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

/// Read all module indexes currently stored on disk.
pub fn read_all_modules(codex_tree_dir: &Path) -> Result<Vec<ModuleIndex>> {
    let modules_dir = codex_tree_dir.join("modules");
    let mut modules = Vec::new();

    if !modules_dir.exists() {
        return Ok(modules);
    }

    for entry in walkdir::WalkDir::new(&modules_dir) {
        let entry = entry.map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
        })?;

        if !entry.file_type().is_file() || entry.file_name() != "index.json" {
            continue;
        }

        let content = fs::read_to_string(entry.path())?;
        if let Ok(module) = serde_json::from_str::<ModuleIndex>(&content) {
            modules.push(module);
        }
    }

    Ok(modules)
}

/// Format a byte count as a human-readable string: B, KB, or MB.
pub fn format_size(bytes: u64) -> String {
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
pub fn calculate_dir_size(path: &Path) -> std::io::Result<u64> {
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

/// Current UTC time as ISO-8601 string.
pub fn current_utc_iso8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let second = secs % 60;
    let minutes = secs / 60;
    let minute = minutes % 60;
    let hours = minutes / 60;
    let hour = hours % 24;
    let days = hours / 24;

    let jd = days + 2_440_588;
    let f = jd + 1401 + (((4 * jd + 274_277) / 146_097) * 3) / 4 - 38;
    let e = 4 * f + 3;
    let g = (e % 1461) / 4;
    let h = 5 * g + 2;

    let day = (h % 153) / 5 + 1;
    let month = (h / 153 + 2) % 12 + 1;
    let year = e / 1461 - 4716 + (14 - month) / 12;

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hour, minute, second
    )
}
