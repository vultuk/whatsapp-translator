//! Link preview fetching - extracts Open Graph metadata from URLs.

use anyhow::{Context, Result};
use futures::StreamExt;
use regex::Regex;
use reqwest::{header, redirect::Policy, Client, Url};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::time::Duration;
use tracing::debug;

/// Maximum response size to fetch (1MB)
const MAX_RESPONSE_SIZE: usize = 1024 * 1024;

/// Maximum redirects to follow while revalidating every target.
const MAX_REDIRECTS: usize = 3;

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

/// Validate a URL is safe for server-side link preview fetching.
pub async fn validate_link_preview_url(url: &str) -> Result<()> {
    let parsed_url = Url::parse(url).context("Invalid URL")?;
    validate_url_target(&parsed_url).await
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
    let original_url = url.to_string();
    let mut current_url = Url::parse(url).context("Invalid URL")?;
    validate_url_target(&current_url).await?;

    let client = Client::builder()
        .timeout(FETCH_TIMEOUT)
        .redirect(Policy::none())
        .user_agent("Mozilla/5.0 (compatible; WhatsAppTranslator/1.0; +https://github.com/vultuk/whatsapp-translator)")
        .build()
        .context("Failed to create HTTP client")?;

    debug!("Fetching link preview for: {}", current_url);

    let mut redirects = 0;
    let response = loop {
        validate_url_target(&current_url).await?;

        let response = client
            .get(current_url.clone())
            .send()
            .await
            .context("Failed to fetch URL")?;

        if !response.status().is_redirection() {
            break response;
        }

        let location = response
            .headers()
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .context("Redirect response missing Location header")?;
        let next_url = current_url
            .join(location)
            .context("Redirect Location is not a valid URL")?;

        if next_url == current_url {
            return Ok(LinkPreview::error(
                original_url,
                "Redirect loop detected".to_string(),
            ));
        }

        if redirects >= MAX_REDIRECTS {
            return Ok(LinkPreview::error(
                original_url,
                "Too many redirects".to_string(),
            ));
        }

        redirects += 1;
        current_url = next_url;
    };

    // Check status
    if !response.status().is_success() {
        return Ok(LinkPreview::error(
            original_url,
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
            original_url,
            "Not an HTML page".to_string(),
        ));
    }

    if response
        .content_length()
        .is_some_and(|len| len > MAX_RESPONSE_SIZE as u64)
    {
        return Ok(LinkPreview::error(
            original_url,
            "Response too large".to_string(),
        ));
    }

    // Stream body with a hard size cap instead of buffering unbounded responses.
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("Failed to read response body")?;
        if body.len() + chunk.len() > MAX_RESPONSE_SIZE {
            return Ok(LinkPreview::error(
                original_url,
                "Response too large".to_string(),
            ));
        }
        body.extend_from_slice(&chunk);
    }

    // Parse HTML and extract metadata
    let html = String::from_utf8_lossy(&body);
    let mut preview = parse_html_metadata(current_url.as_str(), &html);
    preview.url = original_url.clone();

    debug!(
        "Link preview for {}: title={:?}, image={:?}",
        original_url, preview.title, preview.image_url
    );

    Ok(preview)
}

async fn validate_url_target(url: &Url) -> Result<()> {
    match url.scheme() {
        "http" | "https" => {}
        _ => anyhow::bail!("Only http and https URLs are allowed"),
    }

    if !url.username().is_empty() || url.password().is_some() {
        anyhow::bail!("URLs with embedded credentials are not allowed");
    }

    let host = url.host_str().context("URL host is required")?;
    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_blocked_ip(ip) {
            anyhow::bail!("URL resolves to a blocked address");
        }
        return Ok(());
    }

    let port = url
        .port_or_known_default()
        .context("URL port could not be determined")?;
    let addresses = tokio::net::lookup_host((host, port))
        .await
        .with_context(|| format!("Failed to resolve URL host: {}", host))?
        .collect::<Vec<_>>();

    if addresses.is_empty() {
        anyhow::bail!("URL host did not resolve to any addresses");
    }
    if addresses.iter().any(|addr| is_blocked_ip(addr.ip())) {
        anyhow::bail!("URL resolves to a blocked address");
    }

    Ok(())
}

fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let octets = ip.octets();
            octets[0] == 0
                || octets[0] == 10
                || octets[0] == 127
                || (octets[0] == 100 && (64..=127).contains(&octets[1]))
                || (octets[0] == 169 && octets[1] == 254)
                || (octets[0] == 172 && (16..=31).contains(&octets[1]))
                || (octets[0] == 192 && octets[1] == 168)
                || (octets[0] == 192 && octets[1] == 0 && octets[2] == 0)
                || (octets[0] == 192 && octets[1] == 0 && octets[2] == 2)
                || (octets[0] == 198 && (octets[1] == 18 || octets[1] == 19))
                || (octets[0] == 198 && octets[1] == 51 && octets[2] == 100)
                || (octets[0] == 203 && octets[1] == 0 && octets[2] == 113)
                || (224..=255).contains(&octets[0])
        }
        IpAddr::V6(ip) => {
            let segments = ip.segments();
            ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_multicast()
                || ip
                    .to_ipv4_mapped()
                    .is_some_and(|mapped| is_blocked_ip(mapped.into()))
                || (segments[0] & 0xfe00) == 0xfc00
                || (segments[0] & 0xffc0) == 0xfe80
                || (segments[0] == 0x2001 && segments[1] == 0x0db8)
        }
    }
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

    #[test]
    fn blocks_private_and_special_ip_ranges() {
        assert!(is_blocked_ip("127.0.0.1".parse().unwrap()));
        assert!(is_blocked_ip("10.0.0.5".parse().unwrap()));
        assert!(is_blocked_ip("172.16.0.10".parse().unwrap()));
        assert!(is_blocked_ip("192.168.1.10".parse().unwrap()));
        assert!(is_blocked_ip("169.254.169.254".parse().unwrap()));
        assert!(is_blocked_ip("100.64.1.1".parse().unwrap()));
        assert!(is_blocked_ip("::1".parse().unwrap()));
        assert!(is_blocked_ip("fc00::1".parse().unwrap()));
        assert!(is_blocked_ip("fe80::1".parse().unwrap()));
        assert!(!is_blocked_ip("93.184.216.34".parse().unwrap()));
    }

    #[tokio::test]
    async fn validate_link_preview_url_rejects_unsafe_targets() {
        assert!(validate_link_preview_url("file:///etc/passwd")
            .await
            .is_err());
        assert!(validate_link_preview_url("http://user:pass@example.com")
            .await
            .is_err());
        assert!(validate_link_preview_url("http://127.0.0.1:8080")
            .await
            .is_err());
        assert!(validate_link_preview_url("http://[::1]/").await.is_err());
    }
}
