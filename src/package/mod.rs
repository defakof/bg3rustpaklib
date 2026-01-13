//! Package handling module.
//!
//! This module contains all the types and logic for reading and writing PAK files.

pub(crate) mod file_entry;
mod header;
mod reader;
mod version;
mod writer;

pub use header::PackageHeader;
pub use reader::{Package, PackagedFile, PackageMetadata};
pub use version::{PackageFlags, PackageVersion, PACKAGE_SIGNATURE};
pub use writer::{get_package_flags, get_package_priority, PackageBuilder, PackageBuilderOptions, PackageFileEntry};
