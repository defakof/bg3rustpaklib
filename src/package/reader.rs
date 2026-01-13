//! Package reading and extraction logic.

use crate::compression::{decompress, decompress_lz4, decompress_lz4_frame, CompressionMethod};
use crate::error::{PakError, Result};
use crate::package::file_entry::FileEntry;
use crate::package::header::PackageHeader;
use crate::package::version::PackageVersion;
use byteorder::{LittleEndian, ReadBytesExt};
use memmap2::Mmap;
use std::fs::File;
use std::io::{BufReader, Cursor, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A memory-mapped archive part.
struct ArchivePart {
    #[allow(dead_code)]
    file: File,
    mmap: Mmap,
}

impl ArchivePart {
    fn open(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        let mmap = unsafe { Mmap::map(&file)? };
        Ok(ArchivePart { file, mmap })
    }

    fn data(&self) -> &[u8] {
        &self.mmap
    }
}

/// A packaged file that can be extracted.
#[derive(Debug, Clone)]
pub struct PackagedFile {
    /// The file entry information.
    pub(crate) entry: FileEntry,
    /// Reference to the package for extraction (reserved for future use).
    #[allow(dead_code)]
    pub(crate) package_path: PathBuf,
    /// Data offset adjustment for legacy (V10 and earlier) packages.
    /// For V13+ packages, offsets in file entries are absolute.
    pub(crate) legacy_data_offset: u64,
    /// Whether this file is part of a solid archive.
    pub(crate) is_solid: bool,
    /// Offset within the solid archive's decompressed data.
    pub(crate) solid_offset: u64,
}

impl PackagedFile {
    /// Returns the file path within the package.
    pub fn name(&self) -> &str {
        &self.entry.name
    }

    /// Returns the uncompressed file size.
    pub fn size(&self) -> u64 {
        self.entry.size()
    }

    /// Returns the compressed file size (size on disk).
    pub fn compressed_size(&self) -> u64 {
        self.entry.size_on_disk
    }

    /// Returns whether the file is compressed.
    pub fn is_compressed(&self) -> bool {
        self.entry.is_compressed()
    }

    /// Returns the compression method used.
    pub fn compression_method(&self) -> CompressionMethod {
        self.entry.compression_method()
    }

    /// Returns the archive part number.
    pub fn archive_part(&self) -> u32 {
        self.entry.archive_part
    }

    /// Returns whether this entry represents a deleted file.
    pub fn is_deleted(&self) -> bool {
        self.entry.is_deletion()
    }

    /// Returns the CRC32 checksum if available.
    pub fn crc(&self) -> Option<u32> {
        if self.entry.crc != 0 {
            Some(self.entry.crc)
        } else {
            None
        }
    }
}

/// Metadata about a package.
#[derive(Debug, Clone)]
pub struct PackageMetadata {
    /// The package file path.
    pub path: PathBuf,
    /// Package format version.
    pub version: PackageVersion,
    /// Number of files in the package.
    pub file_count: usize,
    /// Total uncompressed size of all files.
    pub total_size: u64,
    /// Total compressed size of all files.
    pub total_compressed_size: u64,
    /// Package priority.
    pub priority: u8,
    /// Whether this is a solid archive.
    pub is_solid: bool,
    /// MD5 hash of the archive.
    pub md5: [u8; 16],
}

/// An opened PAK package.
pub struct Package {
    /// Package header information.
    header: PackageHeader,
    /// Package file path.
    path: PathBuf,
    /// Memory-mapped archive parts.
    parts: Vec<ArchivePart>,
    /// File entries.
    files: Vec<PackagedFile>,
    /// Decompressed solid archive data (if applicable).
    solid_data: Option<Arc<Vec<u8>>>,
}

impl Package {
    /// Opens a PAK package from the given path.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref().to_path_buf();

        // Read the header
        let file = File::open(&path)?;
        let mut reader = BufReader::new(file);
        let header = PackageHeader::read(&mut reader)?;

        // Open all archive parts
        let mut parts = Vec::with_capacity(header.num_parts as usize);
        parts.push(ArchivePart::open(&path)?);

        for part_num in 1..header.num_parts {
            let part_path = make_part_path(&path, part_num);
            if !part_path.exists() {
                return Err(PakError::MissingArchivePart(part_path));
            }
            parts.push(ArchivePart::open(&part_path)?);
        }

        // Read the file list
        let file_entries = Self::read_file_list(&parts[0], &header)?;

        // Calculate data offset for legacy packages
        // For V13+ packages with compressed file lists, offsets in file entries are absolute
        // For V10 and earlier, offsets are relative to the data section
        let legacy_data_offset = if header.version <= PackageVersion::V10 {
            header.data_offset as u64
        } else {
            // V13+ packages have absolute offsets in file entries
            0
        };

        // Check if this is a solid archive
        let is_solid = header.flags.is_solid();

        // Create packaged file entries
        let files: Vec<PackagedFile> = file_entries
            .into_iter()
            .map(|entry| PackagedFile {
                entry,
                package_path: path.clone(),
                legacy_data_offset,
                is_solid,
                solid_offset: 0,
            })
            .collect();

        let mut package = Package {
            header,
            path,
            parts,
            files,
            solid_data: None,
        };

        // If this is a solid archive, decompress the data upfront
        if is_solid && !package.files.is_empty() {
            package.decompress_solid_archive()?;
        }

        Ok(package)
    }

    /// Reads the file list from the archive.
    fn read_file_list(part: &ArchivePart, header: &PackageHeader) -> Result<Vec<FileEntry>> {
        let data = part.data();

        if header.version.has_compressed_file_list() {
            // V13+ uses compressed file list
            let offset = header.file_list_offset as usize;

            if offset >= data.len() {
                return Err(PakError::CorruptedFileList(
                    "file list offset out of bounds".to_string(),
                ));
            }

            let mut cursor = Cursor::new(&data[offset..]);

            // Read number of files
            let num_files = cursor.read_u32::<LittleEndian>()? as usize;

            // Read compressed size (for V14+)
            let compressed_size = if header.version > PackageVersion::V13 {
                cursor.read_u32::<LittleEndian>()? as usize
            } else {
                header.file_list_size as usize - 4
            };

            // Read compressed data
            let compressed_start = cursor.position() as usize;
            let compressed_end = compressed_start + compressed_size;

            if offset + compressed_end > data.len() {
                return Err(PakError::CorruptedFileList(
                    "compressed file list extends beyond file".to_string(),
                ));
            }

            let compressed = &data[offset + compressed_start..offset + compressed_end];

            // Decompress file list
            let entry_size = header.version.file_entry_size();
            let expected_size = entry_size * num_files;
            let decompressed = decompress_lz4(compressed, expected_size)?;

            // Parse file entries
            let mut entry_cursor = Cursor::new(&decompressed);
            FileEntry::read_entries(&mut entry_cursor, header.version, num_files)
        } else {
            // Legacy uncompressed file list
            let offset = header.file_list_offset as usize;
            let mut cursor = Cursor::new(&data[offset..]);
            FileEntry::read_entries(&mut cursor, header.version, header.num_files as usize)
        }
    }

    /// Decompresses a solid archive's data.
    fn decompress_solid_archive(&mut self) -> Result<()> {
        if self.files.is_empty() {
            return Ok(());
        }

        // Calculate the solid data bounds from file entry offsets
        let mut first_offset = u64::MAX;
        let mut last_offset = 0u64;

        for file in &self.files {
            let offset = file.entry.offset;
            let size = file.entry.size_on_disk;

            if offset < first_offset {
                first_offset = offset;
            }
            if offset + size > last_offset {
                last_offset = offset + size;
            }
        }

        // The solid frame starts 7 bytes before the first file offset (LZ4 frame header)
        // But we read from the first file offset position for safety
        let frame_start = if first_offset >= 7 {
            first_offset - 7
        } else {
            0
        };

        // Read the compressed frame
        let part_data = self.parts[0].data();
        if last_offset as usize > part_data.len() {
            return Err(PakError::CorruptedSolidArchive(
                "solid data bounds exceed file size".to_string(),
            ));
        }

        let frame_data = &part_data[frame_start as usize..last_offset as usize];

        // Decompress using LZ4 frame format
        let decompressed = decompress_lz4_frame(frame_data)?;

        // Update file offsets to point into the decompressed data
        let solid_data = Arc::new(decompressed);
        let mut current_offset = 0u64;

        for file in &mut self.files {
            file.solid_offset = current_offset;
            current_offset += file.entry.uncompressed_size;
        }

        self.solid_data = Some(solid_data);

        Ok(())
    }

    /// Returns metadata about this package.
    pub fn metadata(&self) -> PackageMetadata {
        let total_size: u64 = self.files.iter().map(|f| f.size()).sum();
        let total_compressed: u64 = self.files.iter().map(|f| f.compressed_size()).sum();

        PackageMetadata {
            path: self.path.clone(),
            version: self.header.version,
            file_count: self.files.len(),
            total_size,
            total_compressed_size: total_compressed,
            priority: self.header.priority,
            is_solid: self.header.flags.is_solid(),
            md5: self.header.md5,
        }
    }

    /// Returns a slice of all files in the package.
    pub fn files(&self) -> &[PackagedFile] {
        &self.files
    }

    /// Returns the number of files in the package.
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// Returns whether the package is empty.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Finds a file by exact path.
    pub fn get(&self, path: &str) -> Option<&PackagedFile> {
        // Normalize path separators
        let normalized = path.replace('\\', "/");
        self.files
            .iter()
            .find(|f| f.entry.name.replace('\\', "/") == normalized)
    }

    /// Reads a file's contents into a byte vector.
    pub fn read_file(&self, file: &PackagedFile) -> Result<Vec<u8>> {
        if file.is_deleted() {
            return Err(PakError::DeletedFile(file.name().to_string()));
        }

        if file.is_solid {
            // Read from decompressed solid data
            if let Some(solid_data) = &self.solid_data {
                let start = file.solid_offset as usize;
                let end = start + file.entry.uncompressed_size as usize;

                if end > solid_data.len() {
                    return Err(PakError::CorruptedSolidArchive(format!(
                        "file {} extends beyond solid data",
                        file.name()
                    )));
                }

                return Ok(solid_data[start..end].to_vec());
            } else {
                return Err(PakError::CorruptedSolidArchive(
                    "solid data not decompressed".to_string(),
                ));
            }
        }

        // Regular file: read from archive part
        let part_index = file.entry.archive_part as usize;
        if part_index >= self.parts.len() {
            return Err(PakError::MissingArchivePart(make_part_path(
                &self.path,
                file.entry.archive_part as u16,
            )));
        }

        let part_data = self.parts[part_index].data();

        // Calculate actual offset
        // For legacy packages (V10 and earlier), add the data offset for part 0
        // For V13+ packages, offsets are already absolute
        let offset = if part_index == 0 && file.legacy_data_offset > 0 {
            file.entry.offset + file.legacy_data_offset
        } else {
            file.entry.offset
        };

        let offset = offset as usize;
        let size = file.entry.size_on_disk as usize;

        if offset + size > part_data.len() {
            return Err(PakError::CorruptedData(format!(
                "file {} data extends beyond archive part",
                file.name()
            )));
        }

        let compressed = &part_data[offset..offset + size];

        // Decompress if needed
        decompress(
            compressed,
            file.entry.uncompressed_size as usize,
            file.compression_method(),
        )
    }

    /// Extracts a file to the given writer.
    pub fn extract_file<W: Write>(&self, file: &PackagedFile, writer: &mut W) -> Result<u64> {
        let data = self.read_file(file)?;
        writer.write_all(&data)?;
        Ok(data.len() as u64)
    }

    /// Extracts a file to the given path.
    pub fn extract_file_to_path(&self, file: &PackagedFile, output_path: &Path) -> Result<u64> {
        // Create parent directories
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| PakError::OutputCreation {
                path: parent.to_path_buf(),
                source: e,
            })?;
        }

        let mut output_file = File::create(output_path).map_err(|e| PakError::OutputCreation {
            path: output_path.to_path_buf(),
            source: e,
        })?;

        self.extract_file(file, &mut output_file)
    }

    /// Extracts all files to the given directory.
    pub fn extract_all<P: AsRef<Path>>(&self, output_dir: P) -> Result<()> {
        self.extract_filtered(output_dir, |_| true)
    }

    /// Extracts files matching the filter to the given directory.
    pub fn extract_filtered<P, F>(&self, output_dir: P, filter: F) -> Result<()>
    where
        P: AsRef<Path>,
        F: Fn(&PackagedFile) -> bool,
    {
        let output_dir = output_dir.as_ref();

        for file in &self.files {
            if file.is_deleted() {
                continue;
            }

            if !filter(file) {
                continue;
            }

            let output_path = output_dir.join(&file.entry.name);
            self.extract_file_to_path(file, &output_path)?;
        }

        Ok(())
    }
}

/// Creates the path for an archive part.
fn make_part_path(base_path: &Path, part_num: u16) -> PathBuf {
    let stem = base_path.file_stem().unwrap_or_default().to_string_lossy();
    let ext = base_path.extension().unwrap_or_default().to_string_lossy();
    let parent = base_path.parent().unwrap_or(Path::new(""));

    parent.join(format!("{}_{}.{}", stem, part_num, ext))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_make_part_path() {
        let base = Path::new("/path/to/Game.pak");
        assert_eq!(
            make_part_path(base, 1),
            PathBuf::from("/path/to/Game_1.pak")
        );
        assert_eq!(
            make_part_path(base, 2),
            PathBuf::from("/path/to/Game_2.pak")
        );
    }
}
