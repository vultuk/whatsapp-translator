//! OAuth 2.0 Authorization Server implementation with PKCE support.
//!
//! Implements RFC 6749 (OAuth 2.0), RFC 7636 (PKCE), and RFC 8414 (Metadata).
//! Used to secure the MCP endpoint for external LLM clients.
//! All tokens and codes are persisted in SQLite for durability across restarts.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// OAuth 2.0 Authorization Server Metadata (RFC 8414)
#[derive(Debug, Clone, Serialize)]
pub struct OAuthMetadata {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub revocation_endpoint: Option<String>,
    pub response_types_supported: Vec<String>,
    pub grant_types_supported: Vec<String>,
    pub code_challenge_methods_supported: Vec<String>,
    pub token_endpoint_auth_methods_supported: Vec<String>,
    pub scopes_supported: Vec<String>,
}

impl OAuthMetadata {
    pub fn new(base_url: &str) -> Self {
        Self {
            issuer: base_url.to_string(),
            authorization_endpoint: format!("{}/oauth/authorize", base_url),
            token_endpoint: format!("{}/oauth/token", base_url),
            revocation_endpoint: Some(format!("{}/oauth/revoke", base_url)),
            response_types_supported: vec!["code".to_string()],
            grant_types_supported: vec![
                "authorization_code".to_string(),
                "refresh_token".to_string(),
            ],
            code_challenge_methods_supported: vec!["S256".to_string()],
            token_endpoint_auth_methods_supported: vec!["none".to_string()], // Public clients
            scopes_supported: vec!["mcp".to_string()],
        }
    }
}

/// Authorization code stored in the database
#[derive(Debug, Clone)]
pub struct AuthorizationCode {
    pub code: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub code_challenge: String,
    pub code_challenge_method: String,
    pub scope: String,
    pub created_at: i64, // Unix timestamp
    pub expires_at: i64, // Unix timestamp
    pub used: bool,
}

impl AuthorizationCode {
    /// Check if the code has expired
    pub fn is_expired(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        now > self.expires_at
    }

    /// Verify the code_verifier against the stored code_challenge (PKCE)
    pub fn verify_pkce(&self, code_verifier: &str) -> bool {
        if self.code_challenge_method != "S256" {
            return false;
        }

        // SHA256(code_verifier) -> base64url encode -> compare to code_challenge
        let mut hasher = Sha256::new();
        hasher.update(code_verifier.as_bytes());
        let hash = hasher.finalize();
        let computed_challenge = URL_SAFE_NO_PAD.encode(hash);

        computed_challenge == self.code_challenge
    }
}

/// Access token stored in the database
#[derive(Debug, Clone)]
pub struct AccessToken {
    pub token: String,
    pub client_id: String,
    pub scope: String,
    pub created_at: i64,
    pub expires_at: i64,
}

impl AccessToken {
    pub fn is_expired(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        now > self.expires_at
    }
}

/// Refresh token stored in the database
#[derive(Debug, Clone)]
pub struct RefreshToken {
    pub token: String,
    pub client_id: String,
    pub scope: String,
    pub created_at: i64,
    pub expires_at: i64,
}

impl RefreshToken {
    pub fn is_expired(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        now > self.expires_at
    }
}

/// Pending authorization request (before user approves)
#[derive(Debug, Clone)]
pub struct PendingAuthorization {
    pub session_key: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub code_challenge: String,
    pub code_challenge_method: String,
    pub scope: String,
    pub state: Option<String>,
    pub created_at: i64,
    pub expires_at: i64,
}

impl PendingAuthorization {
    pub fn is_expired(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        now > self.expires_at
    }
}

/// Generate a cryptographically secure random token
pub fn generate_token() -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
        .hash(&mut hasher);
    uuid::Uuid::new_v4().hash(&mut hasher);

    // Use SHA256 for better randomness
    let mut sha = Sha256::new();
    sha.update(hasher.finish().to_le_bytes());
    sha.update(uuid::Uuid::new_v4().as_bytes());
    let hash = sha.finalize();
    URL_SAFE_NO_PAD.encode(&hash[..])
}

/// OAuth error types (RFC 6749 Section 4.1.2.1)
#[derive(Debug, Clone, Serialize)]
pub enum OAuthError {
    InvalidRequest,
    InvalidClient,
    InvalidGrant,
    UnauthorizedClient,
    UnsupportedGrantType,
    InvalidScope,
    AccessDenied,
    ServerError,
}

impl OAuthError {
    pub fn as_str(&self) -> &'static str {
        match self {
            OAuthError::InvalidRequest => "invalid_request",
            OAuthError::InvalidClient => "invalid_client",
            OAuthError::InvalidGrant => "invalid_grant",
            OAuthError::UnauthorizedClient => "unauthorized_client",
            OAuthError::UnsupportedGrantType => "unsupported_grant_type",
            OAuthError::InvalidScope => "invalid_scope",
            OAuthError::AccessDenied => "access_denied",
            OAuthError::ServerError => "server_error",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            OAuthError::InvalidRequest => "The request is missing a required parameter",
            OAuthError::InvalidClient => "Client authentication failed",
            OAuthError::InvalidGrant => {
                "The authorization code or refresh token is invalid or expired"
            }
            OAuthError::UnauthorizedClient => "The client is not authorized",
            OAuthError::UnsupportedGrantType => "The grant type is not supported",
            OAuthError::InvalidScope => "The requested scope is invalid",
            OAuthError::AccessDenied => "The resource owner denied the request",
            OAuthError::ServerError => "An unexpected error occurred",
        }
    }
}

/// OAuth error response body
#[derive(Debug, Serialize)]
pub struct OAuthErrorResponse {
    pub error: String,
    pub error_description: String,
}

impl From<OAuthError> for OAuthErrorResponse {
    fn from(err: OAuthError) -> Self {
        Self {
            error: err.as_str().to_string(),
            error_description: err.description().to_string(),
        }
    }
}

/// Token response from /oauth/token
#[derive(Debug, Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub refresh_token: String,
    pub scope: String,
}

/// Authorization request parameters
#[derive(Debug, Deserialize)]
pub struct AuthorizeRequest {
    pub response_type: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub scope: Option<String>,
    pub state: Option<String>,
    pub code_challenge: String,
    pub code_challenge_method: String,
}

/// Token request parameters (form encoded)
#[derive(Debug, Deserialize)]
pub struct TokenRequest {
    pub grant_type: String,
    pub code: Option<String>,
    pub redirect_uri: Option<String>,
    pub code_verifier: Option<String>,
    pub refresh_token: Option<String>,
    pub client_id: Option<String>,
}

/// Token revocation request
#[derive(Debug, Deserialize)]
pub struct RevokeRequest {
    pub token: String,
    pub token_type_hint: Option<String>,
}

/// Approval form submission
#[derive(Debug, Deserialize)]
pub struct ApprovalRequest {
    pub session_key: String,
    pub approved: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pkce_verification() {
        // Test vector from RFC 7636
        let code_verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";

        // SHA256(code_verifier) = base64url encoded
        let mut hasher = Sha256::new();
        hasher.update(code_verifier.as_bytes());
        let hash = hasher.finalize();
        let code_challenge = URL_SAFE_NO_PAD.encode(hash);

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let auth_code = AuthorizationCode {
            code: "test".to_string(),
            client_id: "test".to_string(),
            redirect_uri: "http://localhost".to_string(),
            code_challenge: code_challenge.clone(),
            code_challenge_method: "S256".to_string(),
            scope: "mcp".to_string(),
            created_at: now,
            expires_at: now + 300,
            used: false,
        };

        assert!(auth_code.verify_pkce(code_verifier));
        assert!(!auth_code.verify_pkce("wrong_verifier"));
    }
}
