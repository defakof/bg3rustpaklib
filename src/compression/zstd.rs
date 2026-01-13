//! Zstandard decompression utilities.

use crate::error::{PakError, Result};
use std::io::Read;

/// Decompresses Zstd-compressed data.
pub fn decompress_zstd(compressed: &[u8]) -> Result<Vec<u8>> {
    if compressed.is_empty() {
        return Ok(Vec::new());
    }

    let mut decoder = zstd::stream::Decoder::new(compressed)
        .map_err(|e| PakError::Decompression(format!("Failed to create Zstd decoder: {}", e)))?;

    let mut decompressed = Vec::new();
    decoder
        .read_to_end(&mut decompressed)
        .map_err(|e| PakError::Decompression(format!("Zstd decompression failed: {}", e)))?;

    Ok(decompressed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decompress_empty() {
        let result = decompress_zstd(&[]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_decompress_zstd() {
        // Compress some test data
        let original = b"Hello, World! This is a test of Zstd compression.";
        let compressed = zstd::encode_all(&original[..], 3).unwrap();

        let decompressed = decompress_zstd(&compressed).unwrap();
        assert_eq!(decompressed, original);
    }
}
