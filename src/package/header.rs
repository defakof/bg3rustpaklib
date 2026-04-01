//! Package header structures for different versions.

use crate::error::{PakError, Result};
use crate::package::version::{PackageFlags, PackageVersion, PACKAGE_SIGNATURE};
use std::io::{Read, Seek, SeekFrom};

fn read_u8<R: Read>(r: &mut R) -> std::io::Result<u8> {
    let mut b = [0u8; 1];
    r.read_exact(&mut b)?;
    Ok(b[0])
}

fn read_u16_le<R: Read>(r: &mut R) -> std::io::Result<u16> {
    let mut b = [0u8; 2];
    r.read_exact(&mut b)?;
    Ok(u16::from_le_bytes(b))
}

fn read_u32_le<R: Read>(r: &mut R) -> std::io::Result<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

fn read_u64_le<R: Read>(r: &mut R) -> std::io::Result<u64> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b)?;
    Ok(u64::from_le_bytes(b))
}

/// Common package header information extracted from version-specific headers.
#[derive(Debug, Clone)]
pub struct PackageHeader {
    /// Package format version.
    pub version: PackageVersion,
    /// Offset to the file list in the archive.
    pub file_list_offset: u64,
    /// Size of the (compressed) file list.
    pub file_list_size: u32,
    /// Number of files in the archive (for legacy versions).
    pub num_files: u32,
    /// Number of archive parts.
    pub num_parts: u16,
    /// Offset to packed data in archive part 0 (for legacy versions).
    pub data_offset: u32,
    /// Package flags.
    pub flags: PackageFlags,
    /// Package priority (for mod loading order).
    pub priority: u8,
    /// MD5 hash of the archive contents.
    pub md5: [u8; 16],
}

impl PackageHeader {
    /// Reads a package header from the given reader.
    ///
    /// This function automatically detects the header format and version.
    pub fn read<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        let file_size = reader.seek(SeekFrom::End(0))?;

        if file_size < 8 {
            return Err(PakError::FileTooSmall { size: file_size });
        }

        // First, check for V13+ format: signature at end of file
        reader.seek(SeekFrom::End(-4))?;
        let end_signature = read_u32_le(reader)?;

        if end_signature == PACKAGE_SIGNATURE {
            // Read header size from EOF-8
            reader.seek(SeekFrom::End(-8))?;
            let header_size = read_u32_le(reader)?;

            // Guard against underflow: header_size must not exceed file_size
            if header_size as u64 > file_size {
                return Err(PakError::CorruptedData(format!(
                    "V13 header_size {} exceeds file size {}",
                    header_size, file_size
                )));
            }

            let header_offset = file_size - header_size as u64;
            reader.seek(SeekFrom::Start(header_offset))?;

            return Self::read_v13_header(reader);
        }

        // Check for V10+ format: signature at start of file
        reader.seek(SeekFrom::Start(0))?;
        let start_signature = read_u32_le(reader)?;

        if start_signature == PACKAGE_SIGNATURE {
            let version = read_u32_le(reader)?;
            reader.seek(SeekFrom::Start(4))?; // Go back to after signature

            return match version {
                10 => Self::read_v10_header(reader),
                15 => Self::read_v15_header(reader),
                16 | 18 => Self::read_v16_header(reader),
                v => Err(PakError::UnsupportedVersion(v)),
            };
        }

        // Check for V7/V9 format: version at start, no signature
        reader.seek(SeekFrom::Start(0))?;
        let version = read_u32_le(reader)?;

        if version == 7 || version == 9 {
            reader.seek(SeekFrom::Start(0))?;
            return Self::read_v7_header(reader);
        }

        Err(PakError::NotAPakFile(
            "no valid signature found".to_string(),
        ))
    }

    /// Reads a V7/V9 header.
    fn read_v7_header<R: Read>(reader: &mut R) -> Result<Self> {
        let version = read_u32_le(reader)?;
        let data_offset = read_u32_le(reader)?;
        let num_parts = read_u32_le(reader)?;
        let file_list_size = read_u32_le(reader)?;
        let _little_endian = read_u8(reader)?;
        let num_files = read_u32_le(reader)?;

        let header_size = 4 + 4 + 4 + 4 + 1 + 4; // 21 bytes

        Ok(PackageHeader {
            version: PackageVersion::from_u32(version)?,
            file_list_offset: header_size as u64,
            file_list_size,
            num_files,
            num_parts: num_parts as u16,
            data_offset,
            flags: PackageFlags::NONE,
            priority: 0,
            md5: [0u8; 16],
        })
    }

    /// Reads a V10 header (after signature has been read).
    fn read_v10_header<R: Read>(reader: &mut R) -> Result<Self> {
        let version = read_u32_le(reader)?;
        let data_offset = read_u32_le(reader)?;
        let file_list_size = read_u32_le(reader)?;
        let num_parts = read_u16_le(reader)?;
        let flags = read_u8(reader)?;
        let priority = read_u8(reader)?;
        let num_files = read_u32_le(reader)?;

        let header_size = 4 + 4 + 4 + 4 + 2 + 1 + 1 + 4; // 24 bytes (including signature)

        Ok(PackageHeader {
            version: PackageVersion::from_u32(version)?,
            file_list_offset: header_size as u64,
            file_list_size,
            num_files,
            num_parts,
            data_offset,
            flags: PackageFlags::from_u8(flags),
            priority,
            md5: [0u8; 16],
        })
    }

    /// Reads a V13 header (from the end of file).
    fn read_v13_header<R: Read>(reader: &mut R) -> Result<Self> {
        let version = read_u32_le(reader)?;
        let file_list_offset = read_u32_le(reader)? as u64;
        let file_list_size = read_u32_le(reader)?;
        let num_parts = read_u16_le(reader)?;
        let flags = read_u8(reader)?;
        let priority = read_u8(reader)?;

        let mut md5 = [0u8; 16];
        reader.read_exact(&mut md5)?;

        Ok(PackageHeader {
            version: PackageVersion::from_u32(version)?,
            file_list_offset,
            file_list_size,
            num_files: 0, // Not stored in V13 header
            num_parts,
            data_offset: 0,
            flags: PackageFlags::from_u8(flags),
            priority,
            md5,
        })
    }

    /// Reads a V15 header.
    fn read_v15_header<R: Read>(reader: &mut R) -> Result<Self> {
        let version = read_u32_le(reader)?;
        let file_list_offset = read_u64_le(reader)?;
        let file_list_size = read_u32_le(reader)?;
        let flags = read_u8(reader)?;
        let priority = read_u8(reader)?;

        let mut md5 = [0u8; 16];
        reader.read_exact(&mut md5)?;

        Ok(PackageHeader {
            version: PackageVersion::from_u32(version)?,
            file_list_offset,
            file_list_size,
            num_files: 0,
            num_parts: 1, // V15 doesn't store num_parts, defaults to 1
            data_offset: 0,
            flags: PackageFlags::from_u8(flags),
            priority,
            md5,
        })
    }

    /// Reads a V16/V18 header.
    fn read_v16_header<R: Read>(reader: &mut R) -> Result<Self> {
        let version = read_u32_le(reader)?;
        let file_list_offset = read_u64_le(reader)?;
        let file_list_size = read_u32_le(reader)?;
        let flags = read_u8(reader)?;
        let priority = read_u8(reader)?;

        let mut md5 = [0u8; 16];
        reader.read_exact(&mut md5)?;

        let num_parts = read_u16_le(reader)?;

        Ok(PackageHeader {
            version: PackageVersion::from_u32(version)?,
            file_list_offset,
            file_list_size,
            num_files: 0,
            num_parts,
            data_offset: 0,
            flags: PackageFlags::from_u8(flags),
            priority,
            md5,
        })
    }

    /// Returns the size of the header for this version.
    pub fn header_size(&self) -> usize {
        match self.version {
            PackageVersion::V7 | PackageVersion::V9 => 21,
            PackageVersion::V10 => 24,
            PackageVersion::V13 => 28,
            PackageVersion::V15 => 35,
            PackageVersion::V16 | PackageVersion::V18 => 37,
        }
    }
}
