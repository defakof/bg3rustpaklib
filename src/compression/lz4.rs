//! LZ4 compression and decompression utilities.

use crate::error::{PakError, Result};
use lz4_flex::decompress_size_prepended;
use std::io::{Cursor, Read};

const LZ4_CHUNK_SIZE: usize = 256 * 1024;

/// Compresses data using single-block LZ4 (for file lists and small data).
pub fn compress_lz4_single(data: &[u8]) -> Vec<u8> {
    if data.is_empty() {
        return Vec::new();
    }
    lz4_flex::block::compress(data)
}

/// Decompresses LZ4 block-compressed data.
///
/// BG3 PAK files use a chunked LZ4 format where the data is split into
/// multiple compressed blocks, each prefixed with a 4-byte compressed size.
pub fn decompress_lz4(compressed: &[u8], uncompressed_size: usize) -> Result<Vec<u8>> {
    if compressed.is_empty() {
        return Ok(Vec::new());
    }

    if uncompressed_size == 0 {
        return Ok(Vec::new());
    }

    if let Ok(result) = decompress_lz4_chunked(compressed, uncompressed_size) {
        return Ok(result);
    }

    if let Ok(result) = decompress_lz4_single_block(compressed, uncompressed_size) {
        return Ok(result);
    }

    let message = concat!(
        "LZ4 block decompression failed: ",
        "the offset to copy is not contained in the decompressed buffer"
    );
    Err(PakError::Decompression(message.to_string()))
}

/// Decompresses BG3's chunked LZ4 format.
///
/// Format: sequence of (u32 compressed_size, compressed_block) pairs.
/// Each chunk decompresses to at most LZ4_CHUNK_SIZE bytes.
fn decompress_lz4_chunked(compressed: &[u8], uncompressed_size: usize) -> Result<Vec<u8>> {
    let mut decompressed = Vec::with_capacity(uncompressed_size);
    let mut pos = 0;

    while pos < compressed.len() && decompressed.len() < uncompressed_size {
        if pos + 4 > compressed.len() {
            break;
        }

        let chunk_compressed_size = u32::from_le_bytes([
            compressed[pos],
            compressed[pos + 1],
            compressed[pos + 2],
            compressed[pos + 3],
        ]) as usize;
        pos += 4;

        if chunk_compressed_size == 0 {
            break;
        }

        if pos + chunk_compressed_size > compressed.len() {
            return Err(PakError::Decompression(format!(
                "LZ4 chunk size {} exceeds remaining data {} at offset {}",
                chunk_compressed_size,
                compressed.len() - pos,
                pos
            )));
        }

        let chunk_data = &compressed[pos..pos + chunk_compressed_size];
        pos += chunk_compressed_size;

        let remaining = uncompressed_size - decompressed.len();
        let chunk_uncompressed_size = remaining.min(LZ4_CHUNK_SIZE);

        let chunk_decompressed = lz4_flex::block::decompress(chunk_data, chunk_uncompressed_size)
            .map_err(|e| {
                PakError::Decompression(format!("LZ4 chunk decompression failed: {}", e))
            })?;

        decompressed.extend_from_slice(&chunk_decompressed);
    }

    if decompressed.len() < uncompressed_size {
        return Err(PakError::Decompression(format!(
            "LZ4 decompression incomplete: got {} bytes, expected {}",
            decompressed.len(),
            uncompressed_size
        )));
    }

    decompressed.truncate(uncompressed_size);
    Ok(decompressed)
}

/// Tries to decompress as a single LZ4 block (fallback for non-chunked data).
fn decompress_lz4_single_block(compressed: &[u8], uncompressed_size: usize) -> Result<Vec<u8>> {
    // BUG-13 FIX: use checked_add to prevent overflow
    let buffer_size = uncompressed_size.checked_add(64).ok_or_else(|| {
        PakError::Decompression(format!(
            "LZ4 buffer size overflow: {} + 64",
            uncompressed_size
        ))
    })?;
    let mut decompressed = vec![0u8; buffer_size];

    match lz4_flex::block::decompress_into(compressed, &mut decompressed) {
        Ok(actual_size) => {
            decompressed.truncate(actual_size);
            Ok(decompressed)
        }
        Err(e) => lz4_flex::block::decompress(compressed, uncompressed_size).map_err(|_| {
            PakError::Decompression(format!("LZ4 single block decompression failed: {}", e))
        }),
    }
}

/// Decompresses LZ4 frame-compressed data.
///
/// This is used for solid archives where all files are compressed together
/// as a single LZ4 frame.
pub fn decompress_lz4_frame(compressed: &[u8]) -> Result<Vec<u8>> {
    if compressed.is_empty() {
        return Ok(Vec::new());
    }

    // LZ4 frame format has a magic number at the start
    // Check for LZ4 frame magic: 0x184D2204
    if compressed.len() < 4 {
        return Err(PakError::Decompression(
            "LZ4 frame data too short".to_string(),
        ));
    }

    let magic = u32::from_le_bytes([compressed[0], compressed[1], compressed[2], compressed[3]]);

    if magic == 0x184D2204 {
        // Standard LZ4 frame format
        decompress_lz4_frame_standard(compressed)
    } else {
        // Try as legacy frame or block format
        decompress_lz4_frame_legacy(compressed)
    }
}

/// Decompresses standard LZ4 frame format.
fn decompress_lz4_frame_standard(compressed: &[u8]) -> Result<Vec<u8>> {
    let mut decoder = lz4_flex::frame::FrameDecoder::new(compressed);
    let mut decompressed = Vec::new();

    decoder
        .read_to_end(&mut decompressed)
        .map_err(|e| PakError::Decompression(format!("LZ4 frame decompression failed: {}", e)))?;

    Ok(decompressed)
}

/// Decompresses legacy LZ4 frame format (used in some older packages).
fn decompress_lz4_frame_legacy(compressed: &[u8]) -> Result<Vec<u8>> {
    // Some packages use a simpler format with size-prepended blocks
    // Try multiple approaches

    // First, try as size-prepended data
    if let Ok(decompressed) = decompress_size_prepended(compressed) {
        return Ok(decompressed);
    }

    // Try reading as a sequence of blocks
    let mut cursor = Cursor::new(compressed);
    let mut decompressed = Vec::new();

    while cursor.position() < compressed.len() as u64 {
        let remaining = compressed.len() - cursor.position() as usize;
        if remaining < 4 {
            break;
        }

        // Read block size
        let mut size_buf = [0u8; 4];
        cursor.read_exact(&mut size_buf).map_err(|e| {
            PakError::Decompression(format!("Failed to read LZ4 block size: {}", e))
        })?;
        let block_size = u32::from_le_bytes(size_buf) as usize;

        if block_size == 0 {
            break;
        }

        if block_size > remaining - 4 {
            return Err(PakError::Decompression(format!(
                "Invalid LZ4 block size: {} > {}",
                block_size,
                remaining - 4
            )));
        }

        // Read and decompress block
        let start = cursor.position() as usize;
        let block_data = &compressed[start..start + block_size];
        cursor.set_position((start + block_size) as u64);

        // This block might have its own size prepended
        if let Ok(block_decompressed) = decompress_size_prepended(block_data) {
            decompressed.extend_from_slice(&block_decompressed);
        } else {
            // Just append raw data
            decompressed.extend_from_slice(block_data);
        }
    }

    if decompressed.is_empty() {
        return Err(PakError::Decompression(
            "Failed to decompress LZ4 frame data".to_string(),
        ));
    }

    Ok(decompressed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decompress_empty() {
        let result = decompress_lz4(&[], 0);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_decompress_lz4_block() {
        // Compress some test data
        let original = b"Hello, World! This is a test of LZ4 compression.";
        let compressed = lz4_flex::block::compress(original);

        let decompressed = decompress_lz4(&compressed, original.len()).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn test_decompress_lz4_corrupted() {
        let result = decompress_lz4(&[0xFF, 0xFE, 0xFD, 0xFC, 0xFB], 100);
        assert!(result.is_err());
    }

    #[test]
    fn test_decompress_lz4_zero_uncompressed() {
        let result = decompress_lz4(&[1, 2, 3, 4], 0);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_decompress_lz4_frame_too_short() {
        let result = decompress_lz4_frame(&[1, 2]);
        assert!(result.is_err());
    }

    #[test]
    fn test_lz4_roundtrip() {
        let original = b"The quick brown fox jumps over the lazy dog. Repeated: AAAAAAAAAA";
        let compressed = compress_lz4_single(original);
        assert!(!compressed.is_empty());

        let decompressed = decompress_lz4(&compressed, original.len()).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn test_compress_empty() {
        let compressed = compress_lz4_single(&[]);
        assert!(compressed.is_empty());
    }
}
