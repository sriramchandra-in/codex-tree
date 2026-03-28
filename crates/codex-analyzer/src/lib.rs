// codex-analyzer: AI-powered intent analysis for codex-tree knowledge trees.
//
// Module layout:
//   error        — AnalyzerError enum + Result<T> alias
//   types        — Intent layer data model (Decision, Pattern, Provenance, etc.)
//   client       — Claude API HTTP client (reqwest + Anthropic Messages API)
//   prompts      — Prompt templates for intent analysis
//   response     — Parse/validate Claude API responses
//   cache        — Content-hash-based analysis caching
//   analyzer     — Main orchestration engine
//   claude_layer — Claude-optimised markdown layer generation (L1/L2/L3)
//   cursor_layer — Cursor-optimised markdown (same digest + Cursor usage guide)

pub mod analyzer;
pub mod cache;
pub mod claude_layer;
pub mod cursor_layer;
pub mod client;
pub mod error;
pub mod prompts;
pub mod response;
pub mod types;
