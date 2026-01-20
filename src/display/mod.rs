//! Display module for terminal output formatting.

pub mod message;
pub mod qr;

pub use message::{print_connected, print_error, print_info, print_warning, MessageDisplay};
pub use qr::{clear_qr_display, render_qr_code};
