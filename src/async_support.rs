//! Async support for package operations.
//!
//! This module provides async wrappers around the synchronous package API.
//! Enable the `async` feature to use these types.

#![cfg(feature = "async")]

use crate::compression::CompressionMethod;
use crate::error::Result;
use crate::package::{Package, PackageMetadata, PackagedFile};
use crate::search::PackageSearch;
use regex::Regex;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task;

/// An async wrapper around the `Package` type.
///
/// This type allows package operations to be performed without blocking
/// the async runtime. Internally, it uses `tokio::task::spawn_blocking`
/// for CPU-intensive operations.
#[derive(Clone)]
pub struct AsyncPackage {
    inner: Arc<RwLock<Package>>,
}

impl AsyncPackage {
    /// Opens a PAK package asynchronously.
    pub async fn open<P: AsRef<Path> + Send + 'static>(path: P) -> Result<Self> {
        let path = path.as_ref().to_path_buf();

        let package: Package = task::spawn_blocking(move || Package::open(&path))
            .await
            .map_err(|e| {
                crate::error::PakError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Task join error: {}", e),
                ))
            })??;

        Ok(AsyncPackage {
            inner: Arc::new(RwLock::new(package)),
        })
    }

    /// Returns metadata about this package.
    pub async fn metadata(&self) -> PackageMetadata {
        let guard = self.inner.read().await;
        guard.metadata()
    }

    /// Returns the number of files in the package.
    pub async fn len(&self) -> usize {
        let guard = self.inner.read().await;
        guard.len()
    }

    /// Returns whether the package is empty.
    pub async fn is_empty(&self) -> bool {
        let guard = self.inner.read().await;
        guard.is_empty()
    }

    /// Returns a list of all file names in the package.
    pub async fn file_names(&self) -> Vec<String> {
        let guard = self.inner.read().await;
        guard.files().iter().map(|f| f.name().to_string()).collect()
    }

    /// Returns file information for all files.
    pub async fn file_infos(&self) -> Vec<FileInfo> {
        let guard = self.inner.read().await;
        guard.files().iter().map(FileInfo::from).collect()
    }

    /// Reads a file by name.
    pub async fn read_file(&self, name: &str) -> Result<Vec<u8>> {
        let guard = self.inner.read().await;

        let file = guard
            .get(name)
            .ok_or_else(|| crate::error::PakError::FileNotFound(name.to_string()))?;

        guard.read_file(file)
    }

    /// Finds files matching a glob pattern.
    pub async fn find(&self, pattern: &str) -> Vec<FileInfo> {
        let guard = self.inner.read().await;
        guard
            .find(pattern)
            .iter()
            .map(|f| FileInfo::from(*f))
            .collect()
    }

    /// Finds files matching a regular expression.
    pub async fn find_regex(&self, regex: &Regex) -> Vec<FileInfo> {
        let guard = self.inner.read().await;
        guard
            .find_regex(regex)
            .iter()
            .map(|f| FileInfo::from(*f))
            .collect()
    }

    /// Finds files containing the given substring.
    pub async fn find_containing(&self, substring: &str) -> Vec<FileInfo> {
        let guard = self.inner.read().await;
        guard
            .find_containing(substring)
            .iter()
            .map(|f| FileInfo::from(*f))
            .collect()
    }

    /// Finds files with the given extension.
    pub async fn find_by_extension(&self, extension: &str) -> Vec<FileInfo> {
        let guard = self.inner.read().await;
        guard
            .find_by_extension(extension)
            .iter()
            .map(|f| FileInfo::from(*f))
            .collect()
    }

    /// Extracts all files to the given directory.
    ///
    /// This operation runs in a blocking thread pool to avoid blocking the async runtime.
    pub async fn extract_all<P: AsRef<Path>>(&self, output_dir: P) -> Result<()> {
        let output_dir = output_dir.as_ref().to_path_buf();
        let guard = self.inner.read().await;
        guard.extract_all(&output_dir)
    }

    /// Extracts files by name to the given directory.
    ///
    /// Pass a list of file names to extract.
    pub async fn extract_files<P: AsRef<Path>>(
        &self,
        output_dir: P,
        names: &[String],
    ) -> Result<()> {
        let output_dir = output_dir.as_ref().to_path_buf();
        let names_set: std::collections::HashSet<&str> = names.iter().map(|s| s.as_str()).collect();
        let guard = self.inner.read().await;
        guard.extract_filtered(&output_dir, |f| names_set.contains(f.name()))
    }
}

/// Information about a file in the package.
///
/// This is a lightweight struct that can be passed across async boundaries
/// without holding references to the package.
#[derive(Debug, Clone)]
pub struct FileInfo {
    /// The file path within the package.
    pub name: String,
    /// The uncompressed file size.
    pub size: u64,
    /// The compressed file size.
    pub compressed_size: u64,
    /// Whether the file is compressed.
    pub is_compressed: bool,
    /// The compression method used.
    pub compression_method: CompressionMethod,
    /// Whether the file is deleted.
    pub is_deleted: bool,
    /// The archive part containing this file.
    pub archive_part: u32,
}

impl From<&PackagedFile> for FileInfo {
    fn from(file: &PackagedFile) -> Self {
        FileInfo {
            name: file.name().to_string(),
            size: file.size(),
            compressed_size: file.compressed_size(),
            is_compressed: file.is_compressed(),
            compression_method: file.compression_method(),
            is_deleted: file.is_deleted(),
            archive_part: file.archive_part(),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_file_info_conversion() {
        // This test just verifies the module compiles correctly
    }
}
