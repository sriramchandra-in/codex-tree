/// Parse and validate Claude API responses into `IntentOutput`.
///
/// In Scala terms, this is a pure parse layer — a function
/// `String => Either[AnalyzerError, IntentOutput]` with no side-effects.
/// All validation and ID assignment happens here so the caller gets a clean,
/// well-formed value or a precise error.
use serde::Deserialize;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::{AnalyzerError, Result};
use crate::types::{
    Anchor, Decision, DecisionsDocument, IntentOutput, Pattern, PatternsDocument, Provenance,
};

// ── Intermediate deserialisation types ───────────────────────────────────────

/// Claude's raw JSON output — more permissive than the final types so we can
/// apply validation and defaulting before constructing the canonical structs.
#[derive(Deserialize)]
struct RawIntentOutput {
    decisions: Vec<RawDecision>,
    patterns: Vec<RawPattern>,
}

#[derive(Deserialize)]
struct RawDecision {
    #[serde(default)]
    id: String,
    anchored_to: RawAnchor,
    summary: String,
    rationale: String,
    confidence: f64,
    provenance: Option<Provenance>,
}

#[derive(Deserialize)]
struct RawAnchor {
    file: String,
    symbol: Option<String>,
}

#[derive(Deserialize)]
struct RawPattern {
    #[serde(default)]
    id: String,
    name: String,
    description: String,
    applies_to: Vec<String>,
    confidence: f64,
    provenance: Option<Provenance>,
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Parse a raw Claude response string into `IntentOutput`.
///
/// Steps:
/// 1. Extract JSON from the response (strips markdown code fences if present).
/// 2. Deserialise into intermediate types.
/// 3. Clamp confidence values to `[0.0, 1.0]`.
/// 4. Assign sequential IDs to any item whose `id` field was empty.
/// 5. Wrap into the canonical output types with timestamp + model metadata.
///
/// Returns `AnalyzerError::ResponseParse` on any failure.
pub fn parse_intent_response(raw: &str, model: &str) -> Result<IntentOutput> {
    // ── Step 1: extract JSON ──────────────────────────────────────────────────
    let json_str = extract_json(raw).ok_or_else(|| {
        AnalyzerError::ResponseParse(format!(
            "No JSON object found in response. First 200 chars: {}",
            raw.chars().take(200).collect::<String>()
        ))
    })?;

    // ── Step 2: deserialise ───────────────────────────────────────────────────
    let raw_output: RawIntentOutput = serde_json::from_str(json_str).map_err(|e| {
        AnalyzerError::ResponseParse(format!(
            "JSON schema mismatch: {}. Snippet: {}",
            e,
            json_str.chars().take(300).collect::<String>()
        ))
    })?;

    let generated_at = current_utc_iso8601();

    // ── Step 3+4: validate and normalise decisions ────────────────────────────
    let decisions: Vec<Decision> = raw_output
        .decisions
        .into_iter()
        .enumerate()
        .map(|(i, d)| Decision {
            id: if d.id.is_empty() {
                format!("d{:03}", i + 1)
            } else {
                d.id
            },
            anchored_to: Anchor {
                file: d.anchored_to.file,
                symbol: d.anchored_to.symbol,
            },
            summary: d.summary,
            rationale: d.rationale,
            confidence: d.confidence.clamp(0.0, 1.0),
            provenance: d.provenance.unwrap_or(Provenance::InferredFromCode),
        })
        .collect();

    // ── Step 3+4: validate and normalise patterns ─────────────────────────────
    let patterns: Vec<Pattern> = raw_output
        .patterns
        .into_iter()
        .enumerate()
        .map(|(i, p)| Pattern {
            id: if p.id.is_empty() {
                format!("p{:03}", i + 1)
            } else {
                p.id
            },
            name: p.name,
            description: p.description,
            applies_to: p.applies_to,
            confidence: p.confidence.clamp(0.0, 1.0),
            provenance: p.provenance.unwrap_or(Provenance::InferredFromCode),
        })
        .collect();

    // ── Step 5: wrap into output documents ───────────────────────────────────
    Ok(IntentOutput {
        decisions: DecisionsDocument {
            generated_at: generated_at.clone(),
            generator_model: model.to_string(),
            decisions,
        },
        patterns: PatternsDocument {
            generated_at,
            generator_model: model.to_string(),
            patterns,
        },
    })
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Try to isolate a JSON object from `raw`.
///
/// Strategy (in order):
/// 1. Look for a ` ```json ` code fence and extract its contents.
/// 2. Look for any ` ``` ` code fence.
/// 3. Find the first `{` and last `}` in the string.
/// 4. Return `None` if none of the above succeed.
fn extract_json(raw: &str) -> Option<&str> {
    // ── Strategy 1: ```json fence ─────────────────────────────────────────────
    if let Some(start) = raw.find("```json") {
        let inner = &raw[start + 7..]; // skip "```json"
        if let Some(end) = inner.find("```") {
            return Some(inner[..end].trim());
        }
    }

    // ── Strategy 2: ``` fence (no language tag) ───────────────────────────────
    if let Some(start) = raw.find("```") {
        let inner = &raw[start + 3..];
        if let Some(end) = inner.find("```") {
            let candidate = inner[..end].trim();
            if candidate.starts_with('{') {
                return Some(candidate);
            }
        }
    }

    // ── Strategy 3: first { to last } ────────────────────────────────────────
    let first_brace = raw.find('{')?;
    let last_brace = raw.rfind('}')?;
    if last_brace > first_brace {
        return Some(&raw[first_brace..=last_brace]);
    }

    None
}

/// Return the current UTC time as an ISO-8601 string, e.g. `"2026-03-28T12:00:00Z"`.
///
/// Reimplemented inline (same algorithm as `codex-parser::serializer`) to avoid
/// a cross-crate dependency on a trivial utility.
fn current_utc_iso8601() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let (year, month, day, hour, minute, second) = unix_secs_to_datetime(secs);

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hour, minute, second
    )
}

/// Convert a Unix timestamp to `(year, month, day, hour, min, sec)`.
///
/// Uses the proleptic Gregorian calendar (Richards' algorithm) — works past 2038.
fn unix_secs_to_datetime(secs: u64) -> (u64, u64, u64, u64, u64, u64) {
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

    (year, month, day, hour, minute, second)
}
