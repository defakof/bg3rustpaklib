//! Package reading and extraction logic.

use crate::compression::{decompress, decompress_lz4, decompress_lz4_frame, CompressionMethod};
use crate::error::{PakError, Result};
use crate::package::file_entry::FileEntry;
use crate::package::header::PackageHeader;
use crate::package::version::PackageVersion;
use memmap2::Mmap;
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A file extracted from a PAK package, containing its path and raw data.
#[derive(Debug, Clone)]
pub struct ExtractedFile {
    /// The file's path inside the PAK (e.g., `"Localization/English/strings.loca"`).
    pub path: String,
    /// The decompressed file contents.
    pub data: Vec<u8>,
}

fn read_u32_le<R: Read>(r: &mut R) -> std::io::Result<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

/// A memory-mapped archive part.
struct ArchivePart {
    #[allow(dead_code)]
    file: File,
    mmap: Mmap,
}

impl ArchivePart {
    fn open(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        // SAFETY: the file handle is kept alive in ArchivePart for at least as long as the mmap.
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
    /// HashMap index for O(1) file lookups by normalized path.
    index: HashMap<String, usize>,
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
        // V13+ packages have absolute offsets in file entries
        let legacy_data_offset = if header.version <= PackageVersion::V10 {
            header.data_offset as u64
        } else {
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

        // Build O(1) name → index lookup
        let index: HashMap<String, usize> = files
            .iter()
            .enumerate()
            .map(|(i, f)| (f.entry.name.replace('\\', "/"), i))
            .collect();

        let mut package = Package {
            header,
            path,
            parts,
            files,
            solid_data: None,
            index,
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
            let num_files = read_u32_le(&mut cursor)? as usize;

            // Read compressed size (for V14+)
            let compressed_size = if header.version > PackageVersion::V13 {
                read_u32_le(&mut cursor)? as usize
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

        // Validate: all solid files must be in part 0
        for file in &self.files {
            if file.entry.archive_part != 0 {
                return Err(PakError::CorruptedSolidArchive(format!(
                    "solid archive file '{}' references part {} (expected 0)",
                    file.entry.name, file.entry.archive_part
                )));
            }
        }

        // Per lslib: the LZ4 frame starts at the first file's offset field
        // (which equals DataOffset + 7, where 7 is the LZ4 frame header size).
        // The frame ends at the last file's offset + size_on_disk.
        let frame_start = self
            .files
            .iter()
            .map(|f| f.entry.offset)
            .min()
            .unwrap_or(0);

        let frame_end = self
            .files
            .iter()
            .map(|f| f.entry.offset + f.entry.size_on_disk)
            .max()
            .unwrap_or(0);

        let part_data = self.parts[0].data();
        if frame_end as usize > part_data.len() {
            return Err(PakError::CorruptedSolidArchive(
                "solid data bounds exceed file size".to_string(),
            ));
        }

        let frame_data = &part_data[frame_start as usize..frame_end as usize];

        // Decompress using LZ4 frame format
        let decompressed = decompress_lz4_frame(frame_data)?;

        // Update solid_offset for each file: files are sequentially packed in
        // decompressed order, so we track cumulative uncompressed sizes.
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

    /// Finds a file by exact path (O(1) via HashMap index).
    pub fn get(&self, path: &str) -> Option<&PackagedFile> {
        let normalized = path.replace('\\', "/");
        self.index.get(&normalized).map(|&i| &self.files[i])
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

        // Calculate actual offset with overflow check
        let offset = if part_index == 0 && file.legacy_data_offset > 0 {
            file.entry
                .offset
                .checked_add(file.legacy_data_offset)
                .ok_or_else(|| {
                    PakError::CorruptedData(format!(
                        "file {} offset overflow: {} + {}",
                        file.name(),
                        file.entry.offset,
                        file.legacy_data_offset
                    ))
                })?
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

        let output_file =
            File::create(output_path).map_err(|e| PakError::OutputCreation {
                path: output_path.to_path_buf(),
                source: e,
            })?;
        let mut buffered = std::io::BufWriter::with_capacity(1 << 20, output_file);

        self.extract_file(file, &mut buffered)
    }

    /// Extracts all files to the given directory.
    pub fn extract_all<P: AsRef<Path>>(&self, output_dir: P) -> Result<()> {
        self.extract_filtered(output_dir, |_| true)
    }

    /// Reads all files with the given extension and returns their paths and data.
    ///
    /// The extension should be provided without a leading dot (e.g., `"loca"`, `"lsx"`).
    pub fn extract_by_extension(&self, ext: &str) -> Result<Vec<ExtractedFile>> {
        let ext_lower = ext.to_lowercase();
        let with_dot = format!(".{}", ext_lower);

        self.files
            .iter()
            .filter(|f| !f.is_deleted() && f.name().to_lowercase().ends_with(&with_dot))
            .map(|f| {
                let data = self.read_file(f)?;
                Ok(ExtractedFile {
                    path: f.name().to_string(),
                    data,
                })
            })
            .collect()
    }

    /// Reads all files under a directory prefix and returns their paths and data.
    ///
    /// The prefix is matched against the start of each file's path (case-insensitive,
    /// normalizes backslashes). A trailing `/` is added if not present.
    pub fn extract_by_directory(&self, dir_prefix: &str) -> Result<Vec<ExtractedFile>> {
        let prefix = {
            let mut p = dir_prefix.replace('\\', "/");
            if !p.ends_with('/') {
                p.push('/');
            }
            p.to_lowercase()
        };

        self.files
            .iter()
            .filter(|f| {
                !f.is_deleted() && f.name().replace('\\', "/").to_lowercase().starts_with(&prefix)
            })
            .map(|f| {
                let data = self.read_file(f)?;
                Ok(ExtractedFile {
                    path: f.name().to_string(),
                    data,
                })
            })
            .collect()
    }

    /// Extracts files matching the filter to the given directory in parallel.
    pub fn extract_filtered<P, F>(&self, output_dir: P, filter: F) -> Result<()>
    where
        P: AsRef<Path>,
        F: Fn(&PackagedFile) -> bool + Sync,
    {
        let output_dir = output_dir.as_ref();

        self.files
            .par_iter()
            .filter(|file| !file.is_deleted() && filter(file))
            .try_for_each(|file| {
                let output_path = output_dir.join(&file.entry.name);
                self.extract_file_to_path(file, &output_path).map(|_| ())
            })
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
