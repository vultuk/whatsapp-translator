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

    /// OpenAI API key for translation and AI features
    #[arg(long, env = "OPENAI_API_KEY")]
    pub openai_api_key: Option<String>,

    /// OpenAI model for language detection and style analysis
    #[arg(
        long,
        env = "WA_OPENAI_DETECTION_MODEL",
        default_value = "gpt-5.4-nano"
    )]
    pub openai_detection_model: String,

    /// OpenAI model for translation
    #[arg(
        long,
        env = "WA_OPENAI_TRANSLATION_MODEL",
        default_value = "gpt-5.4-mini"
    )]
    pub openai_translation_model: String,

    /// OpenAI model for AI compose and styled replies
    #[arg(long, env = "WA_OPENAI_HIGH_END_MODEL", default_value = "gpt-5.4")]
    pub openai_high_end_model: String,

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
        let mut args = Self::parse();

        if std::env::var_os("WA_PORT").is_none() {
            if let Ok(port) = std::env::var("PORT") {
                if let Ok(parsed_port) = port.parse::<u16>() {
                    args.port = parsed_port;
                }
            }
        }

        args
    }

    /// Check if translation is enabled
    pub fn translation_enabled(&self) -> bool {
        self.openai_api_key.is_some()
    }
}
