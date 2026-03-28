/// Types for the AI intent layer and AI-facing optimization layers (Claude, Cursor).
///
/// These types model the non-deterministic, AI-generated portion of the
/// codex-tree. While the AST layer (in `codex-parser`) captures "what exists",
/// the intent layer captures "what it means and why."
use serde::{Deserialize, Serialize};

// ── Provenance ───────────────────────────────────────────────────────────────

/// How an AI-generated insight was derived — its evidence trail.
///
/// In Scala terms, this is a sealed trait with case objects.
/// `#[serde(rename_all = "snake_case")]` serialises `InferredFromCode` as
/// `"inferred_from_code"` in JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    /// Extracted from doc comments or documentation files.
    ExtractedFromDocs,
    /// Extracted from commit messages or PR descriptions.
    ExtractedFromCommit,
    /// AI analysis of code patterns and structure.
    InferredFromCode,
    /// Manually added by a developer.
    HumanAnnotated,
}

// ── Anchor ───────────────────────────────────────────────────────────────────

/// A reference to a specific location in the codebase that an insight
/// is anchored to. Every AI-generated decision or pattern is grounded
/// in a concrete code location — this prevents "hallucinated" insights
/// from floating without evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Anchor {
    /// Repo-relative file path, e.g. `"src/parser/language.rs"`.
    pub file: String,
    /// Optional symbol name within the file, e.g. `"LanguageAdapter"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
}

// ── Decision ─────────────────────────────────────────────────────────────────

/// A design decision or architectural rationale inferred by AI analysis.
///
/// Each decision is anchored to a file/symbol, includes a human-readable
/// summary and rationale, and carries confidence + provenance metadata
/// so consuming AI models know what to trust vs verify.
///
/// Confidence scale:
///   1.0       — extracted from explicit documentation
///   0.7–0.9   — inferred from code with high certainty
///   0.4–0.6   — inferred but multiple interpretations possible
///   0.1–0.3   — speculative, no strong evidence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub id: String,
    pub anchored_to: Anchor,
    pub summary: String,
    pub rationale: String,
    /// Confidence in this insight, between 0.0 and 1.0.
    pub confidence: f64,
    pub provenance: Provenance,
}

// ── Pattern ──────────────────────────────────────────────────────────────────

/// A coding or design pattern observed across the codebase.
///
/// Unlike a `Decision` (anchored to one symbol), a `Pattern` may apply
/// to multiple files or directories — it captures cross-cutting concerns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Repo-relative paths this pattern applies to.
    pub applies_to: Vec<String>,
    pub confidence: f64,
    pub provenance: Provenance,
}

// ── Intent documents ─────────────────────────────────────────────────────────

/// The `intent/decisions.json` document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionsDocument {
    pub generated_at: String,
    pub generator_model: String,
    pub decisions: Vec<Decision>,
}

/// The `intent/patterns.json` document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternsDocument {
    pub generated_at: String,
    pub generator_model: String,
    pub patterns: Vec<Pattern>,
}

/// Combined output of the intent analysis phase.
///
/// In Scala: `case class IntentOutput(decisions: DecisionsDocument, patterns: PatternsDocument)`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentOutput {
    pub decisions: DecisionsDocument,
    pub patterns: PatternsDocument,
}

// ── Claude layer output ──────────────────────────────────────────────────────

/// The three progressive-detail markdown documents for `.codex-tree/claude/`.
///
/// These are generated locally from tree data + intent, no API call needed.
/// In Scala: `case class ClaudeLayerOutput(l1: String, l2: String, l3: String)`
pub struct ClaudeLayerOutput {
    /// ~500 tokens — project summary for Haiku / quick tasks.
    pub l1: String,
    /// ~2,000 tokens — module detail for Sonnet / implementation.
    pub l2: String,
    /// Full detail for Opus / architecture decisions.
    pub l3: String,
}

// ── Cursor layer output ───────────────────────────────────────────────────────

/// The three progressive-detail markdown documents for `.codex-tree/cursor/`.
///
/// Same underlying content as [`ClaudeLayerOutput`], plus a Cursor usage preamble.
/// Generated locally from tree data + intent, no API call.
pub struct CursorLayerOutput {
    pub l1: String,
    pub l2: String,
    pub l3: String,
}
