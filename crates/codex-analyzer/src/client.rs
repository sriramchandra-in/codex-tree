/// Claude API HTTP client — thin wrapper around `reqwest`.
///
/// In Scala terms, this is a class that owns a `sttp` or `http4s` client
/// and exposes a single `sendMessage` method returning `IO[String]`.
/// Here we use `reqwest` with `async/await` and Rust's `?` propagation.
use std::env;

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error::{AnalyzerError, Result};

// ── Wire-format types ─────────────────────────────────────────────────────────

/// Outbound request body — matches the Anthropic Messages API schema.
///
/// `#[serde(Serialize)]` generates JSON encoding automatically, analogous to
/// a Scala `case class` with a `circe` Encoder in scope.
#[derive(Serialize)]
struct MessageRequest {
    model: String,
    max_tokens: usize,
    system: String,
    messages: Vec<Message>,
}

/// A single turn in the conversation.
#[derive(Serialize)]
struct Message {
    role: String,
    content: String,
}

/// Top-level response envelope from the Anthropic API.
#[derive(Deserialize)]
struct MessageResponse {
    content: Vec<ContentBlock>,
    usage: Usage,
}

/// One block in the response `content` array.
///
/// Claude always returns at least one `"text"` block; we extract its text.
#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    content_type: String,
    text: Option<String>,
}

/// Token accounting returned by the API.
#[derive(Deserialize)]
struct Usage {
    input_tokens: usize,
    output_tokens: usize,
}

// ── Client ────────────────────────────────────────────────────────────────────

/// Stateful Claude API client.
///
/// `tokens_used` accumulates across calls so the caller can enforce a
/// session-level token budget — analogous to a `Ref[IO, Int]` in Scala.
pub struct ClaudeClient {
    client: Client,
    api_key: String,
    model: String,
    budget_limit: Option<usize>,
    tokens_used: usize,
}

impl ClaudeClient {
    /// Construct a client from environment variables.
    ///
    /// | Variable               | Required | Default                           |
    /// |------------------------|----------|-----------------------------------|
    /// | `ANTHROPIC_API_KEY`    | yes      | —                                 |
    /// | `CODEX_TREE_MODEL`     | no       | `claude-sonnet-4-20250514`        |
    /// | `CODEX_TREE_BUDGET`    | no       | unlimited                         |
    ///
    /// Returns `AnalyzerError::ApiKeyMissing` if the API key env-var is absent.
    pub fn from_env() -> Result<Self> {
        let api_key = env::var("ANTHROPIC_API_KEY").map_err(|_| AnalyzerError::ApiKeyMissing)?;

        let model = env::var("CODEX_TREE_MODEL")
            .unwrap_or_else(|_| "claude-sonnet-4-20250514".to_string());

        let budget_limit = env::var("CODEX_TREE_BUDGET")
            .ok()
            .and_then(|s| s.parse::<usize>().ok());

        let client = Client::new();

        Ok(Self {
            client,
            api_key,
            model,
            budget_limit,
            tokens_used: 0,
        })
    }

    /// The model ID in use, e.g. `"claude-sonnet-4-20250514"`.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Send a single-turn message to Claude and return the text response.
    ///
    /// POSTs to `https://api.anthropic.com/v1/messages` with the required
    /// `x-api-key` and `anthropic-version` headers.
    ///
    /// Token usage is accumulated internally; if a `budget_limit` was set,
    /// returns `AnalyzerError::BudgetExceeded` rather than making the call.
    ///
    /// # Errors
    /// - `AnalyzerError::BudgetExceeded` — pre-call budget check failed
    /// - `AnalyzerError::Http`           — network or transport error
    /// - `AnalyzerError::ApiError`       — non-2xx HTTP status from the API
    /// - `AnalyzerError::ResponseParse`  — response JSON doesn't contain text
    pub async fn send_message(
        &mut self,
        system: &str,
        user_content: &str,
        max_tokens: usize,
    ) -> Result<String> {
        // ── Pre-call budget guard ─────────────────────────────────────────────
        if let Some(limit) = self.budget_limit {
            if self.tokens_used >= limit {
                return Err(AnalyzerError::BudgetExceeded {
                    used: self.tokens_used,
                    limit,
                });
            }
        }

        // ── Build request ─────────────────────────────────────────────────────
        let request_body = MessageRequest {
            model: self.model.clone(),
            max_tokens,
            system: system.to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: user_content.to_string(),
            }],
        };

        // ── Execute HTTP call ─────────────────────────────────────────────────
        let http_response = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request_body)
            .send()
            .await?;

        let status = http_response.status();

        if !status.is_success() {
            let body = http_response.text().await.unwrap_or_default();
            return Err(AnalyzerError::ApiError {
                status: status.as_u16(),
                body,
            });
        }

        // ── Parse response ────────────────────────────────────────────────────
        let response: MessageResponse = http_response.json().await?;

        // ── Accumulate tokens ─────────────────────────────────────────────────
        let call_tokens = response.usage.input_tokens + response.usage.output_tokens;
        self.tokens_used += call_tokens;

        // ── Post-call budget check ────────────────────────────────────────────
        if let Some(limit) = self.budget_limit {
            if self.tokens_used > limit {
                return Err(AnalyzerError::BudgetExceeded {
                    used: self.tokens_used,
                    limit,
                });
            }
        }

        // ── Extract text from first text block ────────────────────────────────
        // Analogous to `response.content.collectFirst { case TextBlock(t) => t }`.
        let text = response
            .content
            .into_iter()
            .find(|b| b.content_type == "text")
            .and_then(|b| b.text)
            .ok_or_else(|| {
                AnalyzerError::ResponseParse(
                    "API response contained no text block".to_string(),
                )
            })?;

        Ok(text)
    }
}
