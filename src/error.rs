//! Error types for the bg3rustpaklib crate.

use std::io;
use std::path::PathBuf;
use thiserror::Error;

/// Result type alias for PAK operations.
pub type Result<T> = std::result::Result<T, PakError>;

/// Errors that can occur when working with PAK files.
#[derive(Error, Debug)]
pub enum PakError {
    /// The file is not a valid PAK file (missing or invalid signature).
    #[error("not a valid PAK file: {0}")]
    NotAPakFile(String),

    /// The PAK file version is not supported.
    #[error("unsupported PAK version: {0}")]
    UnsupportedVersion(u32),

    /// The file is too small to be a valid PAK file.
    #[error("file too small to be a valid PAK file: {size} bytes")]
    FileTooSmall {
        /// The actual size of the file.
        size: u64,
    },

    /// An I/O error occurred.
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    /// Failed to decompress data.
    #[error("decompression error: {0}")]
    Decompression(String),

    /// The compressed data is corrupted or invalid.
    #[error("corrupted data: {0}")]
    CorruptedData(String),

    /// An unsupported compression method was encountered.
    #[error("unsupported compression method: {0}")]
    UnsupportedCompression(u8),

    /// A file was not found in the package.
    #[error("file not found in package: {0}")]
    FileNotFound(String),

    /// Failed to create output directory or file.
    #[error("failed to create output path: {path}")]
    OutputCreation {
        /// The path that could not be created.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: io::Error,
    },

    /// The archive part file is missing.
    #[error("missing archive part: {0}")]
    MissingArchivePart(PathBuf),

    /// Invalid file entry data.
    #[error("invalid file entry: {0}")]
    InvalidFileEntry(String),

    /// The file list is corrupted or cannot be read.
    #[error("corrupted file list: {0}")]
    CorruptedFileList(String),

    /// Attempted to read a deleted file.
    #[error("cannot read deleted file: {0}")]
    DeletedFile(String),

    /// The solid archive data is corrupted.
    #[error("corrupted solid archive: {0}")]
    CorruptedSolidArchive(String),
}

impl PakError {
    /// Creates a new decompression error with the given message.
    pub fn decompression(msg: impl Into<String>) -> Self {
        PakError::Decompression(msg.into())
    }

    /// Creates a new corrupted data error with the given message.
    pub fn corrupted(msg: impl Into<String>) -> Self {
        PakError::CorruptedData(msg.into())
    }
}
