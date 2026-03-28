/// The `report` command — display tree statistics and token savings estimates.
///
/// Shows how much context the tree provides compared to raw source scanning,
/// giving users and AI agents a sense of the tree's value.
use std::fs;
use std::path::Path;

use colored::Colorize;

use codex_parser::types::{ModuleIndex, TreeVersion};

use crate::error::{CliError, Result};
use crate::utils::{calculate_dir_size, find_git_root, format_size, read_all_modules};

// ── Output types ─────────────────────────────────────────────────────────────

#[derive(serde::Serialize)]
struct Report {
    tree_version: String,
    generated_at: String,
    source_commit: Option<String>,
    stats: ReportStats,
    token_estimate: TokenEstimate,
}

#[derive(serde::Serialize)]
struct ReportStats {
    total_files: usize,
    total_symbols: usize,
    total_lines_of_code: usize,
    languages: Vec<String>,
    tree_size_bytes: u64,
}

#[derive(serde::Serialize)]
struct TokenEstimate {
    /// Estimated tokens to read all raw source files (no codex-tree).
    raw_source_tokens: usize,
    /// `tree.json` + all `modules/**/index.json` (structural tree only).
    just_tree_tokens: usize,
    /// Structural tree + `.codex-tree/cursor/l2.md` (typical Cursor attach).
    tree_plus_cursor_tokens: usize,
    /// Which digest file was added for `tree_plus_cursor_tokens`, if any.
    cursor_digest_used: Option<String>,
    /// Estimated tokens for all module indexes only (legacy / L3 indexes).
    tree_tokens: usize,
    /// Estimated tokens for `tree.json` only (overview).
    tree_l1_tokens: usize,
    /// Savings: 1 - (just_tree_tokens / raw_source_tokens).
    savings_just_tree: f64,
    /// Savings: 1 - (tree_plus_cursor_tokens / raw_source_tokens).
    savings_tree_plus_cursor: f64,
    /// Legacy: modules-only vs raw (same as older `savings_ratio`).
    savings_ratio: f64,
}

// ── Public entry point ───────────────────────────────────────────────────────

pub fn run(path: &Path, format: &str, _benchmark: bool, _verbose: bool, quiet: bool) -> Result<()> {
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

    // ── Read version and modules ─────────────────────────────────────────────
    let version: TreeVersion =
        serde_json::from_str(&fs::read_to_string(codex_tree_dir.join("version.json"))?)?;

    let modules = read_all_modules(&codex_tree_dir)?;
    let tree_size = calculate_dir_size(&codex_tree_dir).unwrap_or(0);

    // ── Compute token estimates ──────────────────────────────────────────────
    let token_estimate = estimate_tokens(&git_root, &codex_tree_dir, &modules)?;

    let report = Report {
        tree_version: version.tree_version.to_string(),
        generated_at: version.generated_at.clone(),
        source_commit: version.source_commit.clone(),
        stats: ReportStats {
            total_files: version.stats.total_files,
            total_symbols: version.stats.total_symbols,
            total_lines_of_code: version.stats.total_lines_of_code,
            languages: version.stats.languages.clone(),
            tree_size_bytes: tree_size,
        },
        token_estimate,
    };

    // ── Output ───────────────────────────────────────────────────────────────
    if format == "json" {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else if !quiet {
        println!();
        println!("  {}", "codex-tree report".green().bold());
        println!();
        println!("  {:<26} {}", "Tree version:".dimmed(), report.tree_version);
        println!("  {:<26} {}", "Generated at:".dimmed(), report.generated_at);
        if let Some(ref sc) = report.source_commit {
            println!(
                "  {:<26} {}",
                "Source commit:".dimmed(),
                &sc[..sc.len().min(12)]
            );
        }
        println!();
        println!("  {}", "Statistics".bold());
        println!("  {:<26} {}", "  Files:".dimmed(), report.stats.total_files);
        println!(
            "  {:<26} {}",
            "  Symbols:".dimmed(),
            report.stats.total_symbols
        );
        println!(
            "  {:<26} {}",
            "  Lines of code:".dimmed(),
            report.stats.total_lines_of_code
        );
        println!(
            "  {:<26} {}",
            "  Languages:".dimmed(),
            report.stats.languages.join(", ")
        );
        println!(
            "  {:<26} {}",
            "  Tree size:".dimmed(),
            format_size(report.stats.tree_size_bytes)
        );
        println!();
        println!("  {}", "Context strategies (estimated tokens)".bold());
        let raw = report.token_estimate.raw_source_tokens.max(1);
        print_strategy_row(
            "  Nothing (raw source only)",
            report.token_estimate.raw_source_tokens,
            raw,
        );
        print_strategy_row(
            "  Just tree (JSON indexes)",
            report.token_estimate.just_tree_tokens,
            raw,
        );
        let tree_cursor_label = match &report.token_estimate.cursor_digest_used {
            Some(p) => format!("  Tree + Cursor (JSON + {})", p),
            None => "  Tree + Cursor (no digest on disk)".to_string(),
        };
        print_strategy_row(
            &tree_cursor_label,
            report.token_estimate.tree_plus_cursor_tokens,
            raw,
        );
        if let Some(ref path) = report.token_estimate.cursor_digest_used {
            println!(
                "  {:<40} {}",
                "".dimmed(),
                format!("(digest: {})", path).dimmed()
            );
        } else if report.token_estimate.tree_plus_cursor_tokens
            == report.token_estimate.just_tree_tokens
        {
            println!(
                "  {:<40} {}",
                "".dimmed(),
                "(no cursor/l2.md — same as tree-only)".dimmed()
            );
        }
        println!();
        println!("  {}", "Detail (bytes→tokens heuristics)".bold());
        println!(
            "  {:<26} {} tokens",
            "  Modules JSON only:".dimmed(),
            format_number(report.token_estimate.tree_tokens)
        );
        println!(
            "  {:<26} {} tokens",
            "  tree.json only:".dimmed(),
            format_number(report.token_estimate.tree_l1_tokens)
        );
        println!(
            "  {:<26} {:.0}%",
            "  Savings (modules vs raw):".dimmed(),
            report.token_estimate.savings_ratio * 100.0
        );
        println!();
    }

    Ok(())
}

fn print_strategy_row(label: &str, tokens: usize, raw_tokens: usize) {
    let pct = if raw_tokens > 0 {
        (1.0 - (tokens as f64 / raw_tokens as f64)) * 100.0
    } else {
        0.0
    };
    println!(
        "  {:<38} {:>8} tokens  {}",
        label.dimmed(),
        format_number(tokens),
        format!("{:.0}% vs raw", pct).dimmed(),
    );
}

// ── Token estimation ─────────────────────────────────────────────────────────

/// Estimate token counts for raw source vs tree representations.
///
/// The scaffolding reads all file sizes and computes the byte totals.
/// The core decision is how to convert bytes → tokens for each content type.
fn estimate_tokens(
    git_root: &Path,
    codex_tree_dir: &Path,
    modules: &[ModuleIndex],
) -> Result<TokenEstimate> {
    // Raw source: sum of all source file sizes
    let mut raw_bytes: usize = 0;
    for module in modules {
        let src_path = git_root.join(&module.path);
        if let Ok(meta) = fs::metadata(&src_path) {
            raw_bytes += meta.len() as usize;
        }
    }

    // Tree JSON: sum of all module index.json sizes + tree.json
    let tree_json_size = fs::metadata(codex_tree_dir.join("tree.json"))
        .map(|m| m.len() as usize)
        .unwrap_or(0);

    let mut module_json_bytes: usize = 0;
    for module in modules {
        let index_path = codex_tree_dir
            .join("modules")
            .join(&module.path)
            .join("index.json");
        if let Ok(meta) = fs::metadata(&index_path) {
            module_json_bytes += meta.len() as usize;
        }
    }

    let raw_source_tokens = bytes_to_tokens(raw_bytes, ContentKind::Code);
    let tree_tokens = bytes_to_tokens(module_json_bytes, ContentKind::Json);
    let tree_l1_tokens = bytes_to_tokens(tree_json_size, ContentKind::Json);

    let just_tree_bytes = tree_json_size.saturating_add(module_json_bytes);
    let just_tree_tokens = bytes_to_tokens(just_tree_bytes, ContentKind::Json);

    let (cursor_digest_used, cursor_extra_tokens) = cursor_digest_token_delta(codex_tree_dir);
    let tree_plus_cursor_tokens = just_tree_tokens.saturating_add(cursor_extra_tokens);

    let savings_ratio = if raw_source_tokens > 0 {
        1.0 - (tree_tokens as f64 / raw_source_tokens as f64)
    } else {
        0.0
    };
    let savings_just_tree = if raw_source_tokens > 0 {
        1.0 - (just_tree_tokens as f64 / raw_source_tokens as f64)
    } else {
        0.0
    };
    let savings_tree_plus_cursor = if raw_source_tokens > 0 {
        1.0 - (tree_plus_cursor_tokens as f64 / raw_source_tokens as f64)
    } else {
        0.0
    };

    Ok(TokenEstimate {
        raw_source_tokens,
        just_tree_tokens,
        tree_plus_cursor_tokens,
        cursor_digest_used,
        tree_tokens,
        tree_l1_tokens,
        savings_just_tree,
        savings_tree_plus_cursor,
        savings_ratio,
    })
}

/// Prefer `cursor/l2.md`, then `l1.md`; return (relative path for display, extra tokens).
fn cursor_digest_token_delta(codex_tree_dir: &Path) -> (Option<String>, usize) {
    for rel in ["cursor/l2.md", "cursor/l1.md"] {
        let p = codex_tree_dir.join(rel);
        if let Ok(meta) = fs::metadata(&p) {
            let bytes = meta.len() as usize;
            return (
                Some(rel.to_string()),
                bytes_to_tokens(bytes, ContentKind::Markdown),
            );
        }
    }
    (None, 0)
}

// ── Token estimation helpers ────────────────────────────────────────────────

enum ContentKind {
    Code,
    Json,
    Markdown,
}

/// Convert a byte count to an estimated token count using content-aware ratios.
///
/// BPE tokenizers (like Claude's) split text into subword units. The average
/// bytes-per-token ratio varies by content type:
///
/// - **Code** (~3.5 bytes/token): identifiers get split at camelCase/snake_case
///   boundaries, but keywords and operators are usually single tokens. Indentation
///   whitespace compresses well (one token per indent level regardless of spaces).
///
/// - **JSON** (~3.0 bytes/token): more token-dense because of repeated structural
///   characters (`"`, `{`, `}`, `:`, `,`) that each consume a token, and quoted
///   keys that tokenize less efficiently than bare identifiers.
///
/// - **Markdown** (~3.3 bytes/token): mix of prose, headings, and code fences.
fn bytes_to_tokens(bytes: usize, kind: ContentKind) -> usize {
    let ratio = match kind {
        ContentKind::Code => 3.5,
        ContentKind::Json => 3.0,
        ContentKind::Markdown => 3.3,
    };
    (bytes as f64 / ratio).round() as usize
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn format_number(n: usize) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}
