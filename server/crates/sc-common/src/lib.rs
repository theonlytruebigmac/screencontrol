//! # sc-common
//!
//! Shared utilities, configuration, and error types for ScreenControl.

pub mod config;
pub mod error;

pub use config::AppConfig;
pub use error::{AppError, AppResult};
