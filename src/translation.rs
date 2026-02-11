//! Translation service using OpenAI API.
//!
//! Uses a cheaper model for language detection/translation and a stronger model for AI compose.

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

/// Models to use for language and translation workflows
const DETECTION_MODEL: &str = "gpt-4o-mini";
const TRANSLATION_MODEL: &str = "gpt-4o-mini";
const AI_COMPOSE_MODEL: &str = "gpt-4o";
const OPENAI_API_URL: &str = "https://api.openai.com/v1/chat/completions";

/// Pricing per million tokens (USD, as of 2026)
/// GPT-4o mini: $0.15/M input, $0.60/M output
/// GPT-4o: $2.50/M input, $10.00/M output
const MINI_INPUT_COST_PER_M: f64 = 0.15;
const MINI_OUTPUT_COST_PER_M: f64 = 0.60;
const GPT4O_INPUT_COST_PER_M: f64 = 2.50;
const GPT4O_OUTPUT_COST_PER_M: f64 = 10.00;

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

/// OpenAI Chat Completions request structure
#[derive(Serialize)]
struct OpenAiRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<OpenAiMessage>,
}

#[derive(Serialize)]
struct OpenAiMessage {
    role: String,
    content: OpenAiMessageContent,
}

#[derive(Serialize)]
#[serde(untagged)]
enum OpenAiMessageContent {
    Text(String),
    Blocks(Vec<OpenAiContentBlock>),
}

#[derive(Serialize)]
#[serde(tag = "type")]
enum OpenAiContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: OpenAiImageUrl },
}

#[derive(Serialize)]
struct OpenAiImageUrl {
    url: String,
}

/// OpenAI Chat Completions response structure
#[derive(Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
    #[serde(default)]
    usage: OpenAiUsage,
}

#[derive(Deserialize, Debug, Clone, Copy, Default)]
struct OpenAiUsage {
    #[serde(default)]
    prompt_tokens: u32,
    #[serde(default)]
    completion_tokens: u32,
}

#[derive(Deserialize)]
struct OpenAiChoice {
    message: OpenAiResponseMessage,
}

#[derive(Deserialize)]
struct OpenAiResponseMessage {
    content: Option<String>,
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

    /// Calculate cost for GPT-4o mini usage
    fn calculate_mini_cost(usage: &OpenAiUsage) -> f64 {
        let input_cost = (usage.prompt_tokens as f64 / 1_000_000.0) * MINI_INPUT_COST_PER_M;
        let output_cost = (usage.completion_tokens as f64 / 1_000_000.0) * MINI_OUTPUT_COST_PER_M;
        input_cost + output_cost
    }

    /// Calculate cost for GPT-4o usage
    fn calculate_gpt4o_cost(usage: &OpenAiUsage) -> f64 {
        let input_cost = (usage.prompt_tokens as f64 / 1_000_000.0) * GPT4O_INPUT_COST_PER_M;
        let output_cost = (usage.completion_tokens as f64 / 1_000_000.0) * GPT4O_OUTPUT_COST_PER_M;
        input_cost + output_cost
    }

    /// Build a text-only OpenAI request
    fn text_request(model: &str, max_tokens: u32, prompt: String) -> OpenAiRequest {
        OpenAiRequest {
            model: model.to_string(),
            max_tokens,
            messages: vec![OpenAiMessage {
                role: "user".to_string(),
                content: OpenAiMessageContent::Text(prompt),
            }],
        }
    }

    /// Send an OpenAI chat completion request
    async fn send_request(
        &self,
        request: &OpenAiRequest,
        context: &'static str,
    ) -> Result<reqwest::Response> {
        self.client
            .post(OPENAI_API_URL)
            .bearer_auth(&self.api_key)
            .header("content-type", "application/json")
            .json(request)
            .send()
            .await
            .context(context)
    }

    /// Extract first textual assistant response
    fn extract_response_text(response: &OpenAiResponse) -> Option<String> {
        response
            .choices
            .first()
            .and_then(|c| c.message.content.clone())
            .map(|content| content.trim().to_string())
            .filter(|content| !content.is_empty())
    }

    /// Parse language detection JSON from model output
    fn parse_language_detection(content: &str) -> Option<LanguageDetection> {
        let start = content.find('{')?;
        let end = content.rfind('}')?;
        serde_json::from_str::<LanguageDetection>(&content[start..=end]).ok()
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

        let request = Self::text_request(DETECTION_MODEL, 100, prompt);

        let response = self
            .send_request(&request, "Failed to send language detection request")
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            warn!("Language detection API error: {} - {}", status, body);
            return Ok((true, self.default_language.clone(), UsageInfo::default()));
        }

        let openai_response: OpenAiResponse = response
            .json()
            .await
            .context("Failed to parse language detection response")?;

        // Calculate usage info for GPT-4o mini
        let usage_info = UsageInfo {
            input_tokens: openai_response.usage.prompt_tokens,
            output_tokens: openai_response.usage.completion_tokens,
            cost_usd: Self::calculate_mini_cost(&openai_response.usage),
        };

        debug!(
            "Language detection usage: {} in, {} out, ${:.6}",
            usage_info.input_tokens, usage_info.output_tokens, usage_info.cost_usd
        );

        let content = Self::extract_response_text(&openai_response).unwrap_or_default();

        if let Some(detection) = Self::parse_language_detection(&content) {
            debug!(
                "Detected language: {} (isEnglish: {})",
                detection.language, detection.is_english
            );
            return Ok((detection.is_english, detection.language, usage_info));
        }

        // Fallback: assume default language
        Ok((true, self.default_language.clone(), usage_info))
    }

    /// Translate text to a target language with optional style
    async fn translate(
        &self,
        text: &str,
        source_language: &str,
        target_language: Option<&str>,
        translation_style: Option<&str>,
    ) -> Result<(String, UsageInfo)> {
        let target = target_language.unwrap_or(&self.default_language);

        // Build style instruction if provided
        let style_instruction = match translation_style {
            Some(style) if !style.trim().is_empty() => {
                format!("\nUse a {} tone in the translation.", style.trim())
            }
            _ => String::new(),
        };

        let prompt = format!(
            r#"Translate the following text (from {}) to {}.{}
Respond with ONLY the translated text, nothing else. Preserve the original formatting and meaning as closely as possible.

Text to translate:
{}"#,
            source_language, target, style_instruction, text
        );

        let request = Self::text_request(TRANSLATION_MODEL, 2000, prompt);

        let response = self
            .send_request(&request, "Failed to send translation request")
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            warn!("Translation API error: {} - {}", status, body);
            return Ok((text.to_string(), UsageInfo::default()));
        }

        let openai_response: OpenAiResponse = response
            .json()
            .await
            .context("Failed to parse translation response")?;

        // Calculate usage info for GPT-4o mini
        let usage_info = UsageInfo {
            input_tokens: openai_response.usage.prompt_tokens,
            output_tokens: openai_response.usage.completion_tokens,
            cost_usd: Self::calculate_mini_cost(&openai_response.usage),
        };

        debug!(
            "Translation usage: {} in, {} out, ${:.6}",
            usage_info.input_tokens, usage_info.output_tokens, usage_info.cost_usd
        );

        let translated =
            Self::extract_response_text(&openai_response).unwrap_or_else(|| text.to_string());

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

        let request = Self::text_request(TRANSLATION_MODEL, 2000, prompt);

        let response = self
            .send_request(&request, "Failed to send translation request")
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            warn!("Translation API error: {} - {}", status, body);
            return Ok((text.to_string(), total_usage));
        }

        let openai_response: OpenAiResponse = response
            .json()
            .await
            .context("Failed to parse translation response")?;

        // Calculate usage info for GPT-4o mini
        let translation_usage = UsageInfo {
            input_tokens: openai_response.usage.prompt_tokens,
            output_tokens: openai_response.usage.completion_tokens,
            cost_usd: Self::calculate_mini_cost(&openai_response.usage),
        };
        total_usage = Self::combine_usage(&total_usage, &translation_usage);

        debug!(
            "Outgoing translation usage: {} in, {} out, ${:.6}",
            translation_usage.input_tokens,
            translation_usage.output_tokens,
            translation_usage.cost_usd
        );

        let translated =
            Self::extract_response_text(&openai_response).unwrap_or_else(|| text.to_string());

        Ok((translated.trim().to_string(), total_usage))
    }

    /// Translate outgoing text to a specific target language.
    /// When force=true, always translates even if text appears to already be in target language.
    /// Used for translating outgoing messages when user has set a language override.
    pub async fn translate_outgoing(
        &self,
        text: &str,
        target_language: &str,
        force: bool,
    ) -> Result<(String, UsageInfo)> {
        let mut total_usage = UsageInfo::default();

        // If not forcing, skip if target is the default language (likely English)
        if !force && target_language.to_lowercase() == self.default_language.to_lowercase() {
            return Ok((text.to_string(), total_usage));
        }

        // Detect the source language
        let (_is_target_lang, detected_lang, detection_usage) = self.detect_language(text).await?;
        total_usage = Self::combine_usage(&total_usage, &detection_usage);

        // If not forcing, skip if text is already in target language
        if !force && detected_lang.to_lowercase() == target_language.to_lowercase() {
            debug!(
                "Text already in target language ({}), skipping translation",
                target_language
            );
            return Ok((text.to_string(), total_usage));
        }

        info!(
            "Translating outgoing message from {} to {} (force: {})",
            detected_lang, target_language, force
        );

        let prompt = format!(
            r#"Translate the following text to {}.
Respond with ONLY the translated text, nothing else. Preserve the original formatting, tone, and meaning as closely as possible.

Text to translate:
{}"#,
            target_language, text
        );

        let request = Self::text_request(TRANSLATION_MODEL, 2000, prompt);

        let response = self
            .send_request(&request, "Failed to send translation request")
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            warn!("Translation API error: {} - {}", status, body);
            return Ok((text.to_string(), total_usage));
        }

        let openai_response: OpenAiResponse = response
            .json()
            .await
            .context("Failed to parse translation response")?;

        let translation_usage = UsageInfo {
            input_tokens: openai_response.usage.prompt_tokens,
            output_tokens: openai_response.usage.completion_tokens,
            cost_usd: Self::calculate_mini_cost(&openai_response.usage),
        };
        total_usage = Self::combine_usage(&total_usage, &translation_usage);

        debug!(
            "Outgoing translation usage: {} in, {} out, ${:.6}",
            translation_usage.input_tokens,
            translation_usage.output_tokens,
            translation_usage.cost_usd
        );

        let translated =
            Self::extract_response_text(&openai_response).unwrap_or_else(|| text.to_string());

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
    ///
    /// Parameters:
    /// - text: The text to translate
    /// - language_override: Optional target language override (e.g., "Spanish")
    /// - translation_style: Optional style instruction (e.g., "formal", "casual")
    pub async fn process_text(
        &self,
        text: &str,
        language_override: Option<&str>,
        translation_style: Option<&str>,
    ) -> TranslationResult {
        let mut total_usage = UsageInfo::default();

        // Determine the target language
        let target_language = language_override.unwrap_or(&self.default_language);

        if text.trim().is_empty() {
            return TranslationResult {
                needs_translation: false,
                original_text: text.to_string(),
                translated_text: None,
                source_language: target_language.to_string(),
                usage: total_usage,
            };
        }

        // Step 1: Detect language
        let (is_target_lang, detected_language, detection_usage) =
            match self.detect_language(text).await {
                Ok((is_english, lang, usage)) => {
                    // Check if detected language matches the target language
                    let is_target = if language_override.is_some() {
                        lang.to_lowercase() == target_language.to_lowercase()
                    } else {
                        is_english
                    };
                    (is_target, lang, usage)
                }
                Err(e) => {
                    warn!("Language detection failed: {}", e);
                    (true, target_language.to_string(), UsageInfo::default())
                }
            };
        total_usage = Self::combine_usage(&total_usage, &detection_usage);

        if is_target_lang {
            return TranslationResult {
                needs_translation: false,
                original_text: text.to_string(),
                translated_text: None,
                source_language: detected_language,
                usage: total_usage,
            };
        }

        // Step 2: Translate
        info!(
            "Translating message from {} to {}{}...",
            detected_language,
            target_language,
            translation_style
                .map(|s| format!(" (style: {})", s))
                .unwrap_or_default()
        );
        let (translated, translation_usage) = match self
            .translate(
                text,
                &detected_language,
                language_override,
                translation_style,
            )
            .await
        {
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

        // Build request - include image blocks when available
        let request = if let Some((media_type, base64_data)) = reply_image {
            let mut content_blocks = vec![
                OpenAiContentBlock::ImageUrl {
                    image_url: OpenAiImageUrl {
                        url: format!("data:{};base64,{}", media_type, base64_data),
                    },
                },
                OpenAiContentBlock::Text { text: text_content },
            ];

            if let Some((sender, _)) = reply_context {
                content_blocks.insert(
                    1,
                    OpenAiContentBlock::Text {
                        text: format!("The above image was sent by {}.", sender),
                    },
                );
            }

            OpenAiRequest {
                model: AI_COMPOSE_MODEL.to_string(),
                max_tokens: 300,
                messages: vec![OpenAiMessage {
                    role: "user".to_string(),
                    content: OpenAiMessageContent::Blocks(content_blocks),
                }],
            }
        } else {
            Self::text_request(AI_COMPOSE_MODEL, 300, text_content)
        };

        let response = self
            .send_request(&request, "Failed to send AI compose request")
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("AI compose API error: {} - {}", status, body);
        }

        let openai_response: OpenAiResponse = response
            .json()
            .await
            .context("Failed to parse AI compose response")?;

        let usage_info = UsageInfo {
            input_tokens: openai_response.usage.prompt_tokens,
            output_tokens: openai_response.usage.completion_tokens,
            cost_usd: Self::calculate_gpt4o_cost(&openai_response.usage),
        };

        let composed = Self::extract_response_text(&openai_response).unwrap_or_default();

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
    /// - my_examples: Examples of user's outgoing messages to this contact
    pub async fn compose_styled_reply(
        &self,
        message_to_reply: &crate::storage::StoredMessage,
        recent_conversation: &[crate::storage::StoredMessage],
        global_style: &crate::storage::StyleProfile,
        contact_style: Option<&crate::storage::StyleProfile>,
        my_examples: &[crate::storage::StoredMessage],
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

        // Format my example messages
        let my_examples_formatted = Self::format_my_examples(my_examples);

        // Build the contact-specific style section
        let contact_style_section = if let Some(cs) = contact_style {
            format!(
                "## MY STYLE WITH THIS SPECIFIC CONTACT:\n{}\n",
                cs.profile_text
            )
        } else {
            "## MY STYLE WITH THIS SPECIFIC CONTACT:\nNo specific style data for this contact yet. Use my general style.\n".to_string()
        };

        // Build the full prompt - prioritize examples, be aggressive about casual tone
        let prompt = format!(
            r#"Write a WhatsApp reply AS ME. You must sound EXACTLY like my example messages below.

## MY ACTUAL MESSAGES (COPY THIS STYLE EXACTLY):
{}

## RECENT CHAT FOR CONTEXT:
{}

## REPLYING TO:
{}: "{}"

## STYLE NOTES:
{}
{}

## ABSOLUTE RULES - FOLLOW THESE OR FAIL:
1. BE SHORT. Real WhatsApp messages are 1-2 sentences max, not paragraphs
2. DO NOT start with "Oh" or "Ah" or any filler words - that's AI speak
3. DO NOT over-explain feelings ("I love that", "That's really interesting") - just react naturally  
4. DO NOT write in complete formal sentences if my examples don't
5. DO NOT be more enthusiastic or wordy than my examples show
6. COPY my emoji patterns exactly - if I use "😂" use that, if I don't use emojis, DON'T add them
7. COPY my punctuation - if I skip full stops, skip them. If I use "haha" vs "lol", match it
8. COPY my greeting/sign-off style (xxxxxx, etc) if I use them
9. Sound like a REAL HUMAN texting a friend, not an AI assistant being helpful
10. Output ONLY the message text, nothing else

Write my reply (keep it short and casual like my examples):"#,
            my_examples_formatted,
            conversation_context,
            sender_name,
            reply_to_text,
            global_style.profile_text,
            contact_style_section
        );

        debug!(
            "AI reply prompt length: {} chars, examples: {}, conversation: {} msgs",
            prompt.len(),
            my_examples.len(),
            recent_conversation.len()
        );

        // Call OpenAI - use lower max_tokens to encourage shorter replies
        let request = Self::text_request(AI_COMPOSE_MODEL, 150, prompt);

        let response = self
            .send_request(&request, "Failed to send styled reply request")
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            warn!("AI reply API error: {} - {}", status, body);
            anyhow::bail!("Styled reply API error: {} - {}", status, body);
        }

        let openai_response: OpenAiResponse = response
            .json()
            .await
            .context("Failed to parse styled reply response")?;

        let usage_info = UsageInfo {
            input_tokens: openai_response.usage.prompt_tokens,
            output_tokens: openai_response.usage.completion_tokens,
            cost_usd: Self::calculate_gpt4o_cost(&openai_response.usage),
        };

        let reply = Self::extract_response_text(&openai_response).unwrap_or_default();

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
                // Keep more text for better style matching
                let text = if text.len() > 200 {
                    format!("{}...", &text[..197])
                } else {
                    text
                };
                format!("{}. \"{}\"", i + 1, text)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_language_detection_reads_embedded_json() {
        let content = "Language analysis: {\"language\":\"Spanish\",\"isEnglish\":false}";
        let detection =
            TranslationService::parse_language_detection(content).expect("detection should parse");

        assert_eq!(detection.language, "Spanish");
        assert!(!detection.is_english);
    }

    #[test]
    fn parse_language_detection_returns_none_for_invalid_content() {
        let content = "No JSON here";
        assert!(TranslationService::parse_language_detection(content).is_none());
    }

    #[test]
    fn extract_response_text_returns_first_choice() {
        let response = OpenAiResponse {
            choices: vec![OpenAiChoice {
                message: OpenAiResponseMessage {
                    content: Some("Bonjour".to_string()),
                },
            }],
            usage: OpenAiUsage {
                prompt_tokens: 12,
                completion_tokens: 5,
            },
        };

        let text = TranslationService::extract_response_text(&response);
        assert_eq!(text.as_deref(), Some("Bonjour"));
    }

    #[test]
    fn extract_response_text_returns_none_without_choices() {
        let response = OpenAiResponse {
            choices: vec![],
            usage: OpenAiUsage::default(),
        };

        assert!(TranslationService::extract_response_text(&response).is_none());
    }

    #[test]
    fn request_serializes_vision_payload_in_openai_shape() {
        let request = OpenAiRequest {
            model: "gpt-4o".to_string(),
            max_tokens: 64,
            messages: vec![OpenAiMessage {
                role: "user".to_string(),
                content: OpenAiMessageContent::Blocks(vec![
                    OpenAiContentBlock::ImageUrl {
                        image_url: OpenAiImageUrl {
                            url: "data:image/png;base64,abc123".to_string(),
                        },
                    },
                    OpenAiContentBlock::Text {
                        text: "Describe this image".to_string(),
                    },
                ]),
            }],
        };

        let json = serde_json::to_value(request).expect("request should serialize");

        assert_eq!(json["messages"][0]["content"][0]["type"], "image_url");
        assert_eq!(
            json["messages"][0]["content"][0]["image_url"]["url"],
            "data:image/png;base64,abc123"
        );
        assert_eq!(json["messages"][0]["content"][1]["type"], "text");
    }
}
