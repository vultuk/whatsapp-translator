//! Style analyzer for AI reply generation.
//!
//! Analyzes user's outgoing messages to build a style profile that helps
//! AI-generated replies sound like the user.

use anyhow::{Context, Result};
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use tracing::info;

use crate::storage::{MessageStore, StoredMessage, StyleProfile};

const OPENAI_API_URL: &str = "https://api.openai.com/v1/responses";
const GPT_5_4_NANO_INPUT_COST_PER_M: f64 = 0.10;
const GPT_5_4_NANO_CACHED_INPUT_COST_PER_M: f64 = 0.01;
const GPT_5_4_NANO_OUTPUT_COST_PER_M: f64 = 0.625;

const REFRESH_THRESHOLD: i32 = 50;
const MAX_MESSAGES_FOR_ANALYSIS: usize = 50;

pub struct StyleAnalyzer {
    client: Client,
    api_key: String,
    model: String,
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

impl StyleAnalyzer {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            model,
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

        let body = json!({
            "model": self.model,
            "instructions": "Analyze the user's writing style for future AI-assisted WhatsApp replies. Output only the style profile text.",
            "input": prompt,
            "max_output_tokens": 500,
            "reasoning": { "effort": "none" },
            "text": { "verbosity": "low" },
        });

        let response = self
            .client
            .post(OPENAI_API_URL)
            .bearer_auth(&self.api_key)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Failed to send style analysis request")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Style analysis API error: {} - {}", status, body);
        }

        let openai_response: OpenAiResponse = response
            .json()
            .await
            .context("Failed to parse style analysis response")?;

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

        let usage = Self::calculate_usage(openai_response.usage);

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

    fn calculate_usage(usage: Option<ApiUsage>) -> StyleAnalysisUsage {
        let usage = usage.unwrap_or_default();
        let cached_input_tokens = usage
            .input_tokens_details
            .unwrap_or_default()
            .cached_tokens
            .min(usage.input_tokens);
        let uncached_input_tokens = usage.input_tokens.saturating_sub(cached_input_tokens);

        let input_cost =
            (uncached_input_tokens as f64 / 1_000_000.0) * GPT_5_4_NANO_INPUT_COST_PER_M;
        let cached_input_cost =
            (cached_input_tokens as f64 / 1_000_000.0) * GPT_5_4_NANO_CACHED_INPUT_COST_PER_M;
        let output_cost =
            (usage.output_tokens as f64 / 1_000_000.0) * GPT_5_4_NANO_OUTPUT_COST_PER_M;

        StyleAnalysisUsage {
            input_tokens: usage.input_tokens,
            cached_input_tokens,
            output_tokens: usage.output_tokens,
            cost_usd: input_cost + cached_input_cost + output_cost,
        }
    }
}
