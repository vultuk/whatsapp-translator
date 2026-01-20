//! CLI argument parsing using clap.

use clap::Parser;
use std::path::PathBuf;

/// WhatsApp Translator - Connect to WhatsApp and display incoming messages
#[derive(Parser, Debug, Clone)]
#[command(name = "whatsapp-translator")]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// Enable verbose/debug logging
    #[arg(short, long, env = "WA_VERBOSE")]
    pub verbose: bool,

    /// Output messages as raw JSON (useful for piping to other tools)
    #[arg(long, env = "WA_JSON")]
    pub json: bool,

    /// Clear existing session and scan a new QR code
    #[arg(long, env = "WA_LOGOUT")]
    pub logout: bool,

    /// Custom data directory for session storage
    #[arg(long, value_name = "DIR", env = "WA_DATA_DIR")]
    pub data_dir: Option<PathBuf>,

    /// Path to wa-bridge binary (auto-detected by default)
    #[arg(long, value_name = "PATH", env = "WA_BRIDGE_PATH")]
    pub bridge_path: Option<PathBuf>,

    /// Start web server mode (serves web UI and API)
    #[arg(long, env = "WA_WEB")]
    pub web: bool,

    /// Port for web server (default: 3000)
    #[arg(long, default_value = "3000", env = "WA_PORT")]
    pub port: u16,

    /// Host address to bind web server to (default: 0.0.0.0)
    #[arg(long, default_value = "0.0.0.0", env = "WA_HOST")]
    pub host: String,

    /// Claude API key for message translation
    #[arg(long, env = "ANTHROPIC_API_KEY")]
    pub claude_api_key: Option<String>,

    /// Default language for messages (messages in this language won't be translated)
    #[arg(long, default_value = "English", env = "WA_DEFAULT_LANGUAGE")]
    pub default_language: String,

    /// Password to protect the web interface (if not set, no password required)
    #[arg(long, env = "WA_PASSWORD")]
    pub password: Option<String>,
}

impl Args {
    /// Parse command line arguments
    pub fn parse_args() -> Self {
        Self::parse()
    }

    /// Check if translation is enabled
    pub fn translation_enabled(&self) -> bool {
        self.claude_api_key.is_some()
    }
}
