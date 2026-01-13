//! Package writing and creation logic.

use crate::compression::{compress_lz4_single, CompressionMethod};
use crate::error::{PakError, Result};
use crate::package::version::{PackageFlags, PackageVersion, PACKAGE_SIGNATURE};
use byteorder::{LittleEndian, WriteBytesExt};
use std::fs::File;
use std::io::{BufWriter, Read, Seek, Write};
use std::path::Path;

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

#[derive(Debug, Clone)]
pub struct PackageBuilderOptions {
    pub version: PackageVersion,
    pub compression: CompressionMethod,
    pub priority: u8,
    pub flags: PackageFlags,
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

#[derive(Debug, Clone)]
pub struct PackageFileEntry {
    pub archive_path: String,
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

pub struct PackageBuilder {
    options: PackageBuilderOptions,
    files: Vec<PackageFileEntry>,
}

impl PackageBuilder {
    pub fn new() -> Self {
        PackageBuilder {
            options: PackageBuilderOptions::default(),
            files: Vec::new(),
        }
    }

    pub fn with_options(options: PackageBuilderOptions) -> Self {
        PackageBuilder { options, files: Vec::new() }
    }

    pub fn version(mut self, version: PackageVersion) -> Self {
        self.options.version = version;
        self
    }

    pub fn compression(mut self, compression: CompressionMethod) -> Self {
        self.options.compression = compression;
        self
    }

    pub fn priority(mut self, priority: u8) -> Self {
        self.options.priority = priority;
        self
    }

    pub fn compute_hash(mut self, compute: bool) -> Self {
        self.options.compute_hash = compute;
        self
    }

    pub fn add_file<P: AsRef<Path>, S: Into<String>>(mut self, source_path: P, archive_path: S) -> Self {
        self.files.push(PackageFileEntry {
            source_path: source_path.as_ref().to_path_buf(),
            archive_path: normalize_path(archive_path.into()),
        });
        self
    }

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
                let relative = path.strip_prefix(base_path)
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

    pub fn build<P: AsRef<Path>>(self, output_path: P) -> Result<()> {
        let output_path = output_path.as_ref();
        let file = File::create(output_path).map_err(|e| PakError::OutputCreation {
            path: output_path.to_path_buf(),
            source: e,
        })?;
        let mut writer = BufWriter::new(file);

        match self.options.version {
            PackageVersion::V18 => self.write_v18(&mut writer),
            PackageVersion::V16 => self.write_v16(&mut writer),
            PackageVersion::V15 => self.write_v15(&mut writer),
            v => Err(PakError::UnsupportedVersion(v.as_u32())),
        }
    }

    fn write_v18<W: Write + Seek>(self, writer: &mut W) -> Result<()> {
        writer.write_u32::<LittleEndian>(PACKAGE_SIGNATURE)?;
        writer.write_u32::<LittleEndian>(self.options.version.as_u32())?;
        writer.write_u64::<LittleEndian>(0)?;
        writer.write_u32::<LittleEndian>(0)?;
        writer.write_u8(self.options.flags.as_u8())?;
        writer.write_u8(self.options.priority)?;
        let mut md5 = [0u8; 16];
        writer.write_all(&md5)?;
        writer.write_u16::<LittleEndian>(1)?;

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

        writer.write_u32::<LittleEndian>(written_entries.len() as u32)?;
        writer.write_u32::<LittleEndian>(compressed_list.len() as u32)?;
        writer.write_all(&compressed_list)?;

        let file_list_size = (4 + 4 + compressed_list.len()) as u32;

        if self.options.compute_hash {
            md5 = compute_archive_hash(&all_data, self.options.version);
        }

        writer.seek(std::io::SeekFrom::Start(8))?;
        writer.write_u64::<LittleEndian>(file_list_offset)?;
        writer.write_u32::<LittleEndian>(file_list_size)?;
        writer.seek(std::io::SeekFrom::Start(22))?;
        writer.write_all(&md5)?;

        Ok(())
    }

    fn write_v16<W: Write + Seek>(self, writer: &mut W) -> Result<()> {
        self.write_v18(writer)
    }

    fn write_v15<W: Write + Seek>(self, writer: &mut W) -> Result<()> {
        writer.write_u32::<LittleEndian>(PACKAGE_SIGNATURE)?;
        writer.write_u32::<LittleEndian>(self.options.version.as_u32())?;
        writer.write_u64::<LittleEndian>(0)?;
        writer.write_u32::<LittleEndian>(0)?;
        writer.write_u8(self.options.flags.as_u8())?;
        writer.write_u8(self.options.priority)?;
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

        writer.write_u32::<LittleEndian>(written_entries.len() as u32)?;
        writer.write_u32::<LittleEndian>(compressed_list.len() as u32)?;
        writer.write_all(&compressed_list)?;

        let file_list_size = 4 + 4 + compressed_list.len();

        if self.options.compute_hash {
            md5 = compute_archive_hash(&all_data, self.options.version);
        }

        writer.seek(std::io::SeekFrom::Start(20))?;
        writer.write_u64::<LittleEndian>(file_list_offset)?;
        writer.write_u32::<LittleEndian>(file_list_size as u32)?;
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
            data.write_u32::<LittleEndian>(offset_low)?;
            data.write_u16::<LittleEndian>(offset_high)?;
            data.write_u8(entry.archive_part as u8)?;
            data.write_u8(entry.compression_flags)?;
            data.write_u32::<LittleEndian>(entry.size_on_disk as u32)?;

            let uncompressed = if entry.compression_flags == 0 {
                0u32
            } else {
                entry.uncompressed_size as u32
            };
            data.write_u32::<LittleEndian>(uncompressed)?;
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

            data.write_u64::<LittleEndian>(entry.offset)?;
            data.write_u64::<LittleEndian>(entry.size_on_disk)?;

            let uncompressed = if entry.compression_flags == 0 {
                0u64
            } else {
                entry.uncompressed_size
            };
            data.write_u64::<LittleEndian>(uncompressed)?;

            data.write_u32::<LittleEndian>(entry.archive_part)?;
            data.write_u32::<LittleEndian>(entry.compression_flags as u32)?;
            data.write_u32::<LittleEndian>(0)?;
            data.write_u32::<LittleEndian>(0)?;
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
