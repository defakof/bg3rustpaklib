//! # bg3rustpaklib
//!
//! A Rust library for reading and extracting Baldur's Gate 3 PAK files.
//!
//! This library provides functionality similar to [lslib](https://github.com/Norbyte/lslib)
//! for working with Larian Studios' PAK (LSPK) archive format.
//!
//! ## Features
//!
//! - Read PAK files (versions 15, 16, 18 for BG3)
//! - List and search files within packages
//! - Extract individual files or entire packages
//! - Support for LZ4 and Zstd compression
//! - Support for solid archives
//! - Optional async API (enable `async` feature)
//!
//! ## Example
//!
//! ```no_run
//! use bg3rustpaklib::{Package, PackageSearch};
//!
//! fn main() -> bg3rustpaklib::Result<()> {
//!     // Open a PAK file
//!     let package = Package::open("Game.pak")?;
//!
//!     // Get package metadata
//!     let metadata = package.metadata();
//!     println!("Package version: {}", metadata.version);
//!     println!("File count: {}", metadata.file_count);
//!
//!     // List all files
//!     for file in package.files() {
//!         println!("{} ({} bytes)", file.name(), file.size());
//!     }
//!
//!     // Find files by pattern
//!     let lsf_files = package.find("**/*.lsf");
//!     println!("Found {} .lsf files", lsf_files.len());
//!
//!     // Extract a specific file
//!     if let Some(file) = package.get("Public/Game/GUI/Assets/Tooltips/tooltip.lsf") {
//!         let contents = package.read_file(file)?;
//!         println!("File size: {} bytes", contents.len());
//!     }
//!
//!     // Extract all files to a directory
//!     package.extract_all("output/")?;
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Async Support
//!
//! Enable the `async` feature to use async APIs:
//!
//! ```toml
//! [dependencies]
//! bg3rustpaklib = { version = "0.1", features = ["async"] }
//! ```
//!
//! ```no_run
//! use bg3rustpaklib::AsyncPackage;
//!
//! #[tokio::main]
//! async fn main() -> bg3rustpaklib::Result<()> {
//!     let package = AsyncPackage::open("Game.pak").await?;
//!     let metadata = package.metadata().await;
//!     println!("File count: {}", metadata.file_count);
//!     Ok(())
//! }
//! ```

#![warn(missing_docs)]
#![warn(rustdoc::missing_crate_level_docs)]

mod compression;
mod error;
pub mod loca;
mod package;
mod search;

#[cfg(feature = "async")]
mod async_support;

// Re-export main types
pub use compression::CompressionMethod;
pub use error::{PakError, Result};
pub use package::{
    get_package_flags, get_package_priority, ExtractedFile, Package, PackageBuilder,
    PackageBuilderOptions, PackageFileEntry, PackageFlags, PackageHeader, PackageMetadata,
    PackageVersion, PackagedFile,
};
pub use search::{FileFilter, PackageSearch};

#[cfg(feature = "async")]
pub use async_support::{AsyncPackage, FileInfo};

/// The LSPK package signature.
pub const SIGNATURE: u32 = package::PACKAGE_SIGNATURE;

/// Test helper module exposing compression internals for integration tests.
#[doc(hidden)]
pub mod compression_test_helpers {
    pub use crate::compression::{decompress, decompress_lz4, decompress_zstd, CompressionMethod};
}
