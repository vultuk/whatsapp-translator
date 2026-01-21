//! Link preview fetching - extracts Open Graph metadata from URLs.

use anyhow::{Context, Result};
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::debug;

/// Maximum response size to fetch (1MB)
const MAX_RESPONSE_SIZE: usize = 1024 * 1024;

/// Request timeout
const FETCH_TIMEOUT: Duration = Duration::from_secs(5);

/// Link preview metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkPreview {
    pub url: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub image_url: Option<String>,
    pub site_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl LinkPreview {
    /// Create an error result
    pub fn error(url: String, error: String) -> Self {
        Self {
            url,
            title: None,
            description: None,
            image_url: None,
            site_name: None,
            error: Some(error),
        }
    }
}

/// Extract URLs from text
pub fn extract_urls(text: &str) -> Vec<String> {
    // Match URLs starting with http:// or https://
    // This regex is intentionally simple but handles most common cases
    let url_regex = Regex::new(r"https?://[^\s<>\[\](){}|\\^`\x00-\x1f\x7f]+").unwrap();

    url_regex
        .find_iter(text)
        .map(|m| {
            let mut url = m.as_str().to_string();
            // Remove trailing punctuation that's likely not part of the URL
            while url.ends_with(['.', ',', '!', '?', ')', ']', '}', ';', ':', '\'', '"']) {
                url.pop();
            }
            url
        })
        .collect()
}

/// Fetch link preview metadata from a URL
pub async fn fetch_link_preview(url: &str) -> Result<LinkPreview> {
    let client = Client::builder()
        .timeout(FETCH_TIMEOUT)
        .user_agent("Mozilla/5.0 (compatible; WhatsAppTranslator/1.0; +https://github.com/vultuk/whatsapp-translator)")
        .build()
        .context("Failed to create HTTP client")?;

    debug!("Fetching link preview for: {}", url);

    let response = client
        .get(url)
        .send()
        .await
        .context("Failed to fetch URL")?;

    // Check status
    if !response.status().is_success() {
        return Ok(LinkPreview::error(
            url.to_string(),
            format!("HTTP {}", response.status()),
        ));
    }

    // Check content type - only process HTML
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !content_type.contains("text/html") {
        return Ok(LinkPreview::error(
            url.to_string(),
            "Not an HTML page".to_string(),
        ));
    }

    // Fetch body with size limit
    let body = response
        .bytes()
        .await
        .context("Failed to read response body")?;

    if body.len() > MAX_RESPONSE_SIZE {
        return Ok(LinkPreview::error(
            url.to_string(),
            "Response too large".to_string(),
        ));
    }

    // Parse HTML and extract metadata
    let html = String::from_utf8_lossy(&body);
    let preview = parse_html_metadata(url, &html);

    debug!(
        "Link preview for {}: title={:?}, image={:?}",
        url, preview.title, preview.image_url
    );

    Ok(preview)
}

/// Parse HTML and extract Open Graph / meta tags
fn parse_html_metadata(url: &str, html: &str) -> LinkPreview {
    let mut preview = LinkPreview {
        url: url.to_string(),
        title: None,
        description: None,
        image_url: None,
        site_name: None,
        error: None,
    };

    // Extract Open Graph tags
    preview.title = extract_meta_content(html, "og:title");
    preview.description = extract_meta_content(html, "og:description");
    preview.image_url = extract_meta_content(html, "og:image");
    preview.site_name = extract_meta_content(html, "og:site_name");

    // Fallback to Twitter Card tags
    if preview.title.is_none() {
        preview.title = extract_meta_content(html, "twitter:title");
    }
    if preview.description.is_none() {
        preview.description = extract_meta_content(html, "twitter:description");
    }
    if preview.image_url.is_none() {
        preview.image_url = extract_meta_content(html, "twitter:image");
    }

    // Fallback to standard HTML tags
    if preview.title.is_none() {
        preview.title = extract_html_title(html);
    }
    if preview.description.is_none() {
        preview.description = extract_meta_content(html, "description");
    }

    // Make relative image URLs absolute
    if let Some(ref img) = preview.image_url {
        if img.starts_with('/') {
            if let Ok(base_url) = reqwest::Url::parse(url) {
                if let Ok(absolute) = base_url.join(img) {
                    preview.image_url = Some(absolute.to_string());
                }
            }
        }
    }

    // Truncate long descriptions
    if let Some(ref desc) = preview.description {
        if desc.len() > 200 {
            preview.description = Some(format!("{}...", &desc[..197]));
        }
    }

    preview
}

/// Extract meta tag content by property or name
fn extract_meta_content(html: &str, property: &str) -> Option<String> {
    // Match <meta property="og:title" content="..."> or <meta name="description" content="...">
    // Handle various quote styles and attribute orders
    let patterns = [
        format!(r#"<meta[^>]*property=["']{property}["'][^>]*content=["']([^"']+)["']"#),
        format!(r#"<meta[^>]*content=["']([^"']+)["'][^>]*property=["']{property}["']"#),
        format!(r#"<meta[^>]*name=["']{property}["'][^>]*content=["']([^"']+)["']"#),
        format!(r#"<meta[^>]*content=["']([^"']+)["'][^>]*name=["']{property}["']"#),
    ];

    for pattern in &patterns {
        if let Ok(re) = Regex::new(pattern) {
            if let Some(caps) = re.captures(html) {
                if let Some(content) = caps.get(1) {
                    let value = html_decode(content.as_str().trim());
                    if !value.is_empty() {
                        return Some(value);
                    }
                }
            }
        }
    }

    None
}

/// Extract HTML <title> tag
fn extract_html_title(html: &str) -> Option<String> {
    let re = Regex::new(r"<title[^>]*>([^<]+)</title>").ok()?;
    let caps = re.captures(html)?;
    let title = html_decode(caps.get(1)?.as_str().trim());
    if title.is_empty() {
        None
    } else {
        Some(title)
    }
}

/// Basic HTML entity decoding
fn html_decode(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&#x27;", "'")
        .replace("&nbsp;", " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_urls() {
        let text = "Check out https://example.com and http://test.org/path?query=1";
        let urls = extract_urls(text);
        assert_eq!(urls.len(), 2);
        assert_eq!(urls[0], "https://example.com");
        assert_eq!(urls[1], "http://test.org/path?query=1");
    }

    #[test]
    fn test_extract_urls_with_punctuation() {
        let text = "Visit https://example.com. Also see https://test.org!";
        let urls = extract_urls(text);
        assert_eq!(urls.len(), 2);
        assert_eq!(urls[0], "https://example.com");
        assert_eq!(urls[1], "https://test.org");
    }

    #[test]
    fn test_extract_meta_content() {
        let html = r#"<meta property="og:title" content="Test Title">"#;
        assert_eq!(
            extract_meta_content(html, "og:title"),
            Some("Test Title".to_string())
        );

        let html2 = r#"<meta content="Test Desc" name="description">"#;
        assert_eq!(
            extract_meta_content(html2, "description"),
            Some("Test Desc".to_string())
        );
    }

    #[test]
    fn test_extract_html_title() {
        let html = r#"<html><head><title>Page Title</title></head></html>"#;
        assert_eq!(extract_html_title(html), Some("Page Title".to_string()));
    }
}
