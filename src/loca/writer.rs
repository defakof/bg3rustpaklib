//! Writers for localization files.

use std::io::Write;

use crate::loca::{
    error::{LocaError, Result},
    LocaResource, ENTRY_SIZE, HEADER_SIZE, LOCA_SIGNATURE, MAX_KEY_SIZE,
};

/// Writer for binary `.loca` files.
pub struct LocaWriter<W> {
    writer: W,
}

impl<W: Write> LocaWriter<W> {
    /// Creates a new LOCA writer.
    pub fn new(writer: W) -> Self {
        Self { writer }
    }

    /// Writes the localization resource to the stream.
    pub fn write(&mut self, resource: &LocaResource) -> Result<()> {
        // Validate keys before writing
        for entry in &resource.entries {
            let key_bytes = entry.key.as_bytes();
            if key_bytes.len() > MAX_KEY_SIZE {
                return Err(LocaError::KeyTooLong {
                    key: entry.key.clone(),
                    length: key_bytes.len(),
                });
            }
        }

        // Calculate texts offset
        let texts_offset = HEADER_SIZE + (resource.entries.len() * ENTRY_SIZE);

        // Write header
        self.write_header(resource.entries.len() as u32, texts_offset as u32)?;

        // Write entries
        for entry in &resource.entries {
            self.write_entry(&entry.key, entry.version, &entry.text)?;
        }

        // Write text data
        for entry in &resource.entries {
            self.write_text(&entry.text)?;
        }

        self.writer.flush()?;
        Ok(())
    }

    fn write_header(&mut self, num_entries: u32, texts_offset: u32) -> Result<()> {
        self.writer.write_all(&LOCA_SIGNATURE.to_le_bytes())?;
        self.writer.write_all(&num_entries.to_le_bytes())?;
        self.writer.write_all(&texts_offset.to_le_bytes())?;
        Ok(())
    }

    fn write_entry(&mut self, key: &str, version: u16, text: &str) -> Result<()> {
        // Write key (64 bytes, null-padded)
        let mut key_buf = [0u8; MAX_KEY_SIZE];
        let key_bytes = key.as_bytes();
        key_buf[..key_bytes.len()].copy_from_slice(key_bytes);
        self.writer.write_all(&key_buf)?;

        // Write version
        self.writer.write_all(&version.to_le_bytes())?;

        // Write length (text bytes + null terminator)
        let length = text.as_bytes().len() as u32 + 1;
        self.writer.write_all(&length.to_le_bytes())?;

        Ok(())
    }

    fn write_text(&mut self, text: &str) -> Result<()> {
        self.writer.write_all(text.as_bytes())?;
        self.writer.write_all(&[0u8])?; // Null terminator
        Ok(())
    }
}

/// Writer for XML localization files.
pub struct LocaXmlWriter<W> {
    writer: W,
    indent: bool,
}

impl<W: Write> LocaXmlWriter<W> {
    /// Creates a new XML localization writer.
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            indent: true,
        }
    }

    /// Creates a new XML writer without indentation.
    #[allow(dead_code)]
    pub fn new_compact(writer: W) -> Self {
        Self {
            writer,
            indent: false,
        }
    }

    /// Writes the localization resource as XML.
    pub fn write(&mut self, resource: &LocaResource) -> Result<()> {
        // Write XML declaration
        writeln!(self.writer, r#"<?xml version="1.0" encoding="utf-8"?>"#)?;

        // Write root element
        writeln!(self.writer, "<contentList>")?;

        // Write entries
        for entry in &resource.entries {
            let indent = if self.indent { "\t" } else { "" };
            let encoded_text = Self::encode_xml_entities(&entry.text);

            writeln!(
                self.writer,
                "{}<content contentuid=\"{}\" version=\"{}\">{}</content>",
                indent, entry.key, entry.version, encoded_text
            )?;
        }

        // Close root element
        writeln!(self.writer, "</contentList>")?;

        self.writer.flush()?;
        Ok(())
    }

    fn encode_xml_entities(text: &str) -> String {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('\'', "&apos;")
            .replace('"', "&quot;")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loca::{LocalizedText, LocaReader, LocaXmlReader};
    use std::io::Cursor;

    #[test]
    fn test_write_loca() {
        let mut resource = LocaResource::new();
        resource.add(LocalizedText::new("test_key", 1, "Hello World"));
        resource.add(LocalizedText::new("another_key", 2, "Goodbye"));

        let mut buffer = Vec::new();
        let mut writer = LocaWriter::new(&mut buffer);
        writer.write(&resource).unwrap();

        // Read it back
        let cursor = Cursor::new(buffer);
        let mut reader = LocaReader::new(cursor);
        let read_resource = reader.read().unwrap();

        assert_eq!(read_resource.len(), 2);
        assert_eq!(read_resource.entries[0].key, "test_key");
        assert_eq!(read_resource.entries[0].version, 1);
        assert_eq!(read_resource.entries[0].text, "Hello World");
        assert_eq!(read_resource.entries[1].key, "another_key");
        assert_eq!(read_resource.entries[1].version, 2);
        assert_eq!(read_resource.entries[1].text, "Goodbye");
    }

    #[test]
    fn test_write_xml() {
        let mut resource = LocaResource::new();
        resource.add(LocalizedText::new("key1", 1, "Hello & Goodbye"));
        resource.add(LocalizedText::new("key2", 2, "<text>"));

        let mut buffer = Vec::new();
        let mut writer = LocaXmlWriter::new(&mut buffer);
        writer.write(&resource).unwrap();

        let xml = String::from_utf8(buffer).unwrap();
        assert!(xml.contains("contentuid=\"key1\""));
        assert!(xml.contains("Hello &amp; Goodbye"));
        assert!(xml.contains("&lt;text&gt;"));

        // Read it back
        let cursor = Cursor::new(xml.as_bytes());
        let mut reader = LocaXmlReader::new(cursor);
        let read_resource = reader.read().unwrap();

        assert_eq!(read_resource.len(), 2);
        assert_eq!(read_resource.entries[0].text, "Hello & Goodbye");
        assert_eq!(read_resource.entries[1].text, "<text>");
    }

    #[test]
    fn test_key_too_long() {
        let mut resource = LocaResource::new();
        let long_key = "a".repeat(65);
        resource.add(LocalizedText::new(long_key.clone(), 1, "text"));

        let mut buffer = Vec::new();
        let mut writer = LocaWriter::new(&mut buffer);
        let result = writer.write(&resource);

        assert!(matches!(result, Err(LocaError::KeyTooLong { .. })));
    }

    #[test]
    fn test_roundtrip_empty() {
        let resource = LocaResource::new();

        let mut buffer = Vec::new();
        let mut writer = LocaWriter::new(&mut buffer);
        writer.write(&resource).unwrap();

        let cursor = Cursor::new(buffer);
        let mut reader = LocaReader::new(cursor);
        let read_resource = reader.read().unwrap();

        assert!(read_resource.is_empty());
    }

    #[test]
    fn test_roundtrip_unicode() {
        let mut resource = LocaResource::new();
        resource.add(LocalizedText::new("unicode_key", 1, "Hello"));
        resource.add(LocalizedText::new("chinese", 1, "Bonjour le monde"));

        let mut buffer = Vec::new();
        let mut writer = LocaWriter::new(&mut buffer);
        writer.write(&resource).unwrap();

        let cursor = Cursor::new(buffer);
        let mut reader = LocaReader::new(cursor);
        let read_resource = reader.read().unwrap();

        assert_eq!(read_resource.entries[0].text, "Hello");
        assert_eq!(read_resource.entries[1].text, "Bonjour le monde");
    }
}
