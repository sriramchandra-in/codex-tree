/// Prompt templates for the intent analysis phase.
///
/// In Scala terms, these are pure functions that produce `String` — no side
/// effects, no allocation beyond the returned value.  They are deliberately
/// separated from the HTTP client so they are easy to unit-test and iterate on
/// without making real API calls.
use codex_parser::types::ModuleIndex;

// ── Token budget ──────────────────────────────────────────────────────────────

/// Approximate character limit for the user prompt.
///
/// Claude's context window is far larger, but we cap at ~30 K characters to
/// keep latency and cost predictable.  Characters ≈ tokens * 4 for English
/// prose; code is denser, so 30 K chars ≈ 7–10 K tokens.
const MAX_USER_PROMPT_CHARS: usize = 30_000;

// ── System prompt ─────────────────────────────────────────────────────────────

/// Return the system prompt instructing Claude to produce intent JSON.
///
/// The prompt embeds the target JSON schema inline so Claude knows exactly
/// what structure to emit — analogous to passing a Scala `Decoder` definition
/// as documentation.
pub fn intent_system_prompt() -> String {
    r#"You are an expert code analyst. Your task is to analyse the structure of a software project and produce a structured JSON document capturing design decisions and recurring patterns.

## Output format

Respond with **only** a JSON object — no prose, no markdown, no explanation before or after. Wrap the JSON in a ```json code fence.

Schema:
```json
{
  "decisions": [
    {
      "id": "d001",
      "anchored_to": {
        "file": "src/lib.rs",
        "symbol": "MyStruct"
      },
      "summary": "One sentence: what decision was made.",
      "rationale": "Why — the trade-off or constraint that drove the decision.",
      "confidence": 0.85,
      "provenance": "inferred_from_code"
    }
  ],
  "patterns": [
    {
      "id": "p001",
      "name": "Pattern name",
      "description": "What the pattern is and how it is applied.",
      "applies_to": ["src/foo.rs", "src/bar.rs"],
      "confidence": 0.9,
      "provenance": "inferred_from_code"
    }
  ]
}
```

## Field definitions

**decisions** — architectural or design choices anchored to a specific file/symbol.
- `id` — sequential identifier, e.g. "d001", "d002".
- `anchored_to.file` — repo-relative path of the most relevant file.
- `anchored_to.symbol` — optional: the specific struct/fn/trait name.
- `summary` — one short sentence describing the decision.
- `rationale` — the "why": trade-offs, constraints, goals.
- `confidence` — float 0.0–1.0:
    1.0       = extracted from explicit documentation
    0.7–0.9   = inferred from code with high certainty
    0.4–0.6   = multiple interpretations possible
    0.1–0.3   = speculative
- `provenance` — one of: `extracted_from_docs`, `extracted_from_commit`, `inferred_from_code`

**patterns** — cross-cutting conventions observed across multiple files.
- `id` — sequential identifier, e.g. "p001", "p002".
- `name` — short, memorable name for the pattern.
- `description` — what it is and how it is applied.
- `applies_to` — array of repo-relative file paths where the pattern is seen.
- `confidence` — same scale as decisions.
- `provenance` — same values as decisions.

## Instructions

1. Identify 3–8 significant design decisions. Focus on non-obvious choices that a new contributor would need to understand.
2. Identify 2–5 recurring patterns. Look for consistent conventions in error handling, naming, module layout, trait usage, etc.
3. Anchor every decision to the most relevant file and symbol.
4. Be concise — summaries ≤ 20 words, rationales ≤ 50 words.
5. Assign honest confidence scores. Do not inflate confidence for speculative insights.
6. Output ONLY the JSON object. No commentary before or after."#.to_string()
}

// ── User prompt ───────────────────────────────────────────────────────────────

/// Build the user message from a slice of `ModuleIndex` values.
///
/// Each module is rendered as a compact structured summary.  If the total
/// content would exceed `MAX_USER_PROMPT_CHARS`, the least symbol-rich modules
/// are dropped to stay within budget — analogous to a Scala `takeWhile` over a
/// running character count.
pub fn intent_user_prompt(modules: &[ModuleIndex]) -> String {
    // Sort modules descending by symbol count so the most informative ones are
    // included first when truncation kicks in.
    let mut sorted: Vec<&ModuleIndex> = modules.iter().collect();
    sorted.sort_by(|a, b| b.symbols.len().cmp(&a.symbols.len()));

    let mut parts: Vec<String> = Vec::with_capacity(sorted.len());
    let mut total_chars: usize = 0;

    for module in sorted {
        let summary = format_module_summary(module);
        if total_chars + summary.len() > MAX_USER_PROMPT_CHARS {
            // Budget exhausted — skip remaining modules.
            break;
        }
        total_chars += summary.len();
        parts.push(summary);
    }

    let truncated_note = if parts.len() < modules.len() {
        format!(
            "\n[Note: {} of {} modules shown; remaining modules were truncated to stay within the context budget.]\n",
            parts.len(),
            modules.len()
        )
    } else {
        String::new()
    };

    format!(
        "Analyse the following codebase modules and produce intent JSON as instructed.\n{}\n{}",
        truncated_note,
        parts.join("\n")
    )
}

// ── Module summary ────────────────────────────────────────────────────────────

/// Format a single `ModuleIndex` as a human-readable structured block.
///
/// The output is intentionally compact — no JSON, just indented text — so
/// Claude can scan it quickly without paying the overhead of JSON delimiters.
pub fn format_module_summary(module: &ModuleIndex) -> String {
    let mut out = String::new();

    out.push_str(&format!("## {} [{}]\n", module.path, module.language));

    // ── Imports ───────────────────────────────────────────────────────────────
    if !module.imports.is_empty() {
        out.push_str("imports:\n");
        for imp in &module.imports {
            out.push_str(&format!("  {} {}\n", imp.kind, imp.source));
        }
    }

    // ── Symbols ───────────────────────────────────────────────────────────────
    if !module.symbols.is_empty() {
        out.push_str("symbols:\n");
        for sym in &module.symbols {
            // Visibility prefix — omit "private" to reduce noise.
            let vis = match sym.visibility {
                codex_parser::types::Visibility::Public => "pub ",
                codex_parser::types::Visibility::PublicCrate => "pub(crate) ",
                codex_parser::types::Visibility::PublicSuper => "pub(super) ",
                codex_parser::types::Visibility::Private => "",
            };

            // Kind label.
            let kind = format!("{:?}", sym.kind).to_lowercase();

            // Signature line.
            out.push_str(&format!(
                "  {}{} {}: {}\n",
                vis, kind, sym.name, sym.signature
            ));

            // Doc comment (trimmed, one line max for brevity).
            if let Some(doc) = &sym.doc_comment {
                let first_line = doc.lines().next().unwrap_or("").trim();
                if !first_line.is_empty() {
                    out.push_str(&format!("    # {}\n", first_line));
                }
            }
        }
    }

    out.push('\n');
    out
}
