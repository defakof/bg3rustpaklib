//! File entry structures for different package versions.

use crate::compression::CompressionMethod;
use crate::error::Result;
use crate::package::version::PackageVersion;
use byteorder::{LittleEndian, ReadBytesExt};
use std::io::Read;

/// Deletion marker offset value.
const DELETION_MARKER: u64 = 0x0000BEEFDEADBEEF;

/// A file entry in the package.
#[derive(Debug, Clone)]
pub struct FileEntry {
    /// The file path within the package.
    pub name: String,
    /// The archive part containing this file (for multi-part archives).
    pub archive_part: u32,
    /// Offset to the file data within the archive part.
    pub offset: u64,
    /// Size of the file data on disk (compressed size).
    pub size_on_disk: u64,
    /// Uncompressed size of the file.
    pub uncompressed_size: u64,
    /// Compression method and flags.
    pub compression_flags: u8,
    /// CRC32 checksum (if available).
    pub crc: u32,
}

impl FileEntry {
    /// Returns the compression method used for this file.
    pub fn compression_method(&self) -> CompressionMethod {
        CompressionMethod::from_flags(self.compression_flags)
    }

    /// Returns whether this file is compressed.
    pub fn is_compressed(&self) -> bool {
        self.compression_method() != CompressionMethod::None
    }

    /// Returns the actual file size (uncompressed if compressed, otherwise on-disk size).
    pub fn size(&self) -> u64 {
        if self.is_compressed() {
            self.uncompressed_size
        } else {
            self.size_on_disk
        }
    }

    /// Returns whether this entry represents a deleted file.
    pub fn is_deletion(&self) -> bool {
        (self.offset & 0x0000FFFFFFFFFFFF) == DELETION_MARKER
    }

    /// Reads file entries from the given reader for the specified version.
    pub fn read_entries<R: Read>(
        reader: &mut R,
        version: PackageVersion,
        count: usize,
    ) -> Result<Vec<FileEntry>> {
        let mut entries = Vec::with_capacity(count);

        for _ in 0..count {
            let entry = match version {
                PackageVersion::V7 | PackageVersion::V9 => Self::read_v7(reader)?,
                PackageVersion::V10 | PackageVersion::V13 => Self::read_v10(reader)?,
                PackageVersion::V15 | PackageVersion::V16 => Self::read_v15(reader)?,
                PackageVersion::V18 => Self::read_v18(reader)?,
            };
            entries.push(entry);
        }

        Ok(entries)
    }

    /// Reads a V7/V9 file entry (272 bytes).
    fn read_v7<R: Read>(reader: &mut R) -> Result<Self> {
        let mut name_buf = [0u8; 256];
        reader.read_exact(&mut name_buf)?;
        let name = read_null_terminated_string(&name_buf);

        let offset = reader.read_u32::<LittleEndian>()? as u64;
        let size_on_disk = reader.read_u32::<LittleEndian>()? as u64;
        let uncompressed_size = reader.read_u32::<LittleEndian>()? as u64;
        let archive_part = reader.read_u32::<LittleEndian>()?;

        // V7 uses Zlib compression if uncompressed_size > 0
        let compression_flags = if uncompressed_size > 0 { 0x21 } else { 0x00 };

        Ok(FileEntry {
            name,
            archive_part,
            offset,
            size_on_disk,
            uncompressed_size,
            compression_flags,
            crc: 0,
        })
    }

    /// Reads a V10/V13 file entry (280 bytes).
    fn read_v10<R: Read>(reader: &mut R) -> Result<Self> {
        let mut name_buf = [0u8; 256];
        reader.read_exact(&mut name_buf)?;
        let name = read_null_terminated_string(&name_buf);

        let offset = reader.read_u32::<LittleEndian>()? as u64;
        let size_on_disk = reader.read_u32::<LittleEndian>()? as u64;
        let uncompressed_size = reader.read_u32::<LittleEndian>()? as u64;
        let archive_part = reader.read_u32::<LittleEndian>()?;
        let flags = reader.read_u32::<LittleEndian>()?;
        let crc = reader.read_u32::<LittleEndian>()?;

        Ok(FileEntry {
            name,
            archive_part,
            offset,
            size_on_disk,
            uncompressed_size,
            compression_flags: flags as u8,
            crc,
        })
    }

    /// Reads a V15/V16 file entry (296 bytes).
    fn read_v15<R: Read>(reader: &mut R) -> Result<Self> {
        let mut name_buf = [0u8; 256];
        reader.read_exact(&mut name_buf)?;
        let name = read_null_terminated_string(&name_buf);

        let offset = reader.read_u64::<LittleEndian>()?;
        let size_on_disk = reader.read_u64::<LittleEndian>()?;
        let uncompressed_size = reader.read_u64::<LittleEndian>()?;
        let archive_part = reader.read_u32::<LittleEndian>()?;
        let flags = reader.read_u32::<LittleEndian>()?;
        let crc = reader.read_u32::<LittleEndian>()?;
        let _unknown = reader.read_u32::<LittleEndian>()?;

        Ok(FileEntry {
            name,
            archive_part,
            offset,
            size_on_disk,
            uncompressed_size,
            compression_flags: flags as u8,
            crc,
        })
    }

    /// Reads a V18 file entry (272 bytes).
    fn read_v18<R: Read>(reader: &mut R) -> Result<Self> {
        let mut name_buf = [0u8; 256];
        reader.read_exact(&mut name_buf)?;
        let name = read_null_terminated_string(&name_buf);

        let offset_low = reader.read_u32::<LittleEndian>()? as u64;
        let offset_high = reader.read_u16::<LittleEndian>()? as u64;
        let offset = offset_low | (offset_high << 32);

        let archive_part = reader.read_u8()? as u32;
        let flags = reader.read_u8()?;

        let size_on_disk = reader.read_u32::<LittleEndian>()? as u64;
        let uncompressed_size = reader.read_u32::<LittleEndian>()? as u64;

        Ok(FileEntry {
            name,
            archive_part,
            offset,
            size_on_disk,
            uncompressed_size,
            compression_flags: flags,
            crc: 0, // V18 doesn't have CRC
        })
    }
}

/// Reads a null-terminated UTF-8 string from a fixed-size buffer.
fn read_null_terminated_string(buf: &[u8]) -> String {
    let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..len]).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_null_terminated_string() {
        let buf = b"test\0garbage";
        assert_eq!(read_null_terminated_string(buf), "test");

        let buf = b"no null terminator";
        assert_eq!(read_null_terminated_string(buf), "no null terminator");

        let buf = b"\0empty";
        assert_eq!(read_null_terminated_string(buf), "");
    }

    #[test]
    fn test_compression_method() {
        let entry = FileEntry {
            name: "test".to_string(),
            archive_part: 0,
            offset: 0,
            size_on_disk: 100,
            uncompressed_size: 200,
            compression_flags: 0x02, // LZ4
            crc: 0,
        };
        assert_eq!(entry.compression_method(), CompressionMethod::Lz4);
        assert!(entry.is_compressed());
    }

    #[test]
    fn test_deletion_marker() {
        let entry = FileEntry {
            name: "deleted".to_string(),
            archive_part: 0,
            offset: DELETION_MARKER,
            size_on_disk: 0,
            uncompressed_size: 0,
            compression_flags: 0,
            crc: 0,
        };
        assert!(entry.is_deletion());
    }
}
