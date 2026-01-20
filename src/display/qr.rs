//! QR code rendering for terminal display.

use anyhow::{Context, Result};
use crossterm::style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor};
use crossterm::{cursor, execute, terminal};
use qrcode::QrCode;
use std::io::{stdout, Write};

/// Render a QR code to the terminal using Unicode block characters.
///
/// Uses the upper half block character (▀) to display two rows of QR modules
/// in a single terminal row, making the QR code more compact.
pub fn render_qr_code(data: &str) -> Result<()> {
    let code = QrCode::new(data.as_bytes()).context("Failed to generate QR code")?;
    let modules = code.to_colors();
    let size = code.width();

    let mut stdout = stdout();

    // Clear screen and move to top
    execute!(
        stdout,
        terminal::Clear(terminal::ClearType::All),
        cursor::MoveTo(0, 0)
    )?;

    // Print header
    println!("\n  Scan this QR code with WhatsApp on your phone:\n");
    println!("  1. Open WhatsApp on your phone");
    println!("  2. Tap Menu (⋮) or Settings (⚙)");
    println!("  3. Tap 'Linked Devices'");
    println!("  4. Tap 'Link a Device'");
    println!("  5. Point your phone at this screen\n");

    // Calculate padding for centering
    let term_width = terminal::size().map(|(w, _)| w as usize).unwrap_or(80);
    let qr_width = size + 4; // QR + 2 quiet zone on each side
    let padding = if term_width > qr_width {
        (term_width - qr_width) / 2
    } else {
        2
    };
    let pad_str: String = " ".repeat(padding);

    // Add quiet zone (white border) at top
    print!("{}", pad_str);
    execute!(
        stdout,
        SetBackgroundColor(Color::White),
        SetForegroundColor(Color::White)
    )?;
    for _ in 0..(size + 4) {
        print!("  ");
    }
    execute!(stdout, ResetColor)?;
    println!();

    // Render QR code two rows at a time using half-block characters
    // ▀ (upper half block) - foreground color for top, background for bottom
    let get_module = |x: usize, y: usize| -> bool {
        if x < size && y < size {
            modules[y * size + x] == qrcode::Color::Dark
        } else {
            false // quiet zone is white (light)
        }
    };

    // Process rows in pairs
    for y in (0..size + 2).step_by(2) {
        print!("{}", pad_str);

        // Quiet zone left
        execute!(
            stdout,
            SetBackgroundColor(Color::White),
            SetForegroundColor(Color::White),
            Print("  ")
        )?;

        for x in 0..size {
            // Adjust for quiet zone offset
            let actual_y_top = y.checked_sub(1);
            let actual_y_bottom = y;

            let top_dark = actual_y_top.map_or(false, |yt| yt < size && get_module(x, yt));
            let bottom_dark = actual_y_bottom < size && get_module(x, actual_y_bottom);

            // Use half-block character to represent two vertical modules
            // Upper half block (▀): foreground = top, background = bottom
            match (top_dark, bottom_dark) {
                (true, true) => {
                    // Both black
                    execute!(
                        stdout,
                        SetForegroundColor(Color::Black),
                        SetBackgroundColor(Color::Black),
                        Print("██")
                    )?;
                }
                (true, false) => {
                    // Top black, bottom white
                    execute!(
                        stdout,
                        SetForegroundColor(Color::Black),
                        SetBackgroundColor(Color::White),
                        Print("▀▀")
                    )?;
                }
                (false, true) => {
                    // Top white, bottom black
                    execute!(
                        stdout,
                        SetForegroundColor(Color::Black),
                        SetBackgroundColor(Color::White),
                        Print("▄▄")
                    )?;
                }
                (false, false) => {
                    // Both white
                    execute!(
                        stdout,
                        SetForegroundColor(Color::White),
                        SetBackgroundColor(Color::White),
                        Print("  ")
                    )?;
                }
            }
        }

        // Quiet zone right
        execute!(
            stdout,
            SetBackgroundColor(Color::White),
            SetForegroundColor(Color::White),
            Print("  ")
        )?;

        execute!(stdout, ResetColor)?;
        println!();
    }

    // Add quiet zone at bottom
    print!("{}", pad_str);
    execute!(
        stdout,
        SetBackgroundColor(Color::White),
        SetForegroundColor(Color::White)
    )?;
    for _ in 0..(size + 4) {
        print!("  ");
    }
    execute!(stdout, ResetColor)?;
    println!("\n");

    stdout.flush()?;
    Ok(())
}

/// Clear the QR code display
pub fn clear_qr_display() -> Result<()> {
    let mut stdout = stdout();
    execute!(
        stdout,
        terminal::Clear(terminal::ClearType::All),
        cursor::MoveTo(0, 0)
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qr_generation() {
        // Just ensure QR code can be generated without panicking
        let code = QrCode::new(b"test data").unwrap();
        assert!(code.width() > 0);
    }
}
