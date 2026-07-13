//! Style analyzer for AI reply generation.
//!
//! Analyzes user's outgoing messages to build a style profile that helps
//! AI-generated replies sound like the user.

use anyhow::{Context, Result};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, warn};

use crate::storage::{MessageStore, StoredMessage, StyleProfile};

const OPENAI_API_URL: &str = "https://api.openai.com/v1/responses";
const OPENAI_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const OPENAI_MAX_ATTEMPTS: usize = 3;
const GPT_5_4_NANO_INPUT_COST_PER_M: f64 = 0.10;
const GPT_5_4_NANO_CACHED_INPUT_COST_PER_M: f64 = 0.01;
const GPT_5_4_NANO_OUTPUT_COST_PER_M: f64 = 0.625;

const REFRESH_THRESHOLD: i32 = 50;
const MAX_MESSAGES_FOR_ANALYSIS: usize = 50;

pub struct StyleAnalyzer {
    client: Client,
    api_key: String,
    model: String,
    reasoning_effort: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StyleAnalysisUsage {
    pub input_tokens: i32,
    pub cached_input_tokens: i32,
    pub output_tokens: i32,
    pub cost_usd: f64,
}

#[derive(Deserialize)]
struct OpenAiResponse {
    #[serde(default)]
    output: Vec<OpenAiOutputItem>,
    usage: Option<ApiUsage>,
}

#[derive(Deserialize)]
struct OpenAiOutputItem {
    #[serde(rename = "type")]
    item_type: String,
    content: Option<Vec<OpenAiContentPart>>,
}

#[derive(Deserialize)]
struct OpenAiContentPart {
    #[serde(rename = "type")]
    part_type: String,
    text: Option<String>,
}

#[derive(Deserialize, Debug, Clone, Copy, Default)]
struct ApiUsage {
    #[serde(default)]
    input_tokens: i32,
    #[serde(default)]
    output_tokens: i32,
    input_tokens_details: Option<InputTokensDetails>,
}

#[derive(Deserialize, Debug, Clone, Copy, Default)]
struct InputTokensDetails {
    #[serde(default)]
    cached_tokens: i32,
}

fn should_retry_reqwest_error(error: &reqwest::Error) -> bool {
    error.is_timeout() || error.is_connect()
}

fn should_retry_status(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::REQUEST_TIMEOUT
        || status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || status.is_server_error()
}

fn openai_retry_delay(attempt: usize) -> Duration {
    Duration::from_millis(200 * (attempt as u64 + 1))
}

impl StyleAnalyzer {
    pub fn new(api_key: String, model: String, reasoning_effort: Option<String>) -> Self {
        Self {
            client: Client::builder()
                .timeout(OPENAI_REQUEST_TIMEOUT)
                .build()
                .expect("OpenAI HTTP client should build"),
            api_key,
            model,
            reasoning_effort,
        }
    }

    pub fn needs_refresh(
        &self,
        profile: Option<&StyleProfile>,
        current_message_count: i32,
    ) -> bool {
        match profile {
            None => true,
            Some(p) => (current_message_count - p.message_count) >= REFRESH_THRESHOLD,
        }
    }

    pub async fn get_or_create_profile(
        &self,
        store: &MessageStore,
        contact_id: Option<&str>,
    ) -> Result<(StyleProfile, Option<StyleAnalysisUsage>)> {
        let profile_id = contact_id.unwrap_or(StyleProfile::GLOBAL_ID);
        let current_count = store.get_outgoing_message_count(contact_id)?;
        let existing = store.get_style_profile(profile_id)?;

        if !self.needs_refresh(existing.as_ref(), current_count) {
            return Ok((existing.unwrap(), None));
        }

        info!(
            "Generating style profile for {} ({} messages)",
            profile_id, current_count
        );

        let messages =
            store.get_outgoing_messages_for_style(contact_id, MAX_MESSAGES_FOR_ANALYSIS)?;

        if messages.is_empty() {
            let profile = StyleProfile {
                contact_id: profile_id.to_string(),
                profile_text: "No messages available yet to analyze writing style. Use a friendly, conversational tone.".to_string(),
                sample_messages: vec![],
                message_count: 0,
                updated_at: chrono::Utc::now().timestamp(),
            };
            store.save_style_profile(&profile)?;
            return Ok((profile, None));
        }

        let (profile, usage) = self.analyze_messages(&messages, profile_id).await?;
        store.save_style_profile(&profile)?;

        Ok((profile, Some(usage)))
    }

    async fn analyze_messages(
        &self,
        messages: &[StoredMessage],
        profile_id: &str,
    ) -> Result<(StyleProfile, StyleAnalysisUsage)> {
        let message_texts: Vec<String> = messages
            .iter()
            .filter_map(|m| {
                m.original_text.clone().or_else(|| {
                    m.content.as_ref().and_then(|c| {
                        c.get("body")
                            .and_then(|v| v.as_str().map(String::from))
                            .or_else(|| c.get("caption").and_then(|v| v.as_str().map(String::from)))
                    })
                })
            })
            .filter(|t| !t.trim().is_empty())
            .collect();

        if message_texts.is_empty() {
            let profile = StyleProfile {
                contact_id: profile_id.to_string(),
                profile_text:
                    "No text messages available to analyze. Use a friendly, conversational tone."
                        .to_string(),
                sample_messages: vec![],
                message_count: messages.len() as i32,
                updated_at: chrono::Utc::now().timestamp(),
            };
            return Ok((
                profile,
                StyleAnalysisUsage {
                    input_tokens: 0,
                    cached_input_tokens: 0,
                    output_tokens: 0,
                    cost_usd: 0.0,
                },
            ));
        }

        let formatted_messages = message_texts
            .iter()
            .take(MAX_MESSAGES_FOR_ANALYSIS)
            .enumerate()
            .map(|(i, m)| format!("{}. {}", i + 1, m))
            .collect::<Vec<_>>()
            .join("\n");

        let prompt = format!(
            r#"Analyze these WhatsApp messages I've sent and describe my writing style:

<my_messages>
{}
</my_messages>

Describe concisely (max 300 words):
1. Tone: formal, casual, playful, etc.
2. Common greetings and sign-offs I use
3. Emoji and punctuation patterns (or lack thereof)
4. Typical response style to questions, invitations, requests
5. Message length tendencies
6. Any distinctive phrases or quirks

Be specific with examples from the messages. This description will help an AI write messages that sound like me."#,
            formatted_messages
        );

        let mut body = json!({
            "model": self.model,
            "instructions": "Analyze the user's writing style for future AI-assisted WhatsApp replies. Output only the style profile text.",
            "input": prompt,
            "max_output_tokens": 500,
            "text": { "verbosity": "low" },
        });
        body["reasoning"] = json!({ "effort": self.reasoning_effort.as_deref().unwrap_or("none") });

        let openai_response = self.send_style_analysis_request(&body).await?;

        let profile_text = openai_response
            .output
            .iter()
            .filter(|item| item.item_type == "message")
            .filter_map(|item| item.content.as_ref())
            .flat_map(|content| content.iter())
            .filter(|part| part.part_type == "output_text")
            .filter_map(|part| part.text.as_deref())
            .collect::<Vec<_>>()
            .join("")
            .trim()
            .to_string();

        let usage = self.calculate_usage(openai_response.usage);

        info!(
            "Style analysis complete: {} in ({} cached), {} out, ${:.6}",
            usage.input_tokens, usage.cached_input_tokens, usage.output_tokens, usage.cost_usd
        );

        let sample_messages: Vec<String> = message_texts.into_iter().take(10).collect();

        let profile = StyleProfile {
            contact_id: profile_id.to_string(),
            profile_text: if profile_text.is_empty() {
                "Unable to analyze style.".to_string()
            } else {
                profile_text
            },
            sample_messages,
            message_count: messages.len() as i32,
            updated_at: chrono::Utc::now().timestamp(),
        };

        Ok((profile, usage))
    }

    async fn send_style_analysis_request(&self, body: &Value) -> Result<OpenAiResponse> {
        for attempt in 0..OPENAI_MAX_ATTEMPTS {
            let response = self
                .client
                .post(OPENAI_API_URL)
                .bearer_auth(&self.api_key)
                .header("content-type", "application/json")
                .json(body)
                .send()
                .await;

            let response = match response {
                Ok(response) => response,
                Err(error)
                    if should_retry_reqwest_error(&error) && attempt + 1 < OPENAI_MAX_ATTEMPTS =>
                {
                    warn!(
                        "Style analysis request attempt {} failed, retrying: {}",
                        attempt + 1,
                        error
                    );
                    sleep(openai_retry_delay(attempt)).await;
                    continue;
                }
                Err(error) => return Err(error).context("Failed to send style analysis request"),
            };

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                if should_retry_status(status) && attempt + 1 < OPENAI_MAX_ATTEMPTS {
                    warn!(
                        "Style analysis request attempt {} returned {}, retrying",
                        attempt + 1,
                        status
                    );
                    sleep(openai_retry_delay(attempt)).await;
                    continue;
                }
                anyhow::bail!("Style analysis API error: {} - {}", status, body);
            }

            return response
                .json()
                .await
                .context("Failed to parse style analysis response");
        }

        unreachable!("OpenAI retry loop should return or error");
    }

    fn calculate_usage(&self, usage: Option<ApiUsage>) -> StyleAnalysisUsage {
        let usage = usage.unwrap_or_default();
        let cached_input_tokens = usage
            .input_tokens_details
            .unwrap_or_default()
            .cached_tokens
            .min(usage.input_tokens);
        let uncached_input_tokens = usage.input_tokens.saturating_sub(cached_input_tokens);

        let (input_rate, cached_rate, output_rate) = match self.model.as_str() {
            "gpt-5.6-sol" => (5.0, 0.5, 30.0),
            "gpt-5.6-terra" => (2.5, 0.25, 15.0),
            "gpt-5.6-luna" => (1.0, 0.1, 6.0),
            _ => (
                GPT_5_4_NANO_INPUT_COST_PER_M,
                GPT_5_4_NANO_CACHED_INPUT_COST_PER_M,
                GPT_5_4_NANO_OUTPUT_COST_PER_M,
            ),
        };
        let input_cost = (uncached_input_tokens as f64 / 1_000_000.0) * input_rate;
        let cached_input_cost = (cached_input_tokens as f64 / 1_000_000.0) * cached_rate;
        let output_cost = (usage.output_tokens as f64 / 1_000_000.0) * output_rate;

        StyleAnalysisUsage {
            input_tokens: usage.input_tokens,
            cached_input_tokens,
            output_tokens: usage.output_tokens,
            cost_usd: input_cost + cached_input_cost + output_cost,
        }
    }
}
