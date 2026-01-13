//! Package version definitions and utilities.

use crate::error::{PakError, Result};

/// The magic signature for LSPK packages: "LSPK" in little-endian.
pub const PACKAGE_SIGNATURE: u32 = 0x4B50534C;

/// PAK file format versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u32)]
pub enum PackageVersion {
    /// Divinity: Original Sin 1
    V7 = 7,
    /// D:OS 1 Enhanced Edition
    V9 = 9,
    /// Divinity: Original Sin 2
    V10 = 10,
    /// D:OS 2 Definitive Edition
    V13 = 13,
    /// Baldur's Gate 3 Early Access
    V15 = 15,
    /// BG3 EA Patch 4
    V16 = 16,
    /// BG3 Release (current)
    V18 = 18,
}

impl PackageVersion {
    /// Creates a PackageVersion from a raw version number.
    pub fn from_u32(version: u32) -> Result<Self> {
        match version {
            7 => Ok(PackageVersion::V7),
            9 => Ok(PackageVersion::V9),
            10 => Ok(PackageVersion::V10),
            13 => Ok(PackageVersion::V13),
            15 => Ok(PackageVersion::V15),
            16 => Ok(PackageVersion::V16),
            18 => Ok(PackageVersion::V18),
            v => Err(PakError::UnsupportedVersion(v)),
        }
    }

    /// Returns the raw version number.
    pub fn as_u32(self) -> u32 {
        self as u32
    }

    /// Returns whether this version has CRC checksums for files.
    pub fn has_crc(self) -> bool {
        matches!(
            self,
            PackageVersion::V10
                | PackageVersion::V13
                | PackageVersion::V15
                | PackageVersion::V16
        )
    }

    /// Returns the maximum size of a single package part.
    pub fn max_package_size(self) -> u64 {
        if self <= PackageVersion::V15 {
            0x40000000 // 1 GB
        } else {
            0x100000000 // 4 GB
        }
    }

    /// Returns the padding alignment size for this version.
    pub fn padding_size(self) -> usize {
        if self <= PackageVersion::V9 {
            0x1000 // 4 KB
        } else {
            0x40 // 64 bytes
        }
    }

    /// Returns whether this version uses a compressed file list.
    pub fn has_compressed_file_list(self) -> bool {
        self >= PackageVersion::V13
    }

    /// Returns whether the header is at the end of the file.
    pub fn header_at_end(self) -> bool {
        self == PackageVersion::V13
    }

    /// Returns whether this is a BG3 format version.
    pub fn is_bg3(self) -> bool {
        matches!(
            self,
            PackageVersion::V15 | PackageVersion::V16 | PackageVersion::V18
        )
    }

    /// Returns the size of the file entry structure for this version.
    pub fn file_entry_size(self) -> usize {
        match self {
            PackageVersion::V7 | PackageVersion::V9 => 272,
            PackageVersion::V10 | PackageVersion::V13 => 280,
            PackageVersion::V15 | PackageVersion::V16 => 296,
            PackageVersion::V18 => 272,
        }
    }
}

impl std::fmt::Display for PackageVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PackageVersion::V7 => write!(f, "V7 (D:OS 1)"),
            PackageVersion::V9 => write!(f, "V9 (D:OS 1 EE)"),
            PackageVersion::V10 => write!(f, "V10 (D:OS 2)"),
            PackageVersion::V13 => write!(f, "V13 (D:OS 2 DE)"),
            PackageVersion::V15 => write!(f, "V15 (BG3 EA)"),
            PackageVersion::V16 => write!(f, "V16 (BG3 EA Patch 4)"),
            PackageVersion::V18 => write!(f, "V18 (BG3 Release)"),
        }
    }
}

/// Package flags that control archive behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PackageFlags(u8);

impl PackageFlags {
    /// No flags set.
    pub const NONE: PackageFlags = PackageFlags(0);
    /// Allow memory-mapped access to files in this archive.
    pub const ALLOW_MEMORY_MAPPING: PackageFlags = PackageFlags(0x02);
    /// All files are compressed into a single LZ4 stream (solid archive).
    pub const SOLID: PackageFlags = PackageFlags(0x04);
    /// Archive contents should be preloaded on game startup.
    pub const PRELOAD: PackageFlags = PackageFlags(0x08);

    /// Creates flags from a raw byte value.
    pub fn from_u8(value: u8) -> Self {
        PackageFlags(value)
    }

    /// Returns the raw byte value.
    pub fn as_u8(self) -> u8 {
        self.0
    }

    /// Returns whether the archive allows memory mapping.
    pub fn allow_memory_mapping(self) -> bool {
        self.0 & 0x02 != 0
    }

    /// Returns whether this is a solid archive.
    pub fn is_solid(self) -> bool {
        self.0 & 0x04 != 0
    }

    /// Returns whether the archive should be preloaded.
    pub fn preload(self) -> bool {
        self.0 & 0x08 != 0
    }
}

impl std::ops::BitOr for PackageFlags {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        PackageFlags(self.0 | rhs.0)
    }
}

impl std::ops::BitAnd for PackageFlags {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        PackageFlags(self.0 & rhs.0)
    }
}
