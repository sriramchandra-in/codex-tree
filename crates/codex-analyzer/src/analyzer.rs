/// Main orchestration engine for the intent analysis phase.
///
/// `Analyzer` is the public-facing entry point that CLI commands interact with.
/// It wires together caching, prompt construction, the Claude API client, and
/// response parsing into a single `analyze` call — analogous to a Scala
/// `Service` class that coordinates several lower-level components.
///
/// Also provides `write_intent` for persisting the result to disk.
use std::fs;
use std::path::Path;

use codex_parser::types::ModuleIndex;

use crate::cache;
use crate::client::ClaudeClient;
use crate::error::Result;
use crate::prompts;
use crate::response;
use crate::types::IntentOutput;

// ── Analyzer ──────────────────────────────────────────────────────────────────

/// Orchestrates intent analysis: cache lookup → prompt build → API call →
/// response parse → cache write.
pub struct Analyzer {
    client: ClaudeClient,
}

impl Analyzer {
    /// Construct an `Analyzer` from an already-configured `ClaudeClient`.
    ///
    /// In Scala: `new Analyzer(client)` — straightforward constructor injection.
    pub fn new(client: ClaudeClient) -> Self {
        Self { client }
    }

    /// Run intent analysis on the given modules.
    ///
    /// Checks the content-hash cache first; calls the Claude API only on a
    /// cache miss.  On a successful API call the result is cached before
    /// returning.
    ///
    /// # Errors
    /// Propagates any error from the Claude client (network, budget, API key)
    /// or the response parser.  Cache misses and cache write failures are
    /// treated as non-fatal and silently ignored.
    pub async fn analyze(
        &mut self,
        codex_tree_dir: &Path,
        modules: &[ModuleIndex],
    ) -> Result<IntentOutput> {
        // ── Step 1: cache lookup ──────────────────────────────────────────────
        let key = cache::cache_key(modules);
        if let Some(cached) = cache::load_cached(codex_tree_dir, &key) {
            return Ok(cached);
        }

        // ── Step 2: build prompts ─────────────────────────────────────────────
        let system = prompts::intent_system_prompt();
        let user = prompts::intent_user_prompt(modules);

        // ── Step 3: call the Claude API ───────────────────────────────────────
        let raw_response = self.client.send_message(&system, &user, 4096).await?;

        // ── Step 4: parse and validate the response ───────────────────────────
        let output = response::parse_intent_response(&raw_response, self.client.model())?;

        // ── Step 5: cache the result (best-effort — ignore write errors) ──────
        let _ = cache::save_to_cache(codex_tree_dir, &key, &output);

        Ok(output)
    }
}

// ── Disk I/O ──────────────────────────────────────────────────────────────────

/// Write intent output to disk as `intent/decisions.json` and
/// `intent/patterns.json` under `codex_tree_dir`.
///
/// Creates `intent/` if it doesn't exist.  Both JSON files are pretty-printed
/// for human readability in the repository.
///
/// In Scala: `writeIntent(dir: Path, output: IntentOutput): IO[Unit]`
pub fn write_intent(codex_tree_dir: &Path, output: &IntentOutput) -> Result<()> {
    let intent_dir = codex_tree_dir.join("intent");
    fs::create_dir_all(&intent_dir)?;

    // Serialise decisions document.
    let decisions_json = serde_json::to_string_pretty(&output.decisions)?;
    fs::write(intent_dir.join("decisions.json"), decisions_json)?;

    // Serialise patterns document.
    let patterns_json = serde_json::to_string_pretty(&output.patterns)?;
    fs::write(intent_dir.join("patterns.json"), patterns_json)?;

    Ok(())
}
