//! Translation service using the OpenAI Responses API.

use anyhow::{Context, Result};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::RwLock;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, info, warn};

const OPENAI_API_URL: &str = "https://api.openai.com/v1/responses";
const OPENAI_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const OPENAI_MAX_ATTEMPTS: usize = 3;

const GPT_5_4_INPUT_COST_PER_M: f64 = 2.50;
const GPT_5_4_CACHED_INPUT_COST_PER_M: f64 = 0.25;
const GPT_5_4_OUTPUT_COST_PER_M: f64 = 15.00;

const GPT_5_4_MINI_INPUT_COST_PER_M: f64 = 0.75;
const GPT_5_4_MINI_CACHED_INPUT_COST_PER_M: f64 = 0.075;
const GPT_5_4_MINI_OUTPUT_COST_PER_M: f64 = 4.50;

const GPT_5_4_NANO_INPUT_COST_PER_M: f64 = 0.10;
const GPT_5_4_NANO_CACHED_INPUT_COST_PER_M: f64 = 0.01;
const GPT_5_4_NANO_OUTPUT_COST_PER_M: f64 = 0.625;

const GPT_5_6_SOL_PRICING: PricingTier = PricingTier {
    input_cost_per_m: 5.0,
    cached_input_cost_per_m: 0.5,
    output_cost_per_m: 30.0,
};
const GPT_5_6_TERRA_PRICING: PricingTier = PricingTier {
    input_cost_per_m: 2.5,
    cached_input_cost_per_m: 0.25,
    output_cost_per_m: 15.0,
};
const GPT_5_6_LUNA_PRICING: PricingTier = PricingTier {
    input_cost_per_m: 1.0,
    cached_input_cost_per_m: 0.1,
    output_cost_per_m: 6.0,
};

#[derive(Clone, Copy)]
struct PricingTier {
    input_cost_per_m: f64,
    cached_input_cost_per_m: f64,
    output_cost_per_m: f64,
}

const HIGH_END_PRICING: PricingTier = PricingTier {
    input_cost_per_m: GPT_5_4_INPUT_COST_PER_M,
    cached_input_cost_per_m: GPT_5_4_CACHED_INPUT_COST_PER_M,
    output_cost_per_m: GPT_5_4_OUTPUT_COST_PER_M,
};

const TRANSLATION_PRICING: PricingTier = PricingTier {
    input_cost_per_m: GPT_5_4_MINI_INPUT_COST_PER_M,
    cached_input_cost_per_m: GPT_5_4_MINI_CACHED_INPUT_COST_PER_M,
    output_cost_per_m: GPT_5_4_MINI_OUTPUT_COST_PER_M,
};

const CHEAP_PRICING: PricingTier = PricingTier {
    input_cost_per_m: GPT_5_4_NANO_INPUT_COST_PER_M,
    cached_input_cost_per_m: GPT_5_4_NANO_CACHED_INPUT_COST_PER_M,
    output_cost_per_m: GPT_5_4_NANO_OUTPUT_COST_PER_M,
};

/// Translation service for processing messages and AI replies.
pub struct TranslationService {
    client: Client,
    api_url: String,
    api_key: String,
    detection_model: String,
    translation_model: String,
    high_end_model: String,
    default_language: String,
    runtime_settings: RwLock<crate::storage::OpenAiSettings>,
}

/// Result of processing a message for translation.
#[derive(Debug, Clone)]
pub struct TranslationResult {
    pub needs_translation: bool,
    pub original_text: String,
    pub translated_text: Option<String>,
    pub source_language: String,
    pub usage: UsageInfo,
}

/// Token usage and cost information.
#[derive(Debug, Clone, Default)]
pub struct UsageInfo {
    pub input_tokens: u32,
    pub cached_input_tokens: u32,
    pub output_tokens: u32,
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
    input_tokens: u32,
    #[serde(default)]
    output_tokens: u32,
    input_tokens_details: Option<InputTokensDetails>,
}

#[derive(Deserialize, Debug, Clone, Copy, Default)]
struct InputTokensDetails {
    #[serde(default)]
    cached_tokens: u32,
}

#[derive(Deserialize)]
struct LanguageDetection {
    language: String,
    #[serde(rename = "isTargetLanguage", alias = "is_target_language")]
    is_target_language: bool,
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

impl TranslationService {
    pub fn new(
        api_key: String,
        detection_model: String,
        translation_model: String,
        high_end_model: String,
        default_language: String,
    ) -> Self {
        info!(
            "Translation service initialized with OpenAI models (target: {})",
            default_language
        );
        Self {
            client: Self::build_http_client(),
            api_url: OPENAI_API_URL.to_string(),
            api_key,
            detection_model,
            translation_model,
            high_end_model,
            default_language,
            runtime_settings: RwLock::new(Default::default()),
        }
    }

    #[cfg(test)]
    fn new_with_api_url(api_url: String) -> Self {
        Self {
            client: Self::build_http_client(),
            api_url,
            api_key: "test-api-key".to_string(),
            detection_model: "test-detect".to_string(),
            translation_model: "test-translate".to_string(),
            high_end_model: "test-high-end".to_string(),
            default_language: "English".to_string(),
            runtime_settings: RwLock::new(Default::default()),
        }
    }

    fn build_http_client() -> Client {
        Client::builder()
            .timeout(OPENAI_REQUEST_TIMEOUT)
            .build()
            .expect("OpenAI HTTP client should build")
    }

    pub fn get_api_key(&self) -> String {
        self.api_key.clone()
    }

    pub fn get_detection_model(&self) -> String {
        self.runtime_settings
            .read()
            .unwrap()
            .model
            .clone()
            .unwrap_or_else(|| self.detection_model.clone())
    }

    pub fn get_reasoning_effort(&self) -> Option<String> {
        self.runtime_settings
            .read()
            .unwrap()
            .reasoning_effort
            .clone()
    }

    pub fn set_runtime_settings(&self, settings: crate::storage::OpenAiSettings) {
        *self.runtime_settings.write().unwrap() = settings;
    }

    fn pricing_for_model(model: &str, fallback: PricingTier) -> PricingTier {
        match model {
            "gpt-5.6-sol" => GPT_5_6_SOL_PRICING,
            "gpt-5.6-terra" => GPT_5_6_TERRA_PRICING,
            "gpt-5.6-luna" => GPT_5_6_LUNA_PRICING,
            _ => fallback,
        }
    }

    fn usage_from_api(usage: Option<ApiUsage>, pricing: PricingTier) -> UsageInfo {
        let usage = usage.unwrap_or_default();
        let cached_input_tokens = usage
            .input_tokens_details
            .unwrap_or_default()
            .cached_tokens
            .min(usage.input_tokens);
        let uncached_input_tokens = usage.input_tokens.saturating_sub(cached_input_tokens);

        let input_cost = (uncached_input_tokens as f64 / 1_000_000.0) * pricing.input_cost_per_m;
        let cached_input_cost =
            (cached_input_tokens as f64 / 1_000_000.0) * pricing.cached_input_cost_per_m;
        let output_cost = (usage.output_tokens as f64 / 1_000_000.0) * pricing.output_cost_per_m;

        UsageInfo {
            input_tokens: usage.input_tokens,
            cached_input_tokens,
            output_tokens: usage.output_tokens,
            cost_usd: input_cost + cached_input_cost + output_cost,
        }
    }

    fn combine_usage(a: &UsageInfo, b: &UsageInfo) -> UsageInfo {
        UsageInfo {
            input_tokens: a.input_tokens + b.input_tokens,
            cached_input_tokens: a.cached_input_tokens + b.cached_input_tokens,
            output_tokens: a.output_tokens + b.output_tokens,
            cost_usd: a.cost_usd + b.cost_usd,
        }
    }

    fn extract_output_text(response: &OpenAiResponse) -> String {
        response
            .output
            .iter()
            .filter(|item| item.item_type == "message")
            .filter_map(|item| item.content.as_ref())
            .flat_map(|content| content.iter())
            .filter(|part| part.part_type == "output_text")
            .filter_map(|part| part.text.as_deref())
            .collect::<Vec<_>>()
            .join("")
    }

    fn extract_json_object(text: &str) -> Option<&str> {
        let start = text.find('{')?;
        let end = text.rfind('}')?;
        Some(&text[start..=end])
    }

    fn truncate_for_display(text: String, max_len: usize) -> String {
        if text.chars().count() > max_len {
            let truncated: String = text.chars().take(max_len.saturating_sub(3)).collect();
            format!("{}...", truncated)
        } else {
            text
        }
    }

    fn build_data_url(media_type: &str, base64_data: &str) -> String {
        if base64_data.starts_with("data:") {
            base64_data.to_string()
        } else {
            format!("data:{};base64,{}", media_type, base64_data)
        }
    }

    async fn send_request(&self, body: Value) -> Result<OpenAiResponse> {
        for attempt in 0..OPENAI_MAX_ATTEMPTS {
            let response = self
                .client
                .post(&self.api_url)
                .bearer_auth(&self.api_key)
                .header("content-type", "application/json")
                .json(&body)
                .send()
                .await;

            let response = match response {
                Ok(response) => response,
                Err(error)
                    if should_retry_reqwest_error(&error) && attempt + 1 < OPENAI_MAX_ATTEMPTS =>
                {
                    warn!(
                        "OpenAI request attempt {} failed, retrying: {}",
                        attempt + 1,
                        error
                    );
                    sleep(openai_retry_delay(attempt)).await;
                    continue;
                }
                Err(error) => {
                    return Err(error).context("Failed to send OpenAI Responses API request")
                }
            };

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                if should_retry_status(status) && attempt + 1 < OPENAI_MAX_ATTEMPTS {
                    warn!(
                        "OpenAI request attempt {} returned {}, retrying",
                        attempt + 1,
                        status
                    );
                    sleep(openai_retry_delay(attempt)).await;
                    continue;
                }
                anyhow::bail!("OpenAI Responses API error: {} - {}", status, body);
            }

            return response
                .json()
                .await
                .context("Failed to parse OpenAI response");
        }

        unreachable!("OpenAI retry loop should return or error");
    }

    async fn request_text_output(
        &self,
        model: &str,
        pricing: PricingTier,
        instructions: &str,
        input: Value,
        max_output_tokens: u32,
        reasoning_effort: Option<&str>,
        verbosity: Option<&str>,
        json_mode: bool,
    ) -> Result<(String, UsageInfo)> {
        let overrides = self.runtime_settings.read().unwrap().clone();
        let model = overrides.model.as_deref().unwrap_or(model);
        let reasoning_effort = overrides.reasoning_effort.as_deref().or(reasoning_effort);
        let pricing = Self::pricing_for_model(model, pricing);
        let mut body = json!({
            "model": model,
            "instructions": instructions,
            "input": input,
            "max_output_tokens": max_output_tokens,
        });

        if let Some(effort) = reasoning_effort {
            body["reasoning"] = json!({ "effort": effort });
        }

        let mut text_settings = serde_json::Map::new();
        if let Some(level) = verbosity {
            text_settings.insert("verbosity".to_string(), json!(level));
        }
        if json_mode {
            text_settings.insert("format".to_string(), json!({ "type": "json_object" }));
        }
        if !text_settings.is_empty() {
            body["text"] = Value::Object(text_settings);
        }

        let response = self.send_request(body).await?;
        let usage = Self::usage_from_api(response.usage, pricing);
        Ok((Self::extract_output_text(&response), usage))
    }

    async fn detect_language(
        &self,
        text: &str,
        target_language: &str,
    ) -> Result<(bool, String, UsageInfo)> {
        let instructions = format!(
            "Detect the language of the provided text. Set isTargetLanguage to true only if the text is already written primarily in {}.",
            target_language
        );
        let input_text = format!(
            "Return JSON only. Respond with a JSON object in this exact shape: {{\"language\":\"Language Name\",\"isTargetLanguage\":true}}.\n\nText: {}",
            text.chars().take(500).collect::<String>()
        );

        let (content, usage) = self
            .request_text_output(
                &self.detection_model,
                CHEAP_PRICING,
                &instructions,
                json!(input_text),
                120,
                Some("none"),
                None,
                true,
            )
            .await?;

        debug!(
            "Language detection usage: {} in ({} cached), {} out, ${:.6}",
            usage.input_tokens, usage.cached_input_tokens, usage.output_tokens, usage.cost_usd
        );

        if let Some(json_str) = Self::extract_json_object(&content) {
            if let Ok(detection) = serde_json::from_str::<LanguageDetection>(json_str) {
                return Ok((detection.is_target_language, detection.language, usage));
            }
        }

        Ok((true, target_language.to_string(), usage))
    }

    async fn translate(
        &self,
        text: &str,
        source_language: &str,
        target_language: &str,
        translation_style: Option<&str>,
    ) -> Result<(String, UsageInfo)> {
        let style_instruction = match translation_style {
            Some(style) if !style.trim().is_empty() => {
                format!("\nUse a {} tone in the translation.", style.trim())
            }
            _ => String::new(),
        };

        let instructions = format!(
            "Translate the user's text from {} to {}.{} Respond with only the translated text. Preserve formatting, tone, and meaning as closely as possible.",
            source_language, target_language, style_instruction
        );

        let (translated, usage) = self
            .request_text_output(
                &self.translation_model,
                TRANSLATION_PRICING,
                &instructions,
                json!(text),
                2000,
                Some("none"),
                Some("low"),
                false,
            )
            .await?;

        debug!(
            "Translation usage: {} in ({} cached), {} out, ${:.6}",
            usage.input_tokens, usage.cached_input_tokens, usage.output_tokens, usage.cost_usd
        );

        Ok((translated.trim().to_string(), usage))
    }

    pub async fn translate_to(
        &self,
        text: &str,
        target_language: &str,
    ) -> Result<(String, UsageInfo)> {
        let mut total_usage = UsageInfo::default();

        if target_language.eq_ignore_ascii_case(&self.default_language) {
            return Ok((text.to_string(), total_usage));
        }

        let (is_target_lang, detected_lang, detection_usage) =
            self.detect_language(text, target_language).await?;
        total_usage = Self::combine_usage(&total_usage, &detection_usage);

        if is_target_lang || detected_lang.eq_ignore_ascii_case(target_language) {
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

        let (translated, translation_usage) = self
            .translate(text, &detected_lang, target_language, None)
            .await?;
        total_usage = Self::combine_usage(&total_usage, &translation_usage);

        debug!(
            "Outgoing translation usage: {} in ({} cached), {} out, ${:.6}",
            translation_usage.input_tokens,
            translation_usage.cached_input_tokens,
            translation_usage.output_tokens,
            translation_usage.cost_usd
        );

        Ok((translated, total_usage))
    }

    pub async fn translate_outgoing(
        &self,
        text: &str,
        target_language: &str,
    ) -> Result<(String, UsageInfo)> {
        let mut total_usage = UsageInfo::default();

        if target_language.eq_ignore_ascii_case(&self.default_language) {
            return Ok((text.to_string(), total_usage));
        }

        let (is_target_lang, detected_lang, detection_usage) =
            self.detect_language(text, target_language).await?;
        total_usage = Self::combine_usage(&total_usage, &detection_usage);

        if is_target_lang || detected_lang.eq_ignore_ascii_case(target_language) {
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

        let (translated, translation_usage) = self
            .translate(text, &detected_lang, target_language, None)
            .await?;
        total_usage = Self::combine_usage(&total_usage, &translation_usage);

        debug!(
            "Outgoing translation usage: {} in ({} cached), {} out, ${:.6}",
            translation_usage.input_tokens,
            translation_usage.cached_input_tokens,
            translation_usage.output_tokens,
            translation_usage.cost_usd
        );

        Ok((translated, total_usage))
    }

    pub async fn process_text(
        &self,
        text: &str,
        _contact_language: Option<&str>,
        translation_style: Option<&str>,
    ) -> Result<TranslationResult> {
        let mut total_usage = UsageInfo::default();
        // A conversation language identifies the contact's language. Incoming
        // text must always be translated in the other direction, back to the
        // app owner's default language.
        let target_language = &self.default_language;

        if text.trim().is_empty() {
            return Ok(TranslationResult {
                needs_translation: false,
                original_text: text.to_string(),
                translated_text: None,
                source_language: target_language.to_string(),
                usage: total_usage,
            });
        }

        let (is_target_lang, detected_language, detection_usage) = self
            .detect_language(text, target_language)
            .await
            .context("Language detection failed")?;
        total_usage = Self::combine_usage(&total_usage, &detection_usage);

        if is_target_lang {
            return Ok(TranslationResult {
                needs_translation: false,
                original_text: text.to_string(),
                translated_text: None,
                source_language: detected_language,
                usage: total_usage,
            });
        }

        info!(
            "Translating message from {} to {}{}...",
            detected_language,
            target_language,
            translation_style
                .map(|s| format!(" (style: {})", s))
                .unwrap_or_default()
        );

        let (translated, translation_usage) = self
            .translate(text, &detected_language, target_language, translation_style)
            .await
            .context("Translation failed")?;
        total_usage = Self::combine_usage(&total_usage, &translation_usage);

        info!(
            "Translation complete - total usage: {} in ({} cached), {} out, ${:.6}",
            total_usage.input_tokens,
            total_usage.cached_input_tokens,
            total_usage.output_tokens,
            total_usage.cost_usd
        );

        Ok(TranslationResult {
            needs_translation: true,
            original_text: text.to_string(),
            translated_text: Some(translated),
            source_language: detected_language,
            usage: total_usage,
        })
    }

    pub async fn compose_ai_message(
        &self,
        prompt: &str,
        reply_context: Option<(&str, &str)>,
        reply_image: Option<(&str, &str)>,
    ) -> Result<(String, UsageInfo)> {
        if prompt.trim().is_empty() {
            anyhow::bail!("Prompt cannot be empty");
        }
        if prompt.len() > 1000 {
            anyhow::bail!("Prompt is too long (max 1000 characters)");
        }

        let instructions = r#"You are a helpful assistant composing WhatsApp messages. Your task is to write a message based on the user's request.

IMPORTANT RULES:
1. Keep your response SHORT and appropriate for a chat message (max 500 characters)
2. Write ONLY the message content - no explanations, no quotes, no "Here's a message:" prefixes
3. Be conversational and natural, matching the tone requested
4. Do not include anything harmful, offensive, or inappropriate
5. If the request is unclear, write a friendly, neutral message
6. Do not pretend to be someone specific or impersonate anyone
7. Do not include private information or make up facts about real people
8. If an image is provided, you can reference what you see in it when composing your reply"#;

        let text_content = if let Some((sender, text)) = reply_context {
            format!(
                "The user is replying to this message from {}:\n\"{}\"\n\nUser request for their reply: {}",
                sender,
                text.chars().take(500).collect::<String>(),
                prompt
            )
        } else {
            format!("User request: {}", prompt)
        };

        let input = if let Some((media_type, base64_data)) = reply_image {
            let mut content = vec![
                json!({
                    "type": "input_image",
                    "image_url": Self::build_data_url(media_type, base64_data),
                }),
                json!({
                    "type": "input_text",
                    "text": text_content,
                }),
            ];

            if let Some((sender, _)) = reply_context {
                content.insert(
                    1,
                    json!({
                        "type": "input_text",
                        "text": format!("The above image was sent by {}.", sender),
                    }),
                );
            }

            json!([{
                "role": "user",
                "content": content,
            }])
        } else {
            json!(text_content)
        };

        let (composed, usage_info) = self
            .request_text_output(
                &self.high_end_model,
                HIGH_END_PRICING,
                instructions,
                input,
                300,
                Some("medium"),
                Some("low"),
                false,
            )
            .await?;

        let composed = Self::truncate_for_display(composed.trim().to_string(), 500);

        info!(
            "AI compose usage: {} in ({} cached), {} out, ${:.6}",
            usage_info.input_tokens,
            usage_info.cached_input_tokens,
            usage_info.output_tokens,
            usage_info.cost_usd
        );

        Ok((composed, usage_info))
    }

    pub async fn compose_styled_reply(
        &self,
        message_to_reply: &crate::storage::StoredMessage,
        recent_conversation: &[crate::storage::StoredMessage],
        global_style: &crate::storage::StyleProfile,
        contact_style: Option<&crate::storage::StyleProfile>,
        my_examples: &[crate::storage::StoredMessage],
    ) -> Result<(String, UsageInfo)> {
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
        let reply_to_text = Self::truncate_for_display(reply_to_text, 500);

        let sender_name = message_to_reply
            .sender_name
            .clone()
            .or_else(|| message_to_reply.contact_name.clone())
            .unwrap_or_else(|| "Someone".to_string());

        let conversation_context = Self::format_conversation(recent_conversation);
        let my_examples_formatted = Self::format_my_examples(my_examples);

        let contact_style_section = if let Some(cs) = contact_style {
            format!(
                "## MY STYLE WITH THIS SPECIFIC CONTACT:\n{}\n",
                cs.profile_text
            )
        } else {
            "## MY STYLE WITH THIS SPECIFIC CONTACT:\nNo specific style data for this contact yet. Use my general style.\n".to_string()
        };

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

        let (mut reply, mut usage_info) = self
            .request_text_output(
                &self.high_end_model,
                HIGH_END_PRICING,
                "Generate a short WhatsApp reply that follows the user's style exactly. Output only the reply text.",
                json!(prompt.clone()),
                150,
                Some("none"),
                Some("low"),
                false,
            )
            .await?;

        if reply.trim().is_empty() {
            warn!("Styled reply returned no text, retrying with a simpler prompt");
            let retry_prompt = format!(
                "{}\n\nFINAL REQUIREMENT: Reply with one short WhatsApp message right now. Do not leave the answer blank.",
                prompt
            );
            let (retry_reply, retry_usage) = self
                .request_text_output(
                    &self.high_end_model,
                    HIGH_END_PRICING,
                    "Write exactly one short WhatsApp reply in the user's style. Output only the reply text.",
                    json!(retry_prompt),
                    220,
                    Some("none"),
                    Some("low"),
                    false,
                )
                .await?;
            reply = retry_reply;
            usage_info = Self::combine_usage(&usage_info, &retry_usage);
        }

        let reply = Self::truncate_for_display(reply.trim().to_string(), 500);

        if reply.is_empty() {
            anyhow::bail!("OpenAI returned an empty styled reply");
        }

        info!(
            "Styled reply generated: {} chars, {} in ({} cached), {} out, ${:.6}",
            reply.len(),
            usage_info.input_tokens,
            usage_info.cached_input_tokens,
            usage_info.output_tokens,
            usage_info.cost_usd
        );

        Ok((reply, usage_info))
    }

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

                format!("{}: {}", sender, Self::truncate_for_display(text, 200))
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

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
            .map(|(i, text)| format!("{}. \"{}\"", i + 1, Self::truncate_for_display(text, 200)))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::{TranslationService, UsageInfo, CHEAP_PRICING};
    use crate::storage::OpenAiSettings;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    struct MockResponse {
        status: &'static str,
        body: String,
    }

    #[test]
    fn runtime_settings_override_model_and_reasoning() {
        let service = TranslationService::new_with_api_url("http://example.test".to_string());
        service.set_runtime_settings(OpenAiSettings {
            model: Some("gpt-5.6-sol".to_string()),
            reasoning_effort: Some("xhigh".to_string()),
        });

        assert_eq!(service.get_detection_model(), "gpt-5.6-sol");
        assert_eq!(service.get_reasoning_effort().as_deref(), Some("xhigh"));
    }

    fn spawn_openai_mock(responses: Vec<MockResponse>) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let addr = listener.local_addr().expect("mock server addr");
        let handle = thread::spawn(move || {
            for response in responses {
                let (mut stream, _) = listener.accept().expect("accept mock request");
                let _ = read_http_request(&mut stream);
                let body = response.body.as_bytes();
                write!(
                    stream,
                    "HTTP/1.1 {}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    response.status,
                    body.len()
                )
                .expect("write response headers");
                stream.write_all(body).expect("write response body");
            }
        });

        (format!("http://{}", addr), handle)
    }

    fn spawn_capturing_openai_mock(
        responses: Vec<MockResponse>,
    ) -> (String, Arc<Mutex<Vec<String>>>, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let addr = listener.local_addr().expect("mock server addr");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured_requests = Arc::clone(&requests);
        let handle = thread::spawn(move || {
            for response in responses {
                let (mut stream, _) = listener.accept().expect("accept mock request");
                captured_requests
                    .lock()
                    .expect("lock captured requests")
                    .push(read_http_request(&mut stream));
                let body = response.body.as_bytes();
                write!(
                    stream,
                    "HTTP/1.1 {}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    response.status,
                    body.len()
                )
                .expect("write response headers");
                stream.write_all(body).expect("write response body");
            }
        });

        (format!("http://{}", addr), requests, handle)
    }

    fn read_http_request(stream: &mut TcpStream) -> String {
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("set read timeout");

        let mut data = Vec::new();
        let mut buffer = [0_u8; 1024];
        let header_end = loop {
            let bytes_read = stream.read(&mut buffer).expect("read mock request");
            if bytes_read == 0 {
                break data.len();
            }
            data.extend_from_slice(&buffer[..bytes_read]);
            if let Some(position) = data.windows(4).position(|window| window == b"\r\n\r\n") {
                break position + 4;
            }
        };

        let headers = String::from_utf8_lossy(&data[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| line.strip_prefix("content-length:"))
            .or_else(|| {
                headers
                    .lines()
                    .find_map(|line| line.strip_prefix("Content-Length:"))
            })
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(0);
        let expected_len = header_end + content_length;

        while data.len() < expected_len {
            let bytes_read = stream.read(&mut buffer).expect("read mock body");
            if bytes_read == 0 {
                break;
            }
            data.extend_from_slice(&buffer[..bytes_read]);
        }

        String::from_utf8(data).expect("mock request should be UTF-8")
    }

    #[test]
    fn calculates_cost_with_cached_input_tokens() {
        let usage = super::ApiUsage {
            input_tokens: 1_000_000,
            output_tokens: 1_000_000,
            input_tokens_details: Some(super::InputTokensDetails {
                cached_tokens: 250_000,
            }),
        };

        let priced = TranslationService::usage_from_api(Some(usage), CHEAP_PRICING);

        assert_eq!(priced.input_tokens, 1_000_000);
        assert_eq!(priced.cached_input_tokens, 250_000);
        assert_eq!(priced.output_tokens, 1_000_000);
        assert!((priced.cost_usd - 0.7025).abs() < 0.000001);
    }

    #[test]
    fn extracts_json_object_from_wrapped_text() {
        let wrapped = "```json\n{\"language\":\"Spanish\",\"isTargetLanguage\":false}\n```";
        assert_eq!(
            TranslationService::extract_json_object(wrapped),
            Some("{\"language\":\"Spanish\",\"isTargetLanguage\":false}")
        );
    }

    #[test]
    fn builds_data_url_from_raw_base64() {
        assert_eq!(
            TranslationService::build_data_url("image/jpeg", "abc123"),
            "data:image/jpeg;base64,abc123"
        );
    }

    #[test]
    fn preserves_existing_data_url() {
        let data_url = "data:image/png;base64,abc123";
        assert_eq!(
            TranslationService::build_data_url("image/png", data_url),
            data_url
        );
    }

    #[test]
    fn combines_cached_usage() {
        let combined = TranslationService::combine_usage(
            &UsageInfo {
                input_tokens: 10,
                cached_input_tokens: 3,
                output_tokens: 4,
                cost_usd: 1.0,
            },
            &UsageInfo {
                input_tokens: 8,
                cached_input_tokens: 2,
                output_tokens: 5,
                cost_usd: 2.0,
            },
        );

        assert_eq!(combined.input_tokens, 18);
        assert_eq!(combined.cached_input_tokens, 5);
        assert_eq!(combined.output_tokens, 9);
        assert_eq!(combined.cost_usd, 3.0);
    }

    #[tokio::test]
    async fn process_text_returns_error_when_translation_request_fails() {
        let detection_response = r#"{
            "output": [{
                "type": "message",
                "content": [{
                    "type": "output_text",
                    "text": "{\"language\":\"Spanish\",\"isTargetLanguage\":false}"
                }]
            }],
            "usage": {
                "input_tokens": 12,
                "output_tokens": 4,
                "input_tokens_details": { "cached_tokens": 2 }
            }
        }"#;
        let (api_url, server) = spawn_openai_mock(vec![
            MockResponse {
                status: "200 OK",
                body: detection_response.to_string(),
            },
            MockResponse {
                status: "500 Internal Server Error",
                body: r#"{"error":"translation unavailable"}"#.to_string(),
            },
            MockResponse {
                status: "500 Internal Server Error",
                body: r#"{"error":"translation still unavailable"}"#.to_string(),
            },
            MockResponse {
                status: "500 Internal Server Error",
                body: r#"{"error":"translation failed"}"#.to_string(),
            },
        ]);
        let service = TranslationService::new_with_api_url(api_url);

        let error = service
            .process_text("hola, podemos hablar mañana?", None, None)
            .await
            .expect_err("translation failure should be returned");

        assert!(
            error.to_string().contains("Translation failed"),
            "unexpected error: {error:#}"
        );
        server.join().expect("mock server should finish");
    }

    #[tokio::test]
    async fn configured_contact_language_translates_incoming_text_back_to_default_language() {
        let detection_response = r#"{
            "output": [{
                "type": "message",
                "content": [{
                    "type": "output_text",
                    "text": "{\"language\":\"Hungarian\",\"isTargetLanguage\":false}"
                }]
            }],
            "usage": { "input_tokens": 8, "output_tokens": 3 }
        }"#;
        let translation_response = r#"{
            "output": [{
                "type": "message",
                "content": [{ "type": "output_text", "text": "Good morning" }]
            }],
            "usage": { "input_tokens": 10, "output_tokens": 2 }
        }"#;
        let (api_url, requests, server) = spawn_capturing_openai_mock(vec![
            MockResponse {
                status: "200 OK",
                body: detection_response.to_string(),
            },
            MockResponse {
                status: "200 OK",
                body: translation_response.to_string(),
            },
        ]);
        let service = TranslationService::new_with_api_url(api_url);

        let result = service
            .process_text("Jó reggelt", Some("Hungarian"), None)
            .await
            .expect("incoming Hungarian message should translate");

        assert!(result.needs_translation);
        assert_eq!(result.translated_text.as_deref(), Some("Good morning"));
        let requests = requests.lock().expect("lock captured requests");
        assert!(requests[0].contains("primarily in English"));
        assert!(requests[1].contains("Hungarian to English"));
        server.join().expect("mock server should finish");
    }

    #[tokio::test]
    async fn short_outgoing_text_is_still_translated_to_configured_contact_language() {
        let detection_response = r#"{
            "output": [{
                "type": "message",
                "content": [{
                    "type": "output_text",
                    "text": "{\"language\":\"English\",\"isTargetLanguage\":false}"
                }]
            }],
            "usage": { "input_tokens": 6, "output_tokens": 3 }
        }"#;
        let translation_response = r#"{
            "output": [{
                "type": "message",
                "content": [{ "type": "output_text", "text": "Szia" }]
            }],
            "usage": { "input_tokens": 7, "output_tokens": 2 }
        }"#;
        let (api_url, requests, server) = spawn_capturing_openai_mock(vec![
            MockResponse {
                status: "200 OK",
                body: detection_response.to_string(),
            },
            MockResponse {
                status: "200 OK",
                body: translation_response.to_string(),
            },
        ]);
        let service = TranslationService::new_with_api_url(api_url);

        let (translated, _) = service
            .translate_outgoing("Hi", "Hungarian")
            .await
            .expect("short outgoing English message should translate");

        assert_eq!(translated, "Szia");
        let requests = requests.lock().expect("lock captured requests");
        assert!(requests[0].contains("primarily in Hungarian"));
        assert!(requests[1].contains("English to Hungarian"));
        server.join().expect("mock server should finish");
    }
}
