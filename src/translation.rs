//! Translation service using Claude API.
//!
//! Uses a cheap model (Haiku) for language detection and a better model (Sonnet) for translation.

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

/// Models to use for translation
const DETECTION_MODEL: &str = "claude-3-5-haiku-latest";
const TRANSLATION_MODEL: &str = "claude-sonnet-4-20250514";
const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Pricing per million tokens (as of 2024)
/// Haiku: $0.25/M input, $1.25/M output
/// Sonnet 4: $3/M input, $15/M output
const HAIKU_INPUT_COST_PER_M: f64 = 0.25;
const HAIKU_OUTPUT_COST_PER_M: f64 = 1.25;
const SONNET_INPUT_COST_PER_M: f64 = 3.0;
const SONNET_OUTPUT_COST_PER_M: f64 = 15.0;

/// Translation service for processing messages
pub struct TranslationService {
    client: Client,
    api_key: String,
    default_language: String,
}

/// Result of processing a message for translation
#[derive(Debug, Clone)]
pub struct TranslationResult {
    /// Whether translation was needed
    pub needs_translation: bool,
    /// Original text
    pub original_text: String,
    /// Translated text (None if no translation needed)
    pub translated_text: Option<String>,
    /// Detected source language
    pub source_language: String,
    /// Token usage and cost for this translation
    pub usage: UsageInfo,
}

/// Claude API request structure
#[derive(Serialize)]
struct ClaudeRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<ClaudeMessage>,
}

#[derive(Serialize)]
struct ClaudeMessage {
    role: String,
    content: String,
}

/// Claude API response structure
#[derive(Deserialize)]
struct ClaudeResponse {
    content: Vec<ContentBlock>,
    usage: ApiUsage,
}

#[derive(Deserialize, Debug, Clone, Copy, Default)]
struct ApiUsage {
    input_tokens: u32,
    output_tokens: u32,
}

#[derive(Deserialize)]
struct ContentBlock {
    text: Option<String>,
}

/// Token usage and cost information
#[derive(Debug, Clone, Default)]
pub struct UsageInfo {
    /// Total input tokens used
    pub input_tokens: u32,
    /// Total output tokens used
    pub output_tokens: u32,
    /// Total cost in USD
    pub cost_usd: f64,
}

/// Language detection result
#[derive(Deserialize)]
struct LanguageDetection {
    language: String,
    #[serde(rename = "isEnglish")]
    is_english: bool,
}

impl TranslationService {
    /// Create a new translation service
    pub fn new(api_key: String, default_language: String) -> Self {
        info!(
            "Translation service initialized (target: {})",
            default_language
        );
        Self {
            client: Client::new(),
            api_key,
            default_language,
        }
    }

    /// Calculate cost for Haiku model usage
    fn calculate_haiku_cost(usage: &ApiUsage) -> f64 {
        let input_cost = (usage.input_tokens as f64 / 1_000_000.0) * HAIKU_INPUT_COST_PER_M;
        let output_cost = (usage.output_tokens as f64 / 1_000_000.0) * HAIKU_OUTPUT_COST_PER_M;
        input_cost + output_cost
    }

    /// Calculate cost for Sonnet model usage
    fn calculate_sonnet_cost(usage: &ApiUsage) -> f64 {
        let input_cost = (usage.input_tokens as f64 / 1_000_000.0) * SONNET_INPUT_COST_PER_M;
        let output_cost = (usage.output_tokens as f64 / 1_000_000.0) * SONNET_OUTPUT_COST_PER_M;
        input_cost + output_cost
    }

    /// Detect if text is in the default language
    async fn detect_language(&self, text: &str) -> Result<(bool, String, UsageInfo)> {
        // Skip very short messages
        if text.trim().len() < 5 {
            return Ok((true, self.default_language.clone(), UsageInfo::default()));
        }

        let prompt = format!(
            r#"Detect the language of this text and respond with ONLY a JSON object in this exact format: {{"language": "Language Name", "isEnglish": true/false}}

Text: "{}""#,
            text.chars().take(500).collect::<String>()
        );

        let request = ClaudeRequest {
            model: DETECTION_MODEL.to_string(),
            max_tokens: 100,
            messages: vec![ClaudeMessage {
                role: "user".to_string(),
                content: prompt,
            }],
        };

        let response = self
            .client
            .post(ANTHROPIC_API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send language detection request")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            warn!("Language detection API error: {} - {}", status, body);
            return Ok((true, self.default_language.clone(), UsageInfo::default()));
        }

        let claude_response: ClaudeResponse = response
            .json()
            .await
            .context("Failed to parse language detection response")?;

        // Calculate usage info for Haiku
        let usage_info = UsageInfo {
            input_tokens: claude_response.usage.input_tokens,
            output_tokens: claude_response.usage.output_tokens,
            cost_usd: Self::calculate_haiku_cost(&claude_response.usage),
        };

        debug!(
            "Language detection usage: {} in, {} out, ${:.6}",
            usage_info.input_tokens, usage_info.output_tokens, usage_info.cost_usd
        );

        let content = claude_response
            .content
            .first()
            .and_then(|c| c.text.clone())
            .unwrap_or_default();

        // Parse JSON from response
        if let Some(start) = content.find('{') {
            if let Some(end) = content.rfind('}') {
                let json_str = &content[start..=end];
                if let Ok(detection) = serde_json::from_str::<LanguageDetection>(json_str) {
                    debug!(
                        "Detected language: {} (isEnglish: {})",
                        detection.language, detection.is_english
                    );
                    return Ok((detection.is_english, detection.language, usage_info));
                }
            }
        }

        // Fallback: assume default language
        Ok((true, self.default_language.clone(), usage_info))
    }

    /// Translate text to the default language
    async fn translate(&self, text: &str, source_language: &str) -> Result<(String, UsageInfo)> {
        let prompt = format!(
            r#"Translate the following text (from {}) to {}.
Respond with ONLY the translated text, nothing else. Preserve the original formatting, tone, and meaning as closely as possible.

Text to translate:
{}"#,
            source_language, self.default_language, text
        );

        let request = ClaudeRequest {
            model: TRANSLATION_MODEL.to_string(),
            max_tokens: 2000,
            messages: vec![ClaudeMessage {
                role: "user".to_string(),
                content: prompt,
            }],
        };

        let response = self
            .client
            .post(ANTHROPIC_API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send translation request")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            warn!("Translation API error: {} - {}", status, body);
            return Ok((text.to_string(), UsageInfo::default()));
        }

        let claude_response: ClaudeResponse = response
            .json()
            .await
            .context("Failed to parse translation response")?;

        // Calculate usage info for Sonnet
        let usage_info = UsageInfo {
            input_tokens: claude_response.usage.input_tokens,
            output_tokens: claude_response.usage.output_tokens,
            cost_usd: Self::calculate_sonnet_cost(&claude_response.usage),
        };

        debug!(
            "Translation usage: {} in, {} out, ${:.6}",
            usage_info.input_tokens, usage_info.output_tokens, usage_info.cost_usd
        );

        let translated = claude_response
            .content
            .first()
            .and_then(|c| c.text.clone())
            .unwrap_or_else(|| text.to_string());

        Ok((translated.trim().to_string(), usage_info))
    }

    /// Translate text to a specific target language.
    /// Used for translating outgoing messages to match the conversation language.
    /// Returns (translated_text, usage_info)
    pub async fn translate_to(
        &self,
        text: &str,
        target_language: &str,
    ) -> Result<(String, UsageInfo)> {
        let mut total_usage = UsageInfo::default();

        // Skip if target is the default language (likely English)
        if target_language.to_lowercase() == self.default_language.to_lowercase() {
            return Ok((text.to_string(), total_usage));
        }

        // First detect if the text is already in the target language
        let (_is_target_lang, detected_lang, detection_usage) = self.detect_language(text).await?;
        total_usage = Self::combine_usage(&total_usage, &detection_usage);

        // If the text appears to be in the target language already, skip translation
        if detected_lang.to_lowercase() == target_language.to_lowercase() {
            debug!(
                "Text already in target language ({}), skipping translation",
                target_language
            );
            return Ok((text.to_string(), total_usage));
        }

        info!(
            "Translating outgoing message from {} to {}",
            detected_lang, target_language
        );

        let prompt = format!(
            r#"Translate the following text to {}.
Respond with ONLY the translated text, nothing else. Preserve the original formatting, tone, and meaning as closely as possible.

Text to translate:
{}"#,
            target_language, text
        );

        let request = ClaudeRequest {
            model: TRANSLATION_MODEL.to_string(),
            max_tokens: 2000,
            messages: vec![ClaudeMessage {
                role: "user".to_string(),
                content: prompt,
            }],
        };

        let response = self
            .client
            .post(ANTHROPIC_API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send translation request")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            warn!("Translation API error: {} - {}", status, body);
            return Ok((text.to_string(), total_usage));
        }

        let claude_response: ClaudeResponse = response
            .json()
            .await
            .context("Failed to parse translation response")?;

        // Calculate usage info for Sonnet
        let translation_usage = UsageInfo {
            input_tokens: claude_response.usage.input_tokens,
            output_tokens: claude_response.usage.output_tokens,
            cost_usd: Self::calculate_sonnet_cost(&claude_response.usage),
        };
        total_usage = Self::combine_usage(&total_usage, &translation_usage);

        debug!(
            "Outgoing translation usage: {} in, {} out, ${:.6}",
            translation_usage.input_tokens,
            translation_usage.output_tokens,
            translation_usage.cost_usd
        );

        let translated = claude_response
            .content
            .first()
            .and_then(|c| c.text.clone())
            .unwrap_or_else(|| text.to_string());

        Ok((translated.trim().to_string(), total_usage))
    }

    /// Combine two usage infos
    fn combine_usage(a: &UsageInfo, b: &UsageInfo) -> UsageInfo {
        UsageInfo {
            input_tokens: a.input_tokens + b.input_tokens,
            output_tokens: a.output_tokens + b.output_tokens,
            cost_usd: a.cost_usd + b.cost_usd,
        }
    }

    /// Process a message - detect language and translate if needed
    pub async fn process_text(&self, text: &str) -> TranslationResult {
        let mut total_usage = UsageInfo::default();

        if text.trim().is_empty() {
            return TranslationResult {
                needs_translation: false,
                original_text: text.to_string(),
                translated_text: None,
                source_language: self.default_language.clone(),
                usage: total_usage,
            };
        }

        // Step 1: Detect language
        let (is_default, detected_language, detection_usage) =
            match self.detect_language(text).await {
                Ok(result) => result,
                Err(e) => {
                    warn!("Language detection failed: {}", e);
                    (true, self.default_language.clone(), UsageInfo::default())
                }
            };
        total_usage = Self::combine_usage(&total_usage, &detection_usage);

        if is_default {
            return TranslationResult {
                needs_translation: false,
                original_text: text.to_string(),
                translated_text: None,
                source_language: detected_language,
                usage: total_usage,
            };
        }

        // Step 2: Translate
        info!("Translating message from {}...", detected_language);
        let (translated, translation_usage) = match self.translate(text, &detected_language).await {
            Ok(result) => result,
            Err(e) => {
                warn!("Translation failed: {}", e);
                (text.to_string(), UsageInfo::default())
            }
        };
        total_usage = Self::combine_usage(&total_usage, &translation_usage);

        info!(
            "Translation complete - total usage: {} in, {} out, ${:.6}",
            total_usage.input_tokens, total_usage.output_tokens, total_usage.cost_usd
        );

        TranslationResult {
            needs_translation: true,
            original_text: text.to_string(),
            translated_text: Some(translated),
            source_language: detected_language,
            usage: total_usage,
        }
    }

    /// Compose an AI-generated message based on user's prompt
    /// Returns the composed message and usage info
    ///
    /// If reply_context is provided, it contains (sender_name, message_text) of the message being replied to
    pub async fn compose_ai_message(
        &self,
        prompt: &str,
        reply_context: Option<(&str, &str)>,
    ) -> Result<(String, UsageInfo)> {
        // Validate input length (max 1000 chars for the prompt)
        if prompt.trim().is_empty() {
            anyhow::bail!("Prompt cannot be empty");
        }
        if prompt.len() > 1000 {
            anyhow::bail!("Prompt is too long (max 1000 characters)");
        }

        let system_prompt = r#"You are a helpful assistant composing WhatsApp messages. Your task is to write a message based on the user's request.

IMPORTANT RULES:
1. Keep your response SHORT and appropriate for a chat message (max 500 characters)
2. Write ONLY the message content - no explanations, no quotes, no "Here's a message:" prefixes
3. Be conversational and natural, matching the tone requested
4. Do not include anything harmful, offensive, or inappropriate
5. If the request is unclear, write a friendly, neutral message
6. Do not pretend to be someone specific or impersonate anyone
7. Do not include private information or make up facts about real people

Respond with ONLY the message text, nothing else."#;

        // Build the user message with optional reply context
        let user_message = if let Some((sender, text)) = reply_context {
            format!(
                "{}\n\nThe user is REPLYING to this message from {}:\n\"{}\"\n\nUser's request for their reply: {}",
                system_prompt,
                sender,
                text.chars().take(500).collect::<String>(), // Limit context length
                prompt
            )
        } else {
            format!("{}\n\nUser request: {}", system_prompt, prompt)
        };

        let request = ClaudeRequest {
            model: TRANSLATION_MODEL.to_string(),
            max_tokens: 300, // Limit output to keep messages short
            messages: vec![ClaudeMessage {
                role: "user".to_string(),
                content: user_message,
            }],
        };

        let response = self
            .client
            .post(ANTHROPIC_API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send AI compose request")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("AI compose API error: {} - {}", status, body);
        }

        let claude_response: ClaudeResponse = response
            .json()
            .await
            .context("Failed to parse AI compose response")?;

        let usage_info = UsageInfo {
            input_tokens: claude_response.usage.input_tokens,
            output_tokens: claude_response.usage.output_tokens,
            cost_usd: Self::calculate_sonnet_cost(&claude_response.usage),
        };

        let composed = claude_response
            .content
            .first()
            .and_then(|c| c.text.clone())
            .unwrap_or_default()
            .trim()
            .to_string();

        // Final safety check - truncate if somehow still too long
        let composed = if composed.len() > 500 {
            format!("{}...", &composed[..497])
        } else {
            composed
        };

        info!(
            "AI compose usage: {} in, {} out, ${:.6}",
            usage_info.input_tokens, usage_info.output_tokens, usage_info.cost_usd
        );

        Ok((composed, usage_info))
    }
}
