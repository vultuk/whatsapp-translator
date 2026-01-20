//! Message display formatting for terminal output.

use crate::bridge::{Chat, Message, MessageContent};
use crossterm::execute;
use crossterm::style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor};
use std::io::{stdout, Write};

/// Color scheme for different message elements
pub struct ColorScheme {
    pub timestamp: Color,
    pub sender_private: Color,
    pub sender_group: Color,
    pub group_name: Color,
    pub message_type: Color,
    pub message_body: Color,
    pub media_info: Color,
    pub separator: Color,
    pub from_me: Color,
}

impl Default for ColorScheme {
    fn default() -> Self {
        Self {
            timestamp: Color::DarkGrey,
            sender_private: Color::Cyan,
            sender_group: Color::Green,
            group_name: Color::Magenta,
            message_type: Color::Yellow,
            message_body: Color::White,
            media_info: Color::DarkGrey,
            separator: Color::DarkGrey,
            from_me: Color::Blue,
        }
    }
}

/// Formats and displays a message to the terminal
pub struct MessageDisplay {
    colors: ColorScheme,
    show_separator: bool,
}

impl MessageDisplay {
    pub fn new() -> Self {
        Self {
            colors: ColorScheme::default(),
            show_separator: true,
        }
    }

    /// Display a message to stdout
    pub fn display(&self, msg: &Message) -> std::io::Result<()> {
        let mut stdout = stdout();

        if self.show_separator {
            self.print_separator(&mut stdout)?;
        }

        // Timestamp and chat type
        self.print_header(&mut stdout, msg)?;

        // Sender info
        self.print_sender(&mut stdout, msg)?;

        // Message type
        self.print_message_type(&mut stdout, msg)?;

        // Message content
        self.print_content(&mut stdout, msg)?;

        println!();
        stdout.flush()?;
        Ok(())
    }

    /// Display a message with translation
    pub fn display_with_translation(
        &self,
        msg: &Message,
        translated_text: &str,
        source_language: &str,
    ) -> std::io::Result<()> {
        let mut stdout = stdout();

        if self.show_separator {
            self.print_separator(&mut stdout)?;
        }

        // Timestamp and chat type
        self.print_header(&mut stdout, msg)?;

        // Sender info
        self.print_sender(&mut stdout, msg)?;

        // Message type with translation indicator
        execute!(
            stdout,
            Print("Type: "),
            SetForegroundColor(self.colors.message_type),
            Print(msg.content.type_name()),
            ResetColor,
            SetForegroundColor(Color::Magenta),
            Print(format!(" [Translated from {}]", source_language)),
            ResetColor
        )?;
        println!();

        // Translated content
        println!();
        execute!(
            stdout,
            SetForegroundColor(self.colors.message_body),
            Print(translated_text),
            ResetColor
        )?;

        // Original text in dimmed format
        if let MessageContent::Text { body } = &msg.content {
            println!();
            println!();
            execute!(
                stdout,
                SetForegroundColor(Color::DarkGrey),
                SetAttribute(Attribute::Italic),
                Print("Original: "),
                Print(body),
                SetAttribute(Attribute::Reset),
                ResetColor
            )?;
        }

        println!();
        stdout.flush()?;
        Ok(())
    }

    fn print_separator(&self, stdout: &mut std::io::Stdout) -> std::io::Result<()> {
        execute!(
            stdout,
            SetForegroundColor(self.colors.separator),
            Print("━".repeat(70)),
            ResetColor
        )?;
        println!();
        Ok(())
    }

    fn print_header(&self, stdout: &mut std::io::Stdout, msg: &Message) -> std::io::Result<()> {
        let timestamp = msg.timestamp.format("%Y-%m-%d %H:%M:%S");
        let chat_type = match &msg.chat {
            Chat::Private { .. } => "Private Chat",
            Chat::Group { .. } => "Group Chat",
            Chat::Broadcast { .. } => "Broadcast",
            Chat::Status { .. } => "Status",
        };

        execute!(
            stdout,
            SetForegroundColor(self.colors.timestamp),
            Print(format!("[{}] ", timestamp)),
            ResetColor
        )?;

        let chat_color = if msg.chat.is_group() {
            self.colors.group_name
        } else {
            self.colors.sender_private
        };

        execute!(
            stdout,
            SetForegroundColor(chat_color),
            Print(chat_type),
            ResetColor
        )?;

        // If group, show group name
        if let Chat::Group { name, .. } = &msg.chat {
            if let Some(group_name) = name {
                execute!(
                    stdout,
                    SetForegroundColor(self.colors.group_name),
                    Print(format!(": {}", group_name)),
                    ResetColor
                )?;
            }
        }

        println!();
        Ok(())
    }

    fn print_sender(&self, stdout: &mut std::io::Stdout, msg: &Message) -> std::io::Result<()> {
        let sender_color = if msg.is_from_me {
            self.colors.from_me
        } else if msg.chat.is_group() {
            self.colors.sender_group
        } else {
            self.colors.sender_private
        };

        let sender_label = if msg.is_from_me { "To" } else { "From" };
        let display_name = msg.push_name.as_deref().unwrap_or(msg.from.display_name());

        execute!(
            stdout,
            Print(format!("{}: ", sender_label)),
            SetForegroundColor(sender_color),
            SetAttribute(Attribute::Bold),
            Print(display_name),
            SetAttribute(Attribute::Reset),
            ResetColor
        )?;

        // Show phone number if different from display name
        if msg.from.name.is_some() || msg.push_name.is_some() {
            execute!(
                stdout,
                SetForegroundColor(self.colors.media_info),
                Print(format!(" (+{})", msg.from.phone)),
                ResetColor
            )?;
        }

        if msg.is_forwarded {
            execute!(
                stdout,
                SetForegroundColor(self.colors.media_info),
                Print(" [Forwarded]"),
                ResetColor
            )?;
        }

        println!();
        Ok(())
    }

    fn print_message_type(
        &self,
        stdout: &mut std::io::Stdout,
        msg: &Message,
    ) -> std::io::Result<()> {
        execute!(
            stdout,
            Print("Type: "),
            SetForegroundColor(self.colors.message_type),
            Print(msg.content.type_name()),
            ResetColor
        )?;
        println!();
        Ok(())
    }

    fn print_content(&self, stdout: &mut std::io::Stdout, msg: &Message) -> std::io::Result<()> {
        println!();

        match &msg.content {
            MessageContent::Text { body } => {
                execute!(
                    stdout,
                    SetForegroundColor(self.colors.message_body),
                    Print(body),
                    ResetColor
                )?;
            }

            MessageContent::Image {
                caption,
                mime_type,
                file_size,
                ..
            } => {
                self.print_media_info(stdout, "Image", mime_type, *file_size)?;
                if let Some(cap) = caption {
                    println!();
                    execute!(
                        stdout,
                        Print("Caption: "),
                        SetForegroundColor(self.colors.message_body),
                        Print(cap),
                        ResetColor
                    )?;
                }
            }

            MessageContent::Video {
                caption,
                mime_type,
                file_size,
                duration_seconds,
                ..
            } => {
                self.print_media_info(stdout, "Video", mime_type, *file_size)?;
                if let Some(duration) = duration_seconds {
                    execute!(
                        stdout,
                        SetForegroundColor(self.colors.media_info),
                        Print(format!(" ({})", format_duration(*duration))),
                        ResetColor
                    )?;
                }
                if let Some(cap) = caption {
                    println!();
                    execute!(
                        stdout,
                        Print("Caption: "),
                        SetForegroundColor(self.colors.message_body),
                        Print(cap),
                        ResetColor
                    )?;
                }
            }

            MessageContent::Audio {
                mime_type,
                file_size,
                duration_seconds,
                is_voice_note,
                ..
            } => {
                let label = if *is_voice_note {
                    "Voice Note"
                } else {
                    "Audio"
                };
                self.print_media_info(stdout, label, mime_type, *file_size)?;
                if let Some(duration) = duration_seconds {
                    execute!(
                        stdout,
                        SetForegroundColor(self.colors.media_info),
                        Print(format!(" ({})", format_duration(*duration))),
                        ResetColor
                    )?;
                }
            }

            MessageContent::Document {
                caption,
                mime_type,
                file_name,
                file_size,
                ..
            } => {
                let name = file_name.as_deref().unwrap_or("document");
                execute!(
                    stdout,
                    SetForegroundColor(self.colors.media_info),
                    Print(format!(
                        "[Document: {} - {} - {}]",
                        name,
                        mime_type,
                        format_file_size(*file_size)
                    )),
                    ResetColor
                )?;
                if let Some(cap) = caption {
                    println!();
                    execute!(
                        stdout,
                        Print("Caption: "),
                        SetForegroundColor(self.colors.message_body),
                        Print(cap),
                        ResetColor
                    )?;
                }
            }

            MessageContent::Sticker { is_animated, .. } => {
                let sticker_type = if *is_animated {
                    "Animated Sticker"
                } else {
                    "Sticker"
                };
                execute!(
                    stdout,
                    SetForegroundColor(self.colors.media_info),
                    Print(format!("[{}]", sticker_type)),
                    ResetColor
                )?;
            }

            MessageContent::Location {
                latitude,
                longitude,
                name,
                address,
            } => {
                execute!(
                    stdout,
                    SetForegroundColor(self.colors.media_info),
                    Print(format!("[Location: {:.6}, {:.6}]", latitude, longitude)),
                    ResetColor
                )?;
                if let Some(loc_name) = name {
                    println!();
                    execute!(
                        stdout,
                        SetForegroundColor(self.colors.message_body),
                        Print(loc_name),
                        ResetColor
                    )?;
                }
                if let Some(addr) = address {
                    println!();
                    execute!(
                        stdout,
                        SetForegroundColor(self.colors.media_info),
                        Print(addr),
                        ResetColor
                    )?;
                }
            }

            MessageContent::Contact { display_name, .. } => {
                execute!(
                    stdout,
                    SetForegroundColor(self.colors.media_info),
                    Print(format!("[Contact: {}]", display_name)),
                    ResetColor
                )?;
            }

            MessageContent::Reaction {
                emoji,
                target_message_id,
            } => {
                execute!(
                    stdout,
                    Print("Reacted with "),
                    SetForegroundColor(self.colors.message_body),
                    Print(emoji),
                    ResetColor,
                    SetForegroundColor(self.colors.media_info),
                    Print(format!(
                        " to message {}",
                        &target_message_id[..8.min(target_message_id.len())]
                    )),
                    ResetColor
                )?;
            }

            MessageContent::Revoked => {
                execute!(
                    stdout,
                    SetForegroundColor(self.colors.media_info),
                    SetAttribute(Attribute::Italic),
                    Print("[This message was deleted]"),
                    SetAttribute(Attribute::Reset),
                    ResetColor
                )?;
            }

            MessageContent::Poll { question, options } => {
                execute!(
                    stdout,
                    SetForegroundColor(self.colors.message_body),
                    SetAttribute(Attribute::Bold),
                    Print(format!("Poll: {}", question)),
                    SetAttribute(Attribute::Reset),
                    ResetColor
                )?;
                for (i, option) in options.iter().enumerate() {
                    println!();
                    execute!(
                        stdout,
                        SetForegroundColor(self.colors.media_info),
                        Print(format!("  {}. {}", i + 1, option)),
                        ResetColor
                    )?;
                }
            }

            MessageContent::Unknown { raw_type } => {
                execute!(
                    stdout,
                    SetForegroundColor(self.colors.media_info),
                    Print(format!("[Unsupported message type: {}]", raw_type)),
                    ResetColor
                )?;
            }
        }

        println!();
        Ok(())
    }

    fn print_media_info(
        &self,
        stdout: &mut std::io::Stdout,
        label: &str,
        mime_type: &str,
        file_size: u64,
    ) -> std::io::Result<()> {
        execute!(
            stdout,
            SetForegroundColor(self.colors.media_info),
            Print(format!(
                "[{}: {} - {}]",
                label,
                mime_type,
                format_file_size(file_size)
            )),
            ResetColor
        )?;
        Ok(())
    }
}

impl Default for MessageDisplay {
    fn default() -> Self {
        Self::new()
    }
}

/// Format file size in human-readable format
fn format_file_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Format duration in mm:ss or hh:mm:ss format
fn format_duration(seconds: u32) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;

    if hours > 0 {
        format!("{:02}:{:02}:{:02}", hours, minutes, secs)
    } else {
        format!("{:02}:{:02}", minutes, secs)
    }
}

/// Print a connection status message
pub fn print_connected(phone: &str, name: &str) {
    let mut stdout = stdout();
    let _ = execute!(
        stdout,
        SetForegroundColor(Color::Green),
        SetAttribute(Attribute::Bold),
        Print("✓ Connected to WhatsApp"),
        SetAttribute(Attribute::Reset),
        ResetColor
    );
    println!();
    println!("  Phone: {}", phone);
    println!("  Name: {}", name);
    println!();
    println!("Waiting for messages...");
    println!();
}

/// Print an error message
pub fn print_error(message: &str) {
    let mut stdout = stdout();
    let _ = execute!(
        stdout,
        SetForegroundColor(Color::Red),
        SetAttribute(Attribute::Bold),
        Print("✗ Error: "),
        SetAttribute(Attribute::Reset),
        SetForegroundColor(Color::Red),
        Print(message),
        ResetColor
    );
    println!();
}

/// Print a warning message
pub fn print_warning(message: &str) {
    let mut stdout = stdout();
    let _ = execute!(
        stdout,
        SetForegroundColor(Color::Yellow),
        Print("⚠ "),
        Print(message),
        ResetColor
    );
    println!();
}

/// Print an info message
pub fn print_info(message: &str) {
    let mut stdout = stdout();
    let _ = execute!(
        stdout,
        SetForegroundColor(Color::Cyan),
        Print("ℹ "),
        Print(message),
        ResetColor
    );
    println!();
}
