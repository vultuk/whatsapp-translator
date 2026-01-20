//! Process management for the Go bridge subprocess.
//!
//! Handles spawning the wa-bridge binary and communicating via JSON-lines over stdio.

use std::path::PathBuf;
use std::process::Stdio;

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

use super::protocol::{BridgeCommand, BridgeEvent};

/// Manages the Go bridge subprocess
pub struct BridgeProcess {
    child: Child,
    /// Channel for sending commands to the bridge
    command_tx: mpsc::Sender<BridgeCommand>,
}

/// Configuration for the bridge process
#[derive(Debug, Clone)]
pub struct BridgeConfig {
    /// Path to the wa-bridge binary
    pub binary_path: PathBuf,
    /// Directory for storing session data
    pub data_dir: PathBuf,
    /// Enable verbose logging in the bridge
    pub verbose: bool,
}

impl BridgeProcess {
    /// Spawn the Go bridge process and start reading events
    pub async fn spawn(config: BridgeConfig, event_tx: mpsc::Sender<BridgeEvent>) -> Result<Self> {
        // Ensure data directory exists
        tokio::fs::create_dir_all(&config.data_dir)
            .await
            .context("Failed to create data directory")?;

        let mut cmd = Command::new(&config.binary_path);
        cmd.arg("--data-dir")
            .arg(&config.data_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if config.verbose {
            cmd.arg("--verbose");
        }

        let mut child = cmd.spawn().with_context(|| {
            format!(
                "Failed to spawn wa-bridge binary at {:?}",
                config.binary_path
            )
        })?;

        let stdout = child.stdout.take().context("Failed to capture stdout")?;
        let stderr = child.stderr.take().context("Failed to capture stderr")?;
        let stdin = child.stdin.take().context("Failed to capture stdin")?;

        // Channel for commands to send to the bridge
        let (command_tx, command_rx) = mpsc::channel::<BridgeCommand>(32);

        // Spawn task to read stdout (JSON events)
        let event_tx_clone = event_tx.clone();
        tokio::spawn(async move {
            Self::read_events(stdout, event_tx_clone).await;
        });

        // Spawn task to read stderr (logs)
        tokio::spawn(async move {
            Self::read_stderr(stderr, event_tx).await;
        });

        // Spawn task to write commands to stdin
        tokio::spawn(async move {
            Self::write_commands(stdin, command_rx).await;
        });

        Ok(Self { child, command_tx })
    }

    /// Read JSON-line events from stdout
    async fn read_events(stdout: tokio::process::ChildStdout, event_tx: mpsc::Sender<BridgeEvent>) {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();

        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() {
                continue;
            }

            match serde_json::from_str::<BridgeEvent>(&line) {
                Ok(event) => {
                    if event_tx.send(event).await.is_err() {
                        // Receiver dropped, exit
                        break;
                    }
                }
                Err(e) => {
                    // Log parse error but continue
                    let log_event = BridgeEvent::Log {
                        level: "warn".to_string(),
                        message: format!("Failed to parse bridge event: {} - line: {}", e, line),
                    };
                    let _ = event_tx.send(log_event).await;
                }
            }
        }
    }

    /// Read stderr and convert to log events
    async fn read_stderr(stderr: tokio::process::ChildStderr, event_tx: mpsc::Sender<BridgeEvent>) {
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();

        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() {
                continue;
            }

            let log_event = BridgeEvent::Log {
                level: "debug".to_string(),
                message: format!("[bridge] {}", line),
            };
            if event_tx.send(log_event).await.is_err() {
                break;
            }
        }
    }

    /// Write commands to stdin
    async fn write_commands(
        mut stdin: tokio::process::ChildStdin,
        mut command_rx: mpsc::Receiver<BridgeCommand>,
    ) {
        while let Some(cmd) = command_rx.recv().await {
            let json = match serde_json::to_string(&cmd) {
                Ok(j) => j,
                Err(e) => {
                    tracing::error!("Failed to serialize command: {}", e);
                    continue;
                }
            };

            if let Err(e) = stdin.write_all(json.as_bytes()).await {
                tracing::error!("Failed to write command to bridge: {}", e);
                break;
            }
            if let Err(e) = stdin.write_all(b"\n").await {
                tracing::error!("Failed to write newline to bridge: {}", e);
                break;
            }
            if let Err(e) = stdin.flush().await {
                tracing::error!("Failed to flush stdin: {}", e);
                break;
            }
        }
    }

    /// Send a command to the bridge
    pub async fn send_command(&self, cmd: BridgeCommand) -> Result<()> {
        self.command_tx
            .send(cmd)
            .await
            .context("Failed to send command to bridge")
    }

    /// Get a clone of the command sender for use elsewhere (e.g., web API)
    pub fn command_sender(&self) -> mpsc::Sender<BridgeCommand> {
        self.command_tx.clone()
    }

    /// Request graceful shutdown
    pub async fn shutdown(mut self) -> Result<()> {
        // Send disconnect command
        let _ = self.send_command(BridgeCommand::Disconnect).await;

        // Wait for process to exit (with timeout)
        tokio::select! {
            result = self.child.wait() => {
                result.context("Failed to wait for bridge process")?;
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {
                // Force kill if it doesn't exit gracefully
                self.child.kill().await.context("Failed to kill bridge process")?;
            }
        }

        Ok(())
    }

    /// Check if the process is still running
    pub fn try_wait(&mut self) -> Result<Option<std::process::ExitStatus>> {
        self.child
            .try_wait()
            .context("Failed to check bridge process status")
    }
}

/// Find the wa-bridge binary
pub fn find_bridge_binary() -> Result<PathBuf> {
    // First, check if it's in the same directory as the executable
    if let Ok(exe_path) = std::env::current_exe() {
        let dir = exe_path.parent().unwrap_or(std::path::Path::new("."));
        let bridge_path = dir.join("wa-bridge");
        if bridge_path.exists() {
            return Ok(bridge_path);
        }
    }

    // Check current directory
    let current_dir = std::path::Path::new("./wa-bridge");
    if current_dir.exists() {
        return Ok(current_dir.to_path_buf());
    }

    // Check target directory (for development)
    for profile in ["debug", "release"] {
        let dev_path = PathBuf::from(format!("target/{}/wa-bridge", profile));
        if dev_path.exists() {
            return Ok(dev_path);
        }
    }

    // Check if wa-bridge directory exists and has the binary
    let wa_bridge_bin = PathBuf::from("wa-bridge/wa-bridge");
    if wa_bridge_bin.exists() {
        return Ok(wa_bridge_bin);
    }

    anyhow::bail!(
        "Could not find wa-bridge binary. Please ensure it is built and located in \
         the same directory as the executable, or run `go build` in the wa-bridge directory."
    )
}

/// Get the default data directory for storing session and config
pub fn default_data_dir() -> Result<PathBuf> {
    let dir = dirs::data_dir()
        .or_else(dirs::home_dir)
        .context("Could not determine home directory")?
        .join("whatsapp-translator");

    Ok(dir)
}
