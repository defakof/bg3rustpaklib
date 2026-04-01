//! Package writing and creation logic.

use crate::compression::{compress_lz4_single, CompressionMethod};
use crate::error::{PakError, Result};
use crate::package::version::{PackageFlags, PackageVersion, PACKAGE_SIGNATURE};
use std::fs::File;
use std::io::{BufWriter, Read, Seek, Write};
use std::path::Path;

fn write_u8<W: Write>(w: &mut W, v: u8) -> std::io::Result<()> {
    w.write_all(&[v])
}

fn write_u16_le<W: Write>(w: &mut W, v: u16) -> std::io::Result<()> {
    w.write_all(&v.to_le_bytes())
}

fn write_u32_le<W: Write>(w: &mut W, v: u32) -> std::io::Result<()> {
    w.write_all(&v.to_le_bytes())
}

fn write_u64_le<W: Write>(w: &mut W, v: u64) -> std::io::Result<()> {
    w.write_all(&v.to_le_bytes())
}

/// Reads the priority value from an existing PAK file header.
///
/// Returns 0 if the file is not a valid PAK file.
pub fn get_package_priority(path: &Path) -> Result<u8> {
    let mut file = File::open(path)?;
    let mut header = [0u8; 40];
    file.read_exact(&mut header)?;

    let sig = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
    if sig != PACKAGE_SIGNATURE {
        return Ok(0);
    }

    Ok(header[21])
}

/// Reads the flags value from an existing PAK file header.
///
/// Returns 0 if the file is not a valid PAK file.
pub fn get_package_flags(path: &Path) -> Result<u8> {
    let mut file = File::open(path)?;
    let mut header = [0u8; 40];
    file.read_exact(&mut header)?;

    let sig = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
    if sig != PACKAGE_SIGNATURE {
        return Ok(0);
    }

    Ok(header[20])
}

/// Options for building a PAK package.
#[derive(Debug, Clone)]
pub struct PackageBuilderOptions {
    /// Package format version.
    pub version: PackageVersion,
    /// Compression method for files.
    pub compression: CompressionMethod,
    /// Package priority.
    pub priority: u8,
    /// Package flags.
    pub flags: PackageFlags,
    /// Whether to compute MD5 hash.
    pub compute_hash: bool,
}

impl Default for PackageBuilderOptions {
    fn default() -> Self {
        PackageBuilderOptions {
            version: PackageVersion::V18,
            compression: CompressionMethod::Lz4,
            priority: 0,
            flags: PackageFlags::NONE,
            compute_hash: false,
        }
    }
}

/// A file entry to be added to a package.
#[derive(Debug, Clone)]
pub struct PackageFileEntry {
    /// Path within the archive.
    pub archive_path: String,
    /// Path to the source file on disk.
    pub source_path: std::path::PathBuf,
}

struct WrittenFileEntry {
    name: String,
    offset: u64,
    size_on_disk: u64,
    uncompressed_size: u64,
    compression_flags: u8,
    archive_part: u32,
}

/// Builder for creating new PAK packages.
pub struct PackageBuilder {
    options: PackageBuilderOptions,
    files: Vec<PackageFileEntry>,
}

impl PackageBuilder {
    /// Creates a new package builder with default options.
    pub fn new() -> Self {
        PackageBuilder {
            options: PackageBuilderOptions::default(),
            files: Vec::new(),
        }
    }

    /// Creates a package builder with custom options.
    pub fn with_options(options: PackageBuilderOptions) -> Self {
        PackageBuilder {
            options,
            files: Vec::new(),
        }
    }

    /// Sets the package format version.
    pub fn version(mut self, version: PackageVersion) -> Self {
        self.options.version = version;
        self
    }

    /// Sets the compression method for files in the package.
    pub fn compression(mut self, compression: CompressionMethod) -> Self {
        self.options.compression = compression;
        self
    }

    /// Sets the package priority.
    pub fn priority(mut self, priority: u8) -> Self {
        self.options.priority = priority;
        self
    }

    /// Enables or disables MD5 hash computation for the archive.
    pub fn compute_hash(mut self, compute: bool) -> Self {
        self.options.compute_hash = compute;
        self
    }

    /// Adds a single file to the package.
    pub fn add_file<P: AsRef<Path>, S: Into<String>>(
        mut self,
        source_path: P,
        archive_path: S,
    ) -> Self {
        self.files.push(PackageFileEntry {
            source_path: source_path.as_ref().to_path_buf(),
            archive_path: normalize_path(archive_path.into()),
        });
        self
    }

    /// Recursively adds all files from a directory to the package.
    pub fn add_directory<P: AsRef<Path>>(mut self, dir_path: P) -> Result<Self> {
        let dir_path = dir_path.as_ref();
        self.add_directory_recursive(dir_path, dir_path)?;
        Ok(self)
    }

    fn add_directory_recursive(&mut self, base_path: &Path, current_path: &Path) -> Result<()> {
        for entry in std::fs::read_dir(current_path)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                self.add_directory_recursive(base_path, &path)?;
            } else if path.is_file() {
                let relative = path
                    .strip_prefix(base_path)
                    .map_err(|_| PakError::InvalidFileEntry("invalid path".to_string()))?;
                let archive_path = normalize_path(relative.to_string_lossy().to_string());
                self.files.push(PackageFileEntry {
                    source_path: path,
                    archive_path,
                });
            }
        }
        Ok(())
    }

    /// Builds the package and writes it to the specified output path.
    pub fn build<P: AsRef<Path>>(self, output_path: P) -> Result<()> {
        let output_path = output_path.as_ref();
        let file = File::create(output_path).map_err(|e| PakError::OutputCreation {
            path: output_path.to_path_buf(),
            source: e,
        })?;
        let mut writer = BufWriter::with_capacity(1 << 20, file);

        match self.options.version {
            PackageVersion::V18 => self.write_v18(&mut writer),
            PackageVersion::V16 => self.write_v16(&mut writer),
            PackageVersion::V15 => self.write_v15(&mut writer),
            v => Err(PakError::UnsupportedVersion(v.as_u32())),
        }
    }

    fn write_v18<W: Write + Seek>(self, writer: &mut W) -> Result<()> {
        // V18 header layout (40 bytes):
        // [0..4]   signature (u32)
        // [4..8]   version (u32)
        // [8..16]  file_list_offset (u64) — placeholder, fixed up below
        // [16..20] file_list_size (u32)   — placeholder, fixed up below
        // [20]     flags (u8)
        // [21]     priority (u8)
        // [22..38] md5 (16 bytes)         — placeholder, filled if compute_hash
        // [38..40] num_parts (u16)
        write_u32_le(writer, PACKAGE_SIGNATURE)?;
        write_u32_le(writer, self.options.version.as_u32())?;
        write_u64_le(writer, 0)?; // file_list_offset placeholder
        write_u32_le(writer, 0)?; // file_list_size placeholder
        write_u8(writer, self.options.flags.as_u8())?;
        write_u8(writer, self.options.priority)?;
        let mut md5 = [0u8; 16];
        writer.write_all(&md5)?;
        write_u16_le(writer, 1)?; // num_parts

        let mut written_entries: Vec<WrittenFileEntry> = Vec::with_capacity(self.files.len());
        let mut all_data: Vec<Vec<u8>> = Vec::new();

        for file_entry in &self.files {
            let mut source_file = File::open(&file_entry.source_path)?;
            let mut data = Vec::new();
            source_file.read_to_end(&mut data)?;

            let uncompressed_size = data.len() as u64;
            let offset = writer.stream_position()?;

            let (compressed_data, compression_flags) = match self.options.compression {
                CompressionMethod::None => (data.clone(), 0u8),
                CompressionMethod::Lz4 => {
                    let compressed = compress_lz4_single(&data);
                    if compressed.len() < data.len() {
                        (compressed, 0x12)
                    } else {
                        (data.clone(), 0u8)
                    }
                }
                _ => (data.clone(), 0u8),
            };

            writer.write_all(&compressed_data)?;
            let size_on_disk = compressed_data.len() as u64;

            written_entries.push(WrittenFileEntry {
                name: file_entry.archive_path.clone(),
                offset,
                size_on_disk,
                uncompressed_size,
                compression_flags,
                archive_part: 0,
            });

            all_data.push(data);
        }

        let file_list_offset = writer.stream_position()?;
        let file_list_data = self.build_file_list_v18(&written_entries)?;
        let compressed_list = compress_lz4_single(&file_list_data);

        write_u32_le(writer, written_entries.len() as u32)?;
        write_u32_le(writer, compressed_list.len() as u32)?;
        writer.write_all(&compressed_list)?;

        let file_list_size = (4 + 4 + compressed_list.len()) as u32;

        if self.options.compute_hash {
            md5 = compute_archive_hash(&all_data, self.options.version);
        }

        // Fix up the header: seek to offset 8 (after sig+ver)
        writer.seek(std::io::SeekFrom::Start(8))?;
        write_u64_le(writer, file_list_offset)?;
        write_u32_le(writer, file_list_size)?;
        // flags (20) and priority (21) already written; seek to 22 for md5
        writer.seek(std::io::SeekFrom::Start(22))?;
        writer.write_all(&md5)?;

        Ok(())
    }

    fn write_v16<W: Write + Seek>(self, writer: &mut W) -> Result<()> {
        self.write_v18(writer)
    }

    fn write_v15<W: Write + Seek>(self, writer: &mut W) -> Result<()> {
        // V15 header layout (38 bytes, no num_parts field):
        // [0..4]   signature (u32)
        // [4..8]   version (u32)
        // [8..16]  file_list_offset (u64) — placeholder, fixed up below
        // [16..20] file_list_size (u32)   — placeholder, fixed up below
        // [20]     flags (u8)
        // [21]     priority (u8)
        // [22..38] md5 (16 bytes)         — placeholder, filled if compute_hash
        write_u32_le(writer, PACKAGE_SIGNATURE)?;
        write_u32_le(writer, self.options.version.as_u32())?;
        write_u64_le(writer, 0)?; // file_list_offset placeholder
        write_u32_le(writer, 0)?; // file_list_size placeholder
        write_u8(writer, self.options.flags.as_u8())?;
        write_u8(writer, self.options.priority)?;
        let mut md5 = [0u8; 16];
        writer.write_all(&md5)?;

        let mut written_entries: Vec<WrittenFileEntry> = Vec::with_capacity(self.files.len());
        let mut all_data: Vec<Vec<u8>> = Vec::new();

        for file_entry in &self.files {
            let mut source_file = File::open(&file_entry.source_path)?;
            let mut data = Vec::new();
            source_file.read_to_end(&mut data)?;

            let uncompressed_size = data.len() as u64;
            let offset = writer.stream_position()?;

            let (compressed_data, compression_flags) = match self.options.compression {
                CompressionMethod::None => (data.clone(), 0u8),
                CompressionMethod::Lz4 => {
                    let compressed = compress_lz4_single(&data);
                    if compressed.len() < data.len() {
                        (compressed, 0x12)
                    } else {
                        (data.clone(), 0u8)
                    }
                }
                _ => (data.clone(), 0u8),
            };

            writer.write_all(&compressed_data)?;
            let size_on_disk = compressed_data.len() as u64;

            written_entries.push(WrittenFileEntry {
                name: file_entry.archive_path.clone(),
                offset,
                size_on_disk,
                uncompressed_size,
                compression_flags,
                archive_part: 0,
            });

            all_data.push(data);
        }

        let file_list_offset = writer.stream_position()?;
        let file_list_data = self.build_file_list_v15(&written_entries)?;
        let compressed_list = compress_lz4_single(&file_list_data);

        write_u32_le(writer, written_entries.len() as u32)?;
        write_u32_le(writer, compressed_list.len() as u32)?;
        writer.write_all(&compressed_list)?;

        let file_list_size = 4 + 4 + compressed_list.len();

        if self.options.compute_hash {
            md5 = compute_archive_hash(&all_data, self.options.version);
        }

        // Fix up the header: seek to offset 8 (after sig+ver)
        writer.seek(std::io::SeekFrom::Start(8))?;
        write_u64_le(writer, file_list_offset)?;
        write_u32_le(writer, file_list_size as u32)?;
        // flags (20) and priority (21) already written; seek to 22 for md5
        writer.seek(std::io::SeekFrom::Start(22))?;
        writer.write_all(&md5)?;

        Ok(())
    }

    fn build_file_list_v18(&self, entries: &[WrittenFileEntry]) -> Result<Vec<u8>> {
        let mut data = Vec::new();

        for entry in entries {
            let mut name_buf = [0u8; 256];
            let name_bytes = entry.name.as_bytes();
            let len = name_bytes.len().min(255);
            name_buf[..len].copy_from_slice(&name_bytes[..len]);
            data.extend_from_slice(&name_buf);

            let offset_low = (entry.offset & 0xFFFFFFFF) as u32;
            let offset_high = ((entry.offset >> 32) & 0xFFFF) as u16;
            write_u32_le(&mut data, offset_low)?;
            write_u16_le(&mut data, offset_high)?;
            write_u8(&mut data, entry.archive_part as u8)?;
            write_u8(&mut data, entry.compression_flags)?;
            write_u32_le(&mut data, entry.size_on_disk as u32)?;

            // Per lslib: uncompressed_size is 0 for uncompressed entries
            let uncompressed = if entry.compression_flags == 0 {
                0u32
            } else {
                entry.uncompressed_size as u32
            };
            write_u32_le(&mut data, uncompressed)?;
        }

        Ok(data)
    }

    fn build_file_list_v15(&self, entries: &[WrittenFileEntry]) -> Result<Vec<u8>> {
        let mut data = Vec::new();

        for entry in entries {
            let mut name_buf = [0u8; 256];
            let name_bytes = entry.name.as_bytes();
            let len = name_bytes.len().min(255);
            name_buf[..len].copy_from_slice(&name_bytes[..len]);
            data.extend_from_slice(&name_buf);

            write_u64_le(&mut data, entry.offset)?;
            write_u64_le(&mut data, entry.size_on_disk)?;

            // Per lslib: uncompressed_size is 0 for uncompressed entries
            let uncompressed = if entry.compression_flags == 0 {
                0u64
            } else {
                entry.uncompressed_size
            };
            write_u64_le(&mut data, uncompressed)?;

            write_u32_le(&mut data, entry.archive_part)?;
            write_u32_le(&mut data, entry.compression_flags as u32)?;
            write_u32_le(&mut data, 0)?;
            write_u32_le(&mut data, 0)?;
        }

        Ok(data)
    }
}

fn compute_archive_hash(data: &[Vec<u8>], _version: PackageVersion) -> [u8; 16] {
    let mut hasher = md5::Context::new();
    for chunk in data {
        hasher.consume(chunk);
    }
    let result = hasher.compute();

    let mut md5 = [0u8; 16];
    md5.copy_from_slice(&result.0);

    // Intentional Larian obfuscation: each byte is incremented by 1 (wrapping).
    // Matches lslib's PackageWriter.cs ComputeArchiveHash(): hash[i] += 1 for all i.
    for byte in &mut md5 {
        *byte = byte.wrapping_add(1);
    }

    md5
}

impl Default for PackageBuilder {
    fn default() -> Self {
        Self::new()
    }
}

fn normalize_path(path: String) -> String {
    path.replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path() {
        assert_eq!(normalize_path("foo\\bar\\baz".to_string()), "foo/bar/baz");
        assert_eq!(normalize_path("foo/bar/baz".to_string()), "foo/bar/baz");
    }
}
