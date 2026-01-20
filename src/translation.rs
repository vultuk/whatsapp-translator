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
}

#[derive(Deserialize)]
struct ContentBlock {
    text: Option<String>,
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

    /// Detect if text is in the default language
    async fn detect_language(&self, text: &str) -> Result<(bool, String)> {
        // Skip very short messages
        if text.trim().len() < 5 {
            return Ok((true, self.default_language.clone()));
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
            return Ok((true, self.default_language.clone()));
        }

        let claude_response: ClaudeResponse = response
            .json()
            .await
            .context("Failed to parse language detection response")?;

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
                    return Ok((detection.is_english, detection.language));
                }
            }
        }

        // Fallback: assume default language
        Ok((true, self.default_language.clone()))
    }

    /// Translate text to the default language
    async fn translate(&self, text: &str, source_language: &str) -> Result<String> {
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
            return Ok(text.to_string());
        }

        let claude_response: ClaudeResponse = response
            .json()
            .await
            .context("Failed to parse translation response")?;

        let translated = claude_response
            .content
            .first()
            .and_then(|c| c.text.clone())
            .unwrap_or_else(|| text.to_string());

        Ok(translated.trim().to_string())
    }

    /// Translate text to a specific target language.
    /// Used for translating outgoing messages to match the conversation language.
    pub async fn translate_to(&self, text: &str, target_language: &str) -> Result<String> {
        // Skip if target is the default language (likely English)
        if target_language.to_lowercase() == self.default_language.to_lowercase() {
            return Ok(text.to_string());
        }

        // First detect if the text is already in the target language
        let (_is_target_lang, detected_lang) = self.detect_language(text).await?;

        // If the text appears to be in the target language already, skip translation
        if detected_lang.to_lowercase() == target_language.to_lowercase() {
            debug!(
                "Text already in target language ({}), skipping translation",
                target_language
            );
            return Ok(text.to_string());
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
            return Ok(text.to_string());
        }

        let claude_response: ClaudeResponse = response
            .json()
            .await
            .context("Failed to parse translation response")?;

        let translated = claude_response
            .content
            .first()
            .and_then(|c| c.text.clone())
            .unwrap_or_else(|| text.to_string());

        Ok(translated.trim().to_string())
    }

    /// Process a message - detect language and translate if needed
    pub async fn process_text(&self, text: &str) -> TranslationResult {
        if text.trim().is_empty() {
            return TranslationResult {
                needs_translation: false,
                original_text: text.to_string(),
                translated_text: None,
                source_language: self.default_language.clone(),
            };
        }

        // Step 1: Detect language
        let (is_default, detected_language) = match self.detect_language(text).await {
            Ok(result) => result,
            Err(e) => {
                warn!("Language detection failed: {}", e);
                (true, self.default_language.clone())
            }
        };

        if is_default {
            return TranslationResult {
                needs_translation: false,
                original_text: text.to_string(),
                translated_text: None,
                source_language: detected_language,
            };
        }

        // Step 2: Translate
        info!("Translating message from {}...", detected_language);
        let translated = match self.translate(text, &detected_language).await {
            Ok(t) => t,
            Err(e) => {
                warn!("Translation failed: {}", e);
                text.to_string()
            }
        };

        TranslationResult {
            needs_translation: true,
            original_text: text.to_string(),
            translated_text: Some(translated),
            source_language: detected_language,
        }
    }
}
