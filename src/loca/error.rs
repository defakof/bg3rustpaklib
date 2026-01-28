//! Error types for localization file operations.

use std::io;
use thiserror::Error;

/// Result type alias for localization operations.
pub type Result<T> = std::result::Result<T, LocaError>;

/// Errors that can occur when working with localization files.
#[derive(Error, Debug)]
pub enum LocaError {
    /// The file has an invalid or missing signature.
    #[error("invalid loca signature: expected 0x{expected:08X}, got 0x{actual:08X}")]
    InvalidSignature {
        /// The expected signature value.
        expected: u32,
        /// The actual signature found.
        actual: u32,
    },

    /// The file format is invalid or corrupted.
    #[error("invalid loca format: {0}")]
    InvalidFormat(String),

    /// The key string is too long (max 64 bytes).
    #[error("key too long: {key} ({length} bytes, max 64)")]
    KeyTooLong {
        /// The key that was too long.
        key: String,
        /// The actual length in bytes.
        length: usize,
    },

    /// An I/O error occurred.
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    /// XML parsing or writing error.
    #[error("XML error: {0}")]
    Xml(String),

    /// UTF-8 decoding error.
    #[error("UTF-8 error: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),

    /// UTF-8 decoding error from str.
    #[error("UTF-8 error: {0}")]
    Utf8Str(#[from] std::str::Utf8Error),
}

impl LocaError {
    /// Creates a new invalid format error.
    pub fn invalid_format(msg: impl Into<String>) -> Self {
        LocaError::InvalidFormat(msg.into())
    }

    /// Creates a new XML error.
    pub fn xml(msg: impl Into<String>) -> Self {
        LocaError::Xml(msg.into())
    }
}
