//! Bridge module for communicating with the Go wa-bridge subprocess.

pub mod process;
pub mod protocol;

pub use process::{default_data_dir, find_bridge_binary, BridgeConfig, BridgeProcess};
pub use protocol::{
    BridgeCommand, BridgeEvent, Chat, ChatPresenceState, ConnectionState, Contact, Message,
    MessageContent,
};
