//! Translation service using Claude API.
//!
//! Uses a cheap model (Haiku) for language detection and a better model (Sonnet) for translation.

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

/// Models to use for translation
const DETECTION_MODEL: &str = "claude-haiku-4-5";
const TRANSLATION_MODEL: &str = "claude-sonnet-4-5";
const AI_COMPOSE_MODEL: &str = "claude-opus-4-5";
const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Pricing per million tokens (as of 2025)
/// Haiku 4.5: $1/M input, $5/M output
/// Sonnet 4.5: $3/M input, $15/M output
/// Opus 4.5: $5/M input, $25/M output
const HAIKU_INPUT_COST_PER_M: f64 = 1.0;
const HAIKU_OUTPUT_COST_PER_M: f64 = 5.0;
const SONNET_INPUT_COST_PER_M: f64 = 3.0;
const SONNET_OUTPUT_COST_PER_M: f64 = 15.0;
const OPUS_INPUT_COST_PER_M: f64 = 5.0;
const OPUS_OUTPUT_COST_PER_M: f64 = 25.0;

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

/// Claude API request with vision support
#[derive(Serialize)]
struct ClaudeVisionRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<ClaudeVisionMessage>,
}

#[derive(Serialize)]
struct ClaudeVisionMessage {
    role: String,
    content: Vec<VisionContentBlock>,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum VisionContentBlock {
    Text { text: String },
    Image { source: ImageSource },
}

#[derive(Serialize)]
struct ImageSource {
    #[serde(rename = "type")]
    source_type: String,
    media_type: String,
    data: String,
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

    /// Get the API key (for creating other services like StyleAnalyzer)
    pub fn get_api_key(&self) -> String {
        self.api_key.clone()
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

    /// Calculate cost for Opus model usage
    fn calculate_opus_cost(usage: &ApiUsage) -> f64 {
        let input_cost = (usage.input_tokens as f64 / 1_000_000.0) * OPUS_INPUT_COST_PER_M;
        let output_cost = (usage.output_tokens as f64 / 1_000_000.0) * OPUS_OUTPUT_COST_PER_M;
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
    /// Parameters:
    /// - prompt: The user's instruction for what message to compose
    /// - reply_context: Optional (sender_name, message_text) of the message being replied to
    /// - reply_image: Optional (media_type, base64_data) of an image being replied to
    pub async fn compose_ai_message(
        &self,
        prompt: &str,
        reply_context: Option<(&str, &str)>,
        reply_image: Option<(&str, &str)>,
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
8. If an image is provided, you can reference what you see in it when composing your reply

Respond with ONLY the message text, nothing else."#;

        // Build the user message with optional reply context
        let text_content = if let Some((sender, text)) = reply_context {
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

        // Build request - use vision API if image is provided
        let response = if let Some((media_type, base64_data)) = reply_image {
            // Vision request with image
            let mut content_blocks = vec![
                VisionContentBlock::Image {
                    source: ImageSource {
                        source_type: "base64".to_string(),
                        media_type: media_type.to_string(),
                        data: base64_data.to_string(),
                    },
                },
                VisionContentBlock::Text { text: text_content },
            ];

            // If there's text context about the image, add it
            if let Some((sender, _)) = reply_context {
                content_blocks.insert(
                    1,
                    VisionContentBlock::Text {
                        text: format!("The above image was sent by {}.", sender),
                    },
                );
            }

            let request = ClaudeVisionRequest {
                model: AI_COMPOSE_MODEL.to_string(),
                max_tokens: 300,
                messages: vec![ClaudeVisionMessage {
                    role: "user".to_string(),
                    content: content_blocks,
                }],
            };

            self.client
                .post(ANTHROPIC_API_URL)
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", ANTHROPIC_VERSION)
                .header("content-type", "application/json")
                .json(&request)
                .send()
                .await
                .context("Failed to send AI compose request")?
        } else {
            // Text-only request
            let request = ClaudeRequest {
                model: AI_COMPOSE_MODEL.to_string(),
                max_tokens: 300,
                messages: vec![ClaudeMessage {
                    role: "user".to_string(),
                    content: text_content,
                }],
            };

            self.client
                .post(ANTHROPIC_API_URL)
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", ANTHROPIC_VERSION)
                .header("content-type", "application/json")
                .json(&request)
                .send()
                .await
                .context("Failed to send AI compose request")?
        };

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
            cost_usd: Self::calculate_opus_cost(&claude_response.usage),
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

    /// Generate a styled reply to a received message
    ///
    /// Uses the user's style profile and conversation context to generate
    /// a reply that sounds like them.
    ///
    /// Parameters:
    /// - message_to_reply: The incoming message to reply to
    /// - recent_conversation: Recent messages for context (last ~20)
    /// - global_style: User's overall writing style profile
    /// - contact_style: Optional style specific to this contact
    /// - my_reply_examples: Examples of user's outgoing messages to this contact
    pub async fn compose_styled_reply(
        &self,
        message_to_reply: &crate::storage::StoredMessage,
        recent_conversation: &[crate::storage::StoredMessage],
        global_style: &crate::storage::StyleProfile,
        contact_style: Option<&crate::storage::StyleProfile>,
        my_reply_examples: &[crate::storage::StoredMessage],
    ) -> Result<(String, UsageInfo)> {
        // Extract text from the message being replied to
        let reply_to_text = message_to_reply
            .original_text
            .clone()
            .or_else(|| message_to_reply.translated_text.clone())
            .or_else(|| {
                message_to_reply.content.as_ref().and_then(|c| {
                    c.get("body")
                        .and_then(|v| v.as_str().map(String::from))
                        .or_else(|| c.get("caption").and_then(|v| v.as_str().map(String::from)))
                })
            })
            .unwrap_or_else(|| "[No text content]".to_string());

        // Truncate if too long
        let reply_to_text = if reply_to_text.len() > 500 {
            format!("{}...", &reply_to_text[..497])
        } else {
            reply_to_text
        };

        // Get sender name
        let sender_name = message_to_reply
            .sender_name
            .clone()
            .or_else(|| message_to_reply.contact_name.clone())
            .unwrap_or_else(|| "Someone".to_string());

        // Format recent conversation
        let conversation_context = Self::format_conversation(recent_conversation);

        // Format my reply examples
        let my_examples = Self::format_my_examples(my_reply_examples);

        // Build the contact-specific style section
        let contact_style_section = if let Some(cs) = contact_style {
            format!(
                "## MY STYLE WITH THIS SPECIFIC CONTACT:\n{}\n",
                cs.profile_text
            )
        } else {
            "## MY STYLE WITH THIS SPECIFIC CONTACT:\nNo specific style data for this contact yet. Use my general style.\n".to_string()
        };

        // Build the full prompt
        let prompt = format!(
            r#"You are writing a WhatsApp reply for me. Match my writing style EXACTLY.

## MY WRITING STYLE (GENERAL):
{}

{}
## RECENT CONVERSATION:
{}

## EXAMPLES OF MY MESSAGES TO THIS CONTACT:
{}

## MESSAGE I'M REPLYING TO:
From: {}
"{}"

## RULES:
1. Match my tone, emoji usage, and phrasing exactly
2. Keep it natural WhatsApp length (don't over-explain)
3. Respond appropriately to what was asked/said
4. Output ONLY the reply text - no quotes, no "Reply:", no explanations
5. If I typically use certain greetings/phrases/emojis, use them naturally

Generate my reply:"#,
            global_style.profile_text,
            contact_style_section,
            conversation_context,
            my_examples,
            sender_name,
            reply_to_text
        );

        // Call Claude Opus for high-quality reply generation
        let request = ClaudeRequest {
            model: AI_COMPOSE_MODEL.to_string(),
            max_tokens: 300,
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
            .context("Failed to send styled reply request")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Styled reply API error: {} - {}", status, body);
        }

        let claude_response: ClaudeResponse = response
            .json()
            .await
            .context("Failed to parse styled reply response")?;

        let usage_info = UsageInfo {
            input_tokens: claude_response.usage.input_tokens,
            output_tokens: claude_response.usage.output_tokens,
            cost_usd: Self::calculate_opus_cost(&claude_response.usage),
        };

        let reply = claude_response
            .content
            .first()
            .and_then(|c| c.text.clone())
            .unwrap_or_default()
            .trim()
            .to_string();

        // Safety check - truncate if too long
        let reply = if reply.len() > 500 {
            format!("{}...", &reply[..497])
        } else {
            reply
        };

        info!(
            "Styled reply generated: {} chars, {} in, {} out, ${:.6}",
            reply.len(),
            usage_info.input_tokens,
            usage_info.output_tokens,
            usage_info.cost_usd
        );

        Ok((reply, usage_info))
    }

    /// Format conversation messages for the prompt
    fn format_conversation(messages: &[crate::storage::StoredMessage]) -> String {
        if messages.is_empty() {
            return "No recent messages.".to_string();
        }

        messages
            .iter()
            .map(|m| {
                let sender = if m.is_from_me {
                    "Me".to_string()
                } else {
                    m.sender_name
                        .clone()
                        .or_else(|| m.contact_name.clone())
                        .unwrap_or_else(|| "Them".to_string())
                };

                let text = m
                    .original_text
                    .clone()
                    .or_else(|| m.translated_text.clone())
                    .or_else(|| {
                        m.content.as_ref().and_then(|c| {
                            c.get("body")
                                .and_then(|v| v.as_str().map(String::from))
                                .or_else(|| {
                                    c.get("caption").and_then(|v| v.as_str().map(String::from))
                                })
                        })
                    })
                    .unwrap_or_else(|| format!("[{}]", m.content_type));

                // Truncate long messages
                let text = if text.len() > 200 {
                    format!("{}...", &text[..197])
                } else {
                    text
                };

                format!("{}: {}", sender, text)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Format user's example messages for the prompt
    fn format_my_examples(messages: &[crate::storage::StoredMessage]) -> String {
        if messages.is_empty() {
            return "No previous messages to this contact yet.".to_string();
        }

        messages
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
            .enumerate()
            .map(|(i, text)| {
                let text = if text.len() > 150 {
                    format!("{}...", &text[..147])
                } else {
                    text
                };
                format!("{}. {}", i + 1, text)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}
