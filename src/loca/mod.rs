//! Localization file support for Larian Studios games.
//!
//! This module provides reading and writing capabilities for `.loca` binary
//! localization files used by Baldur's Gate 3 and Divinity Original Sin games.
//!
//! # File Format
//!
//! The `.loca` format is a binary format with the following structure:
//!
//! - **Header** (12 bytes):
//!   - Signature: 4 bytes (`0x41434f4c` = "LOCA")
//!   - NumEntries: 4 bytes (u32)
//!   - TextsOffset: 4 bytes (u32)
//!
//! - **Entry Array** (70 bytes per entry):
//!   - Key: 64 bytes (UTF-8, null-padded)
//!   - Version: 2 bytes (u16)
//!   - Length: 4 bytes (u32) - includes null terminator
//!
//! - **Text Data**:
//!   - Sequential UTF-8 strings, each null-terminated
//!
//! # Example
//!
//! ```ignore
//! use bg3rustpaklib::loca::{LocaResource, LocaFormat, LocaUtils};
//!
//! // Load a .loca file
//! let resource = LocaUtils::load("localization.loca")?;
//!
//! // Access entries
//! for entry in &resource.entries {
//!     println!("{}: {}", entry.key, entry.text);
//! }
//!
//! // Save as XML
//! LocaUtils::save(&resource, "localization.xml", LocaFormat::Xml)?;
//! ```

mod error;
mod reader;
mod writer;

pub use error::{LocaError, Result};
pub use reader::{LocaReader, LocaXmlReader};
pub use writer::{LocaWriter, LocaXmlWriter};

use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;

/// The LOCA file signature ("LOCA" in little-endian).
pub const LOCA_SIGNATURE: u32 = 0x41434f4c;

/// All language names used by Baldur's Gate 3 for localization paths.
pub const BG3_LANGUAGES: &[&str] = &[
    "English",
    "French",
    "German",
    "Italian",
    "Spanish",
    "Portuguese",
    "Russian",
    "Polish",
    "Japanese",
    "Korean",
    "ChineseSimplified",
    "ChineseTraditional",
    "Turkish",
    "Ukrainian",
];

/// Extracts the BG3 language name from a PAK-internal path.
///
/// Looks for a `Localization/{Language}/...` segment anywhere in the path, so both
/// `"Localization/Russian/foo.loca"` and `"Mods/MyMod/Localization/Russian/russian.xml"`
/// return `Some("Russian")`.
///
/// Returns `None` if no such segment is found or the language isn't recognized.
pub fn detect_language_from_path(path: &str) -> Option<&'static str> {
    let normalized = path.replace('\\', "/");
    let parts: Vec<&str> = normalized.split('/').collect();

    for window in parts.windows(2) {
        if window[0].eq_ignore_ascii_case("Localization") {
            if let Some(&lang) = BG3_LANGUAGES
                .iter()
                .find(|&&lang| lang.eq_ignore_ascii_case(window[1]))
            {
                return Some(lang);
            }
        }
    }

    None
}

/// Size of the LOCA header in bytes.
pub const HEADER_SIZE: usize = 12;

/// Size of each entry in the entry array (excluding text data).
pub const ENTRY_SIZE: usize = 70;

/// Maximum size of a key string in bytes.
pub const MAX_KEY_SIZE: usize = 64;

/// A single localized text entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalizedText {
    /// The content UID key (e.g., "h00a1b2c3g4567h8i9j0k1l2m3n4o5p6q7r8s").
    pub key: String,
    /// The version number of this entry.
    pub version: u16,
    /// The localized text content.
    pub text: String,
}

impl LocalizedText {
    /// Creates a new localized text entry.
    pub fn new(key: impl Into<String>, version: u16, text: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            version,
            text: text.into(),
        }
    }
}

impl Default for LocalizedText {
    fn default() -> Self {
        Self {
            key: String::new(),
            version: 1,
            text: String::new(),
        }
    }
}

/// A collection of localized text entries.
#[derive(Debug, Clone, Default)]
pub struct LocaResource {
    /// The localized text entries.
    pub entries: Vec<LocalizedText>,
}

impl LocaResource {
    /// Creates a new empty localization resource.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Creates a localization resource with the given entries.
    pub fn with_entries(entries: Vec<LocalizedText>) -> Self {
        Self { entries }
    }

    /// Returns the number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true if there are no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Adds a new entry.
    pub fn add(&mut self, entry: LocalizedText) {
        self.entries.push(entry);
    }

    /// Finds an entry by key.
    pub fn find(&self, key: &str) -> Option<&LocalizedText> {
        self.entries.iter().find(|e| e.key == key)
    }

    /// Finds an entry by key (mutable).
    pub fn find_mut(&mut self, key: &str) -> Option<&mut LocalizedText> {
        self.entries.iter_mut().find(|e| e.key == key)
    }

    /// Removes an entry by key, returning it if found.
    pub fn remove(&mut self, key: &str) -> Option<LocalizedText> {
        if let Some(pos) = self.entries.iter().position(|e| e.key == key) {
            Some(self.entries.remove(pos))
        } else {
            None
        }
    }
}

/// The format of a localization file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocaFormat {
    /// Binary `.loca` format.
    Loca,
    /// XML format (compatible with lslib).
    Xml,
}

impl LocaFormat {
    /// Determines the format from a file extension.
    pub fn from_extension(path: &Path) -> Option<Self> {
        match path.extension()?.to_str()?.to_lowercase().as_str() {
            "loca" => Some(LocaFormat::Loca),
            "xml" => Some(LocaFormat::Xml),
            _ => None,
        }
    }
}

/// Utility functions for loading and saving localization files.
pub struct LocaUtils;

impl LocaUtils {
    /// Loads a localization resource from a file, detecting format from extension.
    pub fn load(path: impl AsRef<Path>) -> Result<LocaResource> {
        let path = path.as_ref();
        let format = LocaFormat::from_extension(path)
            .ok_or_else(|| LocaError::invalid_format("Unknown file extension"))?;
        Self::load_with_format(path, format)
    }

    /// Loads a localization resource from a file with explicit format.
    pub fn load_with_format(path: impl AsRef<Path>, format: LocaFormat) -> Result<LocaResource> {
        let file = File::open(path.as_ref())?;
        let reader = BufReader::new(file);
        Self::load_from_reader(reader, format)
    }

    /// Loads a localization resource from a reader.
    pub fn load_from_reader<R: std::io::Read>(reader: R, format: LocaFormat) -> Result<LocaResource> {
        match format {
            LocaFormat::Loca => {
                let mut loca_reader = LocaReader::new(reader);
                loca_reader.read()
            }
            LocaFormat::Xml => {
                let mut xml_reader = LocaXmlReader::new(reader);
                xml_reader.read()
            }
        }
    }

    /// Loads a localization resource from bytes.
    pub fn load_from_bytes(data: &[u8], format: LocaFormat) -> Result<LocaResource> {
        Self::load_from_reader(std::io::Cursor::new(data), format)
    }

    /// Saves a localization resource to a file, detecting format from extension.
    pub fn save(resource: &LocaResource, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let format = LocaFormat::from_extension(path)
            .ok_or_else(|| LocaError::invalid_format("Unknown file extension"))?;
        Self::save_with_format(resource, path, format)
    }

    /// Saves a localization resource to a file with explicit format.
    pub fn save_with_format(
        resource: &LocaResource,
        path: impl AsRef<Path>,
        format: LocaFormat,
    ) -> Result<()> {
        // Create parent directories if needed
        if let Some(parent) = path.as_ref().parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let file = File::create(path.as_ref())?;
        let writer = BufWriter::new(file);
        Self::save_to_writer(resource, writer, format)
    }

    /// Saves a localization resource to a writer.
    pub fn save_to_writer<W: std::io::Write>(
        resource: &LocaResource,
        writer: W,
        format: LocaFormat,
    ) -> Result<()> {
        match format {
            LocaFormat::Loca => {
                let mut loca_writer = LocaWriter::new(writer);
                loca_writer.write(resource)
            }
            LocaFormat::Xml => {
                let mut xml_writer = LocaXmlWriter::new(writer);
                xml_writer.write(resource)
            }
        }
    }

    /// Saves a localization resource to bytes.
    pub fn save_to_bytes(resource: &LocaResource, format: LocaFormat) -> Result<Vec<u8>> {
        let mut buffer = Vec::new();
        Self::save_to_writer(resource, &mut buffer, format)?;
        Ok(buffer)
    }

    /// Converts a localization resource between formats.
    pub fn convert(
        input_path: impl AsRef<Path>,
        output_path: impl AsRef<Path>,
    ) -> Result<()> {
        let resource = Self::load(input_path)?;
        Self::save(&resource, output_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_localized_text_new() {
        let entry = LocalizedText::new("test_key", 1, "Hello World");
        assert_eq!(entry.key, "test_key");
        assert_eq!(entry.version, 1);
        assert_eq!(entry.text, "Hello World");
    }

    #[test]
    fn test_loca_resource_operations() {
        let mut resource = LocaResource::new();
        assert!(resource.is_empty());

        resource.add(LocalizedText::new("key1", 1, "Text 1"));
        resource.add(LocalizedText::new("key2", 2, "Text 2"));
        assert_eq!(resource.len(), 2);

        let found = resource.find("key1");
        assert!(found.is_some());
        assert_eq!(found.unwrap().text, "Text 1");

        let removed = resource.remove("key1");
        assert!(removed.is_some());
        assert_eq!(resource.len(), 1);
    }

    #[test]
    fn test_loca_format_from_extension() {
        assert_eq!(
            LocaFormat::from_extension(Path::new("test.loca")),
            Some(LocaFormat::Loca)
        );
        assert_eq!(
            LocaFormat::from_extension(Path::new("test.xml")),
            Some(LocaFormat::Xml)
        );
        assert_eq!(
            LocaFormat::from_extension(Path::new("test.LOCA")),
            Some(LocaFormat::Loca)
        );
        assert_eq!(
            LocaFormat::from_extension(Path::new("test.txt")),
            None
        );
    }
}
