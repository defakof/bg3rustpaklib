//! Readers for localization files.

use std::io::Read;

use crate::loca::{
    error::{LocaError, Result},
    LocalizedText, LocaResource, ENTRY_SIZE, HEADER_SIZE, LOCA_SIGNATURE, MAX_KEY_SIZE,
};

/// Header structure for LOCA files.
#[derive(Debug, Clone, Copy)]
struct LocaHeader {
    signature: u32,
    num_entries: u32,
    texts_offset: u32,
}

/// Entry structure in the entry array.
#[derive(Debug, Clone)]
struct LocaEntry {
    key: String,
    version: u16,
    length: u32,
}

/// Reader for binary `.loca` files.
pub struct LocaReader<R> {
    reader: R,
}

impl<R: Read> LocaReader<R> {
    /// Creates a new LOCA reader.
    pub fn new(reader: R) -> Self {
        Self { reader }
    }

    /// Reads the localization resource from the stream.
    pub fn read(&mut self) -> Result<LocaResource> {
        let header = self.read_header()?;

        if header.signature != LOCA_SIGNATURE {
            return Err(LocaError::InvalidSignature {
                expected: LOCA_SIGNATURE,
                actual: header.signature,
            });
        }

        let entries = self.read_entries(header.num_entries as usize)?;

        // Calculate how many bytes we've read so far
        let bytes_read = HEADER_SIZE + (header.num_entries as usize * ENTRY_SIZE);

        // Skip to texts offset if needed
        if header.texts_offset as usize > bytes_read {
            let to_skip = header.texts_offset as usize - bytes_read;
            let mut skip_buf = vec![0u8; to_skip];
            self.reader.read_exact(&mut skip_buf)?;
        }

        // Read text data
        let mut result = LocaResource::new();
        for entry in entries {
            let text = self.read_text(entry.length)?;
            result.entries.push(LocalizedText {
                key: entry.key,
                version: entry.version,
                text,
            });
        }

        Ok(result)
    }

    fn read_header(&mut self) -> Result<LocaHeader> {
        let mut buf = [0u8; HEADER_SIZE];
        self.reader.read_exact(&mut buf)?;

        Ok(LocaHeader {
            signature: u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]),
            num_entries: u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]),
            texts_offset: u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]),
        })
    }

    fn read_entries(&mut self, count: usize) -> Result<Vec<LocaEntry>> {
        let mut entries = Vec::with_capacity(count);

        for _ in 0..count {
            let mut key_buf = [0u8; MAX_KEY_SIZE];
            self.reader.read_exact(&mut key_buf)?;

            // Find the null terminator
            let key_len = key_buf.iter().position(|&b| b == 0).unwrap_or(MAX_KEY_SIZE);
            let key = std::str::from_utf8(&key_buf[..key_len])?.to_string();

            let mut version_buf = [0u8; 2];
            self.reader.read_exact(&mut version_buf)?;
            let version = u16::from_le_bytes(version_buf);

            let mut length_buf = [0u8; 4];
            self.reader.read_exact(&mut length_buf)?;
            let length = u32::from_le_bytes(length_buf);

            entries.push(LocaEntry {
                key,
                version,
                length,
            });
        }

        Ok(entries)
    }

    fn read_text(&mut self, length: u32) -> Result<String> {
        if length == 0 {
            return Ok(String::new());
        }

        // Length includes null terminator
        let mut buf = vec![0u8; length as usize];
        self.reader.read_exact(&mut buf)?;

        // Remove the null terminator
        if let Some(last) = buf.last() {
            if *last == 0 {
                buf.pop();
            }
        }

        Ok(String::from_utf8(buf)?)
    }
}

/// Reader for XML localization files.
pub struct LocaXmlReader<R> {
    reader: R,
}

impl<R: Read> LocaXmlReader<R> {
    /// Creates a new XML localization reader.
    pub fn new(reader: R) -> Self {
        Self { reader }
    }

    /// Reads the localization resource from the XML stream.
    pub fn read(&mut self) -> Result<LocaResource> {
        let mut content = String::new();
        self.reader.read_to_string(&mut content)?;

        let mut resource = LocaResource::new();

        // Simple XML parsing - find all <content> elements
        // Format: <content contentuid="key" version="1">text</content>
        let mut pos = 0;
        while let Some(start) = content[pos..].find("<content ") {
            let abs_start = pos + start;

            // Find the end of this element
            let Some(end_tag) = content[abs_start..].find("</content>") else {
                // Try self-closing tag
                if let Some(self_close) = content[abs_start..].find("/>") {
                    pos = abs_start + self_close + 2;
                    continue;
                }
                break;
            };

            let element = &content[abs_start..abs_start + end_tag + 10];

            // Parse contentuid attribute
            let contentuid = Self::extract_attribute(element, "contentuid");

            // Parse version attribute (default to 1)
            let version = Self::extract_attribute(element, "version")
                .and_then(|v| v.parse::<u16>().ok())
                .unwrap_or(1);

            // Extract text content
            let text = if let Some(tag_end) = element.find('>') {
                let inner_start = tag_end + 1;
                if let Some(inner_end) = element[inner_start..].find("</content>") {
                    Self::decode_xml_entities(&element[inner_start..inner_start + inner_end])
                } else {
                    String::new()
                }
            } else {
                String::new()
            };

            if let Some(key) = contentuid {
                resource.entries.push(LocalizedText {
                    key,
                    version,
                    text,
                });
            }

            pos = abs_start + end_tag + 10;
        }

        Ok(resource)
    }

    fn extract_attribute(element: &str, name: &str) -> Option<String> {
        let pattern = format!("{}=\"", name);
        let start = element.find(&pattern)? + pattern.len();
        let end = element[start..].find('"')? + start;
        Some(element[start..end].to_string())
    }

    fn decode_xml_entities(text: &str) -> String {
        text.replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&")
            .replace("&apos;", "'")
            .replace("&quot;", "\"")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn create_test_loca() -> Vec<u8> {
        let mut data = Vec::new();

        // Header
        data.extend_from_slice(&LOCA_SIGNATURE.to_le_bytes()); // Signature
        data.extend_from_slice(&1u32.to_le_bytes()); // NumEntries
        data.extend_from_slice(&82u32.to_le_bytes()); // TextsOffset (12 + 70 = 82)

        // Entry
        let mut key = [0u8; 64];
        let key_str = b"test_key";
        key[..key_str.len()].copy_from_slice(key_str);
        data.extend_from_slice(&key);
        data.extend_from_slice(&1u16.to_le_bytes()); // Version
        data.extend_from_slice(&12u32.to_le_bytes()); // Length (including null)

        // Text data
        data.extend_from_slice(b"Hello World\0");

        data
    }

    #[test]
    fn test_read_loca() {
        let data = create_test_loca();
        let cursor = Cursor::new(data);
        let mut reader = LocaReader::new(cursor);

        let resource = reader.read().unwrap();
        assert_eq!(resource.len(), 1);
        assert_eq!(resource.entries[0].key, "test_key");
        assert_eq!(resource.entries[0].version, 1);
        assert_eq!(resource.entries[0].text, "Hello World");
    }

    #[test]
    fn test_read_xml() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<contentList>
    <content contentuid="key1" version="1">Hello World</content>
    <content contentuid="key2" version="2">Test &amp; More</content>
</contentList>"#;

        let cursor = Cursor::new(xml.as_bytes());
        let mut reader = LocaXmlReader::new(cursor);

        let resource = reader.read().unwrap();
        assert_eq!(resource.len(), 2);
        assert_eq!(resource.entries[0].key, "key1");
        assert_eq!(resource.entries[0].text, "Hello World");
        assert_eq!(resource.entries[1].key, "key2");
        assert_eq!(resource.entries[1].text, "Test & More");
    }

    #[test]
    fn test_invalid_signature() {
        let mut data = create_test_loca();
        data[0] = 0xFF; // Corrupt signature

        let cursor = Cursor::new(data);
        let mut reader = LocaReader::new(cursor);

        let result = reader.read();
        assert!(matches!(result, Err(LocaError::InvalidSignature { .. })));
    }
}
