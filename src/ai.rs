// Claude API integration for macroeconomic rate-decision interpretation.
//
// Calls the Anthropic Messages API directly over HTTP (no SDK).
// Accepts a snapshot of current indicator readings and returns a plain-English
// analysis of what they collectively imply for the Fed's next rate decision.
//
// API reference: https://docs.anthropic.com/en/api/messages

use serde::{Deserialize, Serialize};
use tracing::info;

use crate::error::AppError;
use crate::indicators::IndicatorReading;

const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
const CLAUDE_MODEL: &str = "claude-haiku-4-5-20251001";
const MAX_TOKENS: u32 = 500;

// --- Anthropic Messages API request/response types ---

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<AnthropicMessage>,
    system: String,
}

#[derive(Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContent>,
}

#[derive(Deserialize)]
struct AnthropicContent {
    #[serde(rename = "type")]
    content_type: String,
    text: Option<String>,
}

/// Send current indicator readings to Claude and return a plain-English Fed policy summary.
///
/// Accepts a slice of references so callers can pass snapshots without transferring ownership.
pub async fn interpret_indicators(
    http_client: &reqwest::Client,
    api_key: &str,
    readings: &[&IndicatorReading],
) -> Result<String, AppError> {
    if readings.is_empty() {
        return Ok(
            "No indicator data available yet. Please wait for the first data poll.".to_string(),
        );
    }

    info!(
        "Requesting AI interpretation for {} indicators",
        readings.len()
    );

    let request_body = AnthropicRequest {
        model: CLAUDE_MODEL.to_string(),
        max_tokens: MAX_TOKENS,
        system: SYSTEM_PROMPT.to_string(),
        messages: vec![AnthropicMessage {
            role: "user".to_string(),
            content: build_data_summary(readings),
        }],
    };

    // Anthropic uses x-api-key auth, not Bearer tokens.
    let response = http_client
        .post(ANTHROPIC_API_URL)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&request_body)
        .send()
        .await?;

    let response = response
        .error_for_status()
        .map_err(|e| AppError::AiError(format!("Anthropic API returned error status: {}", e)))?;

    let api_response: AnthropicResponse = response
        .json()
        .await
        .map_err(|e| AppError::AiError(format!("Failed to parse Anthropic response: {}", e)))?;

    api_response
        .content
        .iter()
        .find(|c| c.content_type == "text")
        .and_then(|c| c.text.as_ref())
        .cloned()
        .ok_or_else(|| AppError::AiError("No text content in Anthropic response".to_string()))
}

/// Format all readings into a structured user prompt.
fn build_data_summary(readings: &[&IndicatorReading]) -> String {
    let mut prompt = String::from("Here are the latest US macroeconomic indicator readings:\n\n");
    for reading in readings {
        prompt.push_str(&format!(
            "- {}: {} (as of {}) — Sentiment: {}\n",
            reading.id.display_name(),
            reading.display_value,
            reading.date.format("%B %d, %Y"),
            reading.sentiment.label(),
        ));
    }
    prompt.push_str("\nBased on these readings, please provide your analysis.");
    prompt
}

const SYSTEM_PROMPT: &str = r#"You are a macroeconomic analyst specializing in Federal Reserve monetary policy.
Your role is to interpret economic indicator data and explain what it means for the Fed's next rate decision.

When given indicator readings, provide a concise (3-5 paragraph) analysis that:
1. Identifies the dominant theme in the data (inflationary pressures, cooling, mixed signals, etc.)
2. Explains what the combined picture suggests for the Fed's next FOMC meeting decision
3. Highlights the 2-3 most significant readings and why they matter most
4. Uses plain English accessible to educated non-economists
5. Ends with a clear directional statement: "Rate cut likely", "Rates on hold likely", or "Rate hike risk"

Be direct and specific. Cite actual numbers. Avoid jargon where possible, or explain it when necessary.
Do not add disclaimers about not being financial advice — this is a data analysis tool."#;
