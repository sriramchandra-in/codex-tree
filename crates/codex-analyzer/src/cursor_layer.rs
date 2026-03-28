/// Cursor optimization layer — **aligned** with the Claude layer: L1/L2/L3 bodies are
/// produced by [`crate::claude_layer`] unchanged; this module only prepends the Cursor
/// usage guide. Do not duplicate or diverge structural sections here — extend
/// `claude_layer` if both consumers need new content, or extend the preamble if the
/// guidance is Cursor-only.
///
/// Writes `.codex-tree/cursor/l1.md`, `l2.md`, `l3.md`. **No API calls.**
use std::fmt::Write as FmtWrite;
use std::fs;
use std::path::Path;

use codex_parser::types::{ModuleIndex, TreeStructure, TreeVersion};

use crate::claude_layer;
use crate::types::IntentOutput;

/// Markdown block inserted after the codex-tree metadata header on every level.
///
/// Kept compact so the digest itself remains the bulk of the tokens.
fn cursor_usage_guide() -> &'static str {
    r#"# Cursor: codex-tree digest

These files are the same **structural summary** as `.codex-tree/claude/*.md`, with guidance for **Cursor** (Chat, Composer, Agent).

## Pick a level

| File | Use when |
|------|----------|
| `l1.md` | Quick orientation, small edits, locating entry points |
| `l2.md` | Feature work — public APIs, imports, intent **patterns** |
| `l3.md` | Refactors / architecture — all symbols, decisions, import/export detail |

## Attach in Cursor

- Use **@** → **Files** → `.codex-tree/cursor/l1.md` (or `l2` / `l3`).
- Prefer **l1 first**; escalate only if the task needs more of the module graph or full symbol lists.
- Optional: a [`.cursor/rules`](https://docs.cursor.com/context/rules-for-ai) entry can point agents at `l2.md` as default repo context.

## Beyond this digest

- Per-file indices: `.codex-tree/modules/<path>/index.json`
- Source of truth remains the repository; use search / read tools for code not spelled out here.

## Intent JSON (when generated)

- `intent/patterns.json` and `intent/decisions.json` are rolled into **Design Patterns** (l2+) and **Design Decisions** (l3).
"#
}

/// Inserts the Cursor guide immediately after the HTML metadata comment and
/// following blank lines, then a horizontal rule before the original body.
fn wrap_for_cursor(base: &str) -> String {
    let guide = cursor_usage_guide();
    let lines: Vec<&str> = base.lines().collect();
    if lines.is_empty() {
        return guide.to_string();
    }

    let mut split_at = 0usize;
    if lines
        .first()
        .is_some_and(|l| l.starts_with("<!-- codex-tree:"))
    {
        split_at = 1;
        while split_at < lines.len() && lines[split_at].trim().is_empty() {
            split_at += 1;
        }
    }

    let header = lines[..split_at].join("\n");
    let rest = lines[split_at..].join("\n");

    let mut out = String::new();
    writeln!(out, "{}", header).unwrap();
    writeln!(out).unwrap();
    write!(out, "{}", guide).unwrap();
    writeln!(out).unwrap();
    writeln!(out, "---").unwrap();
    writeln!(out).unwrap();
    write!(out, "{}", rest).unwrap();
    out
}

/// L1 digest with Cursor framing (~500 tokens of structure + compact guide).
pub fn generate_cursor_l1(
    tree: &TreeStructure,
    modules: &[ModuleIndex],
    version: &TreeVersion,
) -> String {
    wrap_for_cursor(&claude_layer::generate_l1(tree, modules, version))
}

pub fn generate_cursor_l2(
    tree: &TreeStructure,
    modules: &[ModuleIndex],
    version: &TreeVersion,
    intent: Option<&IntentOutput>,
) -> String {
    wrap_for_cursor(&claude_layer::generate_l2(tree, modules, version, intent))
}

pub fn generate_cursor_l3(
    tree: &TreeStructure,
    modules: &[ModuleIndex],
    version: &TreeVersion,
    intent: Option<&IntentOutput>,
) -> String {
    wrap_for_cursor(&claude_layer::generate_l3(tree, modules, version, intent))
}

/// Builds all three levels (convenience for callers that want [`crate::types::CursorLayerOutput`]).
pub fn generate_cursor_layer_output(
    tree: &TreeStructure,
    modules: &[ModuleIndex],
    version: &TreeVersion,
    intent: Option<&IntentOutput>,
) -> crate::types::CursorLayerOutput {
    crate::types::CursorLayerOutput {
        l1: generate_cursor_l1(tree, modules, version),
        l2: generate_cursor_l2(tree, modules, version, intent),
        l3: generate_cursor_l3(tree, modules, version, intent),
    }
}

/// Writes `l1.md`, `l2.md`, `l3.md` under `<output_dir>/cursor/`.
pub fn write_cursor_layer(output_dir: &Path, l1: &str, l2: &str, l3: &str) -> std::io::Result<()> {
    let dir = output_dir.join("cursor");
    fs::create_dir_all(&dir)?;
    fs::write(dir.join("l1.md"), l1)?;
    fs::write(dir.join("l2.md"), l2)?;
    fs::write(dir.join("l3.md"), l3)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_parser::types::{
        Export, Span, Symbol, SymbolKind, TreeEntry, TreeStats, TreeStructure, TreeVersion,
        TreeVersionNumber, Visibility,
    };

    fn make_version() -> TreeVersion {
        TreeVersion {
            format_version: "1.0.0".to_string(),
            tree_version: TreeVersionNumber {
                generation: 1,
                delta_count: 0,
            },
            generated_at: "2026-03-28T10:00:00Z".to_string(),
            generator: "codex-tree 0.1.0".to_string(),
            source_commit: None,
            source_commit_date: None,
            stats: TreeStats {
                total_files: 1,
                total_symbols: 1,
                total_lines_of_code: 10,
                languages: vec!["rust".to_string()],
                delta_size_bytes: 0,
            },
        }
    }

    fn make_tree() -> TreeStructure {
        TreeStructure {
            root: "/tmp/proj".to_string(),
            entries: vec![TreeEntry::Directory {
                path: "src".to_string(),
                children_count: 1,
                total_symbol_count: 1,
                total_line_count: 10,
            }],
        }
    }

    fn make_modules() -> Vec<ModuleIndex> {
        vec![ModuleIndex {
            path: "src/lib.rs".to_string(),
            language: "rust".to_string(),
            content_hash: "abc".to_string(),
            symbols: vec![Symbol {
                name: "foo".to_string(),
                kind: SymbolKind::Function,
                visibility: Visibility::Public,
                span: Span {
                    start_line: 1,
                    end_line: 2,
                },
                signature: "pub fn foo()".to_string(),
                parent: None,
                doc_comment: None,
            }],
            imports: vec![],
            exports: vec![Export {
                name: "foo".to_string(),
                kind: SymbolKind::Function,
            }],
        }]
    }

    #[test]
    fn cursor_l1_inserts_guide_and_keeps_overview() {
        let s = generate_cursor_l1(&make_tree(), &make_modules(), &make_version());
        assert!(s.starts_with("<!-- codex-tree:"));
        assert!(s.contains("# Cursor: codex-tree digest"));
        assert!(s.contains("---"));
        assert!(s.contains("# Project Overview"));
    }

    #[test]
    fn write_cursor_layer_creates_files() {
        let dir = tempfile::tempdir().unwrap();
        write_cursor_layer(dir.path(), "a", "b", "c").unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join("cursor/l1.md")).unwrap(),
            "a"
        );
    }
}
