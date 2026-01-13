//! Compression and decompression utilities.

mod lz4;
mod zstd;

pub use self::lz4::{compress_lz4_single, decompress_lz4, decompress_lz4_frame};
pub use self::zstd::decompress_zstd;

use crate::error::{PakError, Result};
use flate2::read::ZlibDecoder;
use std::io::Read;

/// Compression methods used in PAK files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionMethod {
    /// No compression.
    None,
    /// Zlib compression.
    Zlib,
    /// LZ4 block compression.
    Lz4,
    /// Zstandard compression.
    Zstd,
}

impl CompressionMethod {
    /// Parses compression method from the compression flags byte.
    pub fn from_flags(flags: u8) -> Self {
        match flags & 0x0F {
            0 => CompressionMethod::None,
            1 => CompressionMethod::Zlib,
            2 => CompressionMethod::Lz4,
            3 => CompressionMethod::Zstd,
            _ => CompressionMethod::None, // Unknown methods treated as uncompressed
        }
    }

    /// Returns the flag value for this compression method.
    pub fn to_flags(self) -> u8 {
        match self {
            CompressionMethod::None => 0,
            CompressionMethod::Zlib => 1,
            CompressionMethod::Lz4 => 2,
            CompressionMethod::Zstd => 3,
        }
    }
}

/// Compression levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum CompressionLevel {
    /// Fast compression.
    Fast,
    /// Default/balanced compression.
    Default,
    /// Maximum compression.
    Max,
}

impl CompressionLevel {
    /// Parses compression level from the compression flags byte.
    #[allow(dead_code)]
    pub fn from_flags(flags: u8) -> Self {
        match flags & 0xF0 {
            0x10 => CompressionLevel::Fast,
            0x20 => CompressionLevel::Default,
            0x40 => CompressionLevel::Max,
            _ => CompressionLevel::Default,
        }
    }
}

/// Decompresses data using the specified compression method.
pub fn decompress(
    compressed: &[u8],
    uncompressed_size: usize,
    method: CompressionMethod,
) -> Result<Vec<u8>> {
    match method {
        CompressionMethod::None => Ok(compressed.to_vec()),
        CompressionMethod::Zlib => decompress_zlib(compressed),
        CompressionMethod::Lz4 => decompress_lz4(compressed, uncompressed_size),
        CompressionMethod::Zstd => decompress_zstd(compressed),
    }
}

/// Decompresses Zlib-compressed data.
fn decompress_zlib(compressed: &[u8]) -> Result<Vec<u8>> {
    let mut decoder = ZlibDecoder::new(compressed);
    let mut decompressed = Vec::new();
    decoder
        .read_to_end(&mut decompressed)
        .map_err(|e| PakError::Decompression(format!("Zlib decompression failed: {}", e)))?;
    Ok(decompressed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compression_method_from_flags() {
        assert_eq!(CompressionMethod::from_flags(0x00), CompressionMethod::None);
        assert_eq!(CompressionMethod::from_flags(0x01), CompressionMethod::Zlib);
        assert_eq!(CompressionMethod::from_flags(0x02), CompressionMethod::Lz4);
        assert_eq!(CompressionMethod::from_flags(0x03), CompressionMethod::Zstd);
        assert_eq!(CompressionMethod::from_flags(0x22), CompressionMethod::Lz4);
        // With level flags
    }

    #[test]
    fn test_compression_level_from_flags() {
        assert_eq!(CompressionLevel::from_flags(0x12), CompressionLevel::Fast);
        assert_eq!(
            CompressionLevel::from_flags(0x22),
            CompressionLevel::Default
        );
        assert_eq!(CompressionLevel::from_flags(0x42), CompressionLevel::Max);
    }
}
