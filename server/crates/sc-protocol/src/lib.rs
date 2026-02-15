//! # sc-protocol
//!
//! Shared protocol types for the ScreenControl platform.
//! Auto-generated from Protocol Buffers definitions.

/// Auto-generated protobuf types.
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/screencontrol.rs"));
}

pub use proto::*;

/// Protocol version constant — bump on breaking wire changes.
pub const PROTOCOL_VERSION: u32 = 1;

/// Default heartbeat interval in seconds.
pub const DEFAULT_HEARTBEAT_INTERVAL_SECS: u32 = 30;

/// Maximum message size (10 MB).
pub const MAX_MESSAGE_SIZE: usize = 10 * 1024 * 1024;
