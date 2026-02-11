//! Style analyzer for AI reply generation.
//!
//! Analyzes user's outgoing messages to build a style profile that helps
//! OpenAI generate replies that sound like the user.

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::storage::{MessageStore, StoredMessage, StyleProfile};

/// Model to use for style analysis
const STYLE_ANALYSIS_MODEL: &str = "gpt-4o-mini";
const OPENAI_API_URL: &str = "https://api.openai.com/v1/chat/completions";

/// Pricing per million tokens (GPT-4o mini)
const MINI_INPUT_COST_PER_M: f64 = 0.15;
const MINI_OUTPUT_COST_PER_M: f64 = 0.60;

/// How many new messages before we should refresh a style profile
const REFRESH_THRESHOLD: i32 = 50;

/// Maximum messages to analyze for style
const MAX_MESSAGES_FOR_ANALYSIS: usize = 50;

/// Style analyzer service
pub struct StyleAnalyzer {
    client: Client,
    api_key: String,
}

/// Usage info for style analysis
#[derive(Debug, Clone)]
pub struct StyleAnalysisUsage {
    pub input_tokens: i32,
    pub output_tokens: i32,
    pub cost_usd: f64,
}

/// OpenAI Chat Completions structures (duplicated from translation.rs for modularity)
#[derive(Serialize)]
struct OpenAiRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<OpenAiMessage>,
}

#[derive(Serialize)]
struct OpenAiMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
    #[serde(default)]
    usage: OpenAiUsage,
}

#[derive(Deserialize)]
struct OpenAiChoice {
    message: OpenAiResponseMessage,
}

#[derive(Deserialize)]
struct OpenAiResponseMessage {
    content: Option<String>,
}

#[derive(Deserialize, Default)]
struct OpenAiUsage {
    #[serde(default)]
    prompt_tokens: u32,
    #[serde(default)]
    completion_tokens: u32,
}

impl StyleAnalyzer {
    /// Create a new style analyzer
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
        }
    }

    /// Check if a style profile needs to be refreshed
    /// Returns true if profile is missing or stale (many new messages since last analysis)
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

    /// Get or create a style profile for a contact (or global)
    /// If profile exists and is fresh, returns it. Otherwise generates a new one.
    pub async fn get_or_create_profile(
        &self,
        store: &MessageStore,
        contact_id: Option<&str>,
    ) -> Result<(StyleProfile, Option<StyleAnalysisUsage>)> {
        let profile_id = contact_id.unwrap_or(StyleProfile::GLOBAL_ID);

        // Get current message count
        let current_count = store.get_outgoing_message_count(contact_id)?;

        // Check existing profile
        let existing = store.get_style_profile(profile_id)?;

        if !self.needs_refresh(existing.as_ref(), current_count) {
            // Profile is fresh, return it
            return Ok((existing.unwrap(), None));
        }

        // Need to generate/refresh profile
        info!(
            "Generating style profile for {} ({} messages)",
            profile_id, current_count
        );

        let messages =
            store.get_outgoing_messages_for_style(contact_id, MAX_MESSAGES_FOR_ANALYSIS)?;

        if messages.is_empty() {
            // No messages to analyze, create a placeholder profile
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

        // Generate style profile using OpenAI
        let (profile, usage) = self.analyze_messages(&messages, profile_id).await?;

        // Save the profile
        store.save_style_profile(&profile)?;

        Ok((profile, Some(usage)))
    }

    /// Analyze messages and generate a style profile
    async fn analyze_messages(
        &self,
        messages: &[StoredMessage],
        profile_id: &str,
    ) -> Result<(StyleProfile, StyleAnalysisUsage)> {
        // Extract text from messages
        let message_texts: Vec<String> = messages
            .iter()
            .filter_map(|m| {
                // Get the text content
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
                    output_tokens: 0,
                    cost_usd: 0.0,
                },
            ));
        }

        // Format messages for the prompt
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

        let request = OpenAiRequest {
            model: STYLE_ANALYSIS_MODEL.to_string(),
            max_tokens: 500,
            messages: vec![OpenAiMessage {
                role: "user".to_string(),
                content: prompt,
            }],
        };

        let response = self
            .client
            .post(OPENAI_API_URL)
            .bearer_auth(&self.api_key)
            .header("content-type", "application/json")
            .json(&request)
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
            .choices
            .first()
            .and_then(|c| c.message.content.clone())
            .unwrap_or_else(|| "Unable to analyze style.".to_string());

        let usage = StyleAnalysisUsage {
            input_tokens: openai_response.usage.prompt_tokens as i32,
            output_tokens: openai_response.usage.completion_tokens as i32,
            cost_usd: Self::calculate_cost(&openai_response.usage),
        };

        info!(
            "Style analysis complete: {} in, {} out, ${:.6}",
            usage.input_tokens, usage.output_tokens, usage.cost_usd
        );

        // Keep a sample of messages for reference
        let sample_messages: Vec<String> = message_texts.into_iter().take(10).collect();

        let profile = StyleProfile {
            contact_id: profile_id.to_string(),
            profile_text,
            sample_messages,
            message_count: messages.len() as i32,
            updated_at: chrono::Utc::now().timestamp(),
        };

        Ok((profile, usage))
    }

    /// Calculate cost for GPT-4o mini usage
    fn calculate_cost(usage: &OpenAiUsage) -> f64 {
        let input_cost = (usage.prompt_tokens as f64 / 1_000_000.0) * MINI_INPUT_COST_PER_M;
        let output_cost = (usage.completion_tokens as f64 / 1_000_000.0) * MINI_OUTPUT_COST_PER_M;
        input_cost + output_cost
    }
}
