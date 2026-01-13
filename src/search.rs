//! Search and filtering utilities for package contents.

use crate::package::{Package, PackagedFile};
use glob::Pattern;
use regex::Regex;

/// Extension trait for searching within packages.
pub trait PackageSearch {
    /// Finds files matching a glob pattern.
    ///
    /// The pattern uses standard glob syntax:
    /// - `*` matches any sequence of characters except `/`
    /// - `**` matches any sequence of characters including `/`
    /// - `?` matches any single character
    /// - `[abc]` matches any character in the brackets
    ///
    /// # Example
    /// ```ignore
    /// let matches = package.find("Public/**/*.lsf");
    /// ```
    fn find(&self, pattern: &str) -> Vec<&PackagedFile>;

    /// Finds files matching a regular expression.
    ///
    /// The regex is matched against the full file path.
    ///
    /// # Example
    /// ```ignore
    /// let regex = Regex::new(r"\.lsf$").unwrap();
    /// let matches = package.find_regex(&regex);
    /// ```
    fn find_regex(&self, regex: &Regex) -> Vec<&PackagedFile>;

    /// Finds files with names containing the given substring (case-insensitive).
    ///
    /// # Example
    /// ```ignore
    /// let matches = package.find_containing("character");
    /// ```
    fn find_containing(&self, substring: &str) -> Vec<&PackagedFile>;

    /// Finds files with the given extension.
    ///
    /// The extension should not include the leading dot.
    ///
    /// # Example
    /// ```ignore
    /// let matches = package.find_by_extension("lsf");
    /// ```
    fn find_by_extension(&self, extension: &str) -> Vec<&PackagedFile>;

    /// Finds files in the given directory (including subdirectories).
    ///
    /// # Example
    /// ```ignore
    /// let matches = package.find_in_directory("Public/Game/GUI");
    /// ```
    fn find_in_directory(&self, directory: &str) -> Vec<&PackagedFile>;
}

impl PackageSearch for Package {
    fn find(&self, pattern: &str) -> Vec<&PackagedFile> {
        // Normalize the pattern to use forward slashes
        let pattern = pattern.replace('\\', "/");

        // Try to compile the glob pattern
        let glob_pattern = match Pattern::new(&pattern) {
            Ok(p) => p,
            Err(_) => return Vec::new(),
        };

        self.files()
            .iter()
            .filter(|file| {
                let name = file.name().replace('\\', "/");
                glob_pattern.matches(&name)
            })
            .collect()
    }

    fn find_regex(&self, regex: &Regex) -> Vec<&PackagedFile> {
        self.files()
            .iter()
            .filter(|file| regex.is_match(file.name()))
            .collect()
    }

    fn find_containing(&self, substring: &str) -> Vec<&PackagedFile> {
        let substring_lower = substring.to_lowercase();

        self.files()
            .iter()
            .filter(|file| file.name().to_lowercase().contains(&substring_lower))
            .collect()
    }

    fn find_by_extension(&self, extension: &str) -> Vec<&PackagedFile> {
        let ext_lower = extension.to_lowercase();
        let with_dot = format!(".{}", ext_lower);

        self.files()
            .iter()
            .filter(|file| {
                file.name()
                    .to_lowercase()
                    .ends_with(&with_dot)
            })
            .collect()
    }

    fn find_in_directory(&self, directory: &str) -> Vec<&PackagedFile> {
        // Normalize directory path
        let dir = directory.replace('\\', "/").trim_end_matches('/').to_string();
        let prefix = format!("{}/", dir);

        self.files()
            .iter()
            .filter(|file| {
                let name = file.name().replace('\\', "/");
                name.starts_with(&prefix) || name == dir
            })
            .collect()
    }
}

/// A file filter that can be applied during extraction.
pub struct FileFilter {
    patterns: Vec<Pattern>,
    extensions: Vec<String>,
    directories: Vec<String>,
    exclude_patterns: Vec<Pattern>,
}

impl FileFilter {
    /// Creates a new empty filter that matches all files.
    pub fn new() -> Self {
        FileFilter {
            patterns: Vec::new(),
            extensions: Vec::new(),
            directories: Vec::new(),
            exclude_patterns: Vec::new(),
        }
    }

    /// Adds a glob pattern to match.
    pub fn pattern(mut self, pattern: &str) -> Self {
        if let Ok(p) = Pattern::new(&pattern.replace('\\', "/")) {
            self.patterns.push(p);
        }
        self
    }

    /// Adds an extension to match (without the leading dot).
    pub fn extension(mut self, ext: &str) -> Self {
        self.extensions.push(ext.to_lowercase());
        self
    }

    /// Adds a directory to match.
    pub fn directory(mut self, dir: &str) -> Self {
        self.directories.push(dir.replace('\\', "/").trim_end_matches('/').to_string());
        self
    }

    /// Adds a glob pattern to exclude.
    pub fn exclude(mut self, pattern: &str) -> Self {
        if let Ok(p) = Pattern::new(&pattern.replace('\\', "/")) {
            self.exclude_patterns.push(p);
        }
        self
    }

    /// Tests whether a file matches this filter.
    pub fn matches(&self, file: &PackagedFile) -> bool {
        let name = file.name().replace('\\', "/");
        let name_lower = name.to_lowercase();

        // Check exclusions first
        for pattern in &self.exclude_patterns {
            if pattern.matches(&name) {
                return false;
            }
        }

        // If no positive filters are set, match everything
        if self.patterns.is_empty() && self.extensions.is_empty() && self.directories.is_empty() {
            return true;
        }

        // Check patterns
        for pattern in &self.patterns {
            if pattern.matches(&name) {
                return true;
            }
        }

        // Check extensions
        for ext in &self.extensions {
            let with_dot = format!(".{}", ext);
            if name_lower.ends_with(&with_dot) {
                return true;
            }
        }

        // Check directories
        for dir in &self.directories {
            let prefix = format!("{}/", dir);
            if name.starts_with(&prefix) || name == *dir {
                return true;
            }
        }

        false
    }
}

impl Default for FileFilter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_file(name: &str) -> PackagedFile {
        use crate::package::file_entry::FileEntry;
        use std::path::PathBuf;

        PackagedFile {
            entry: FileEntry {
                name: name.to_string(),
                archive_part: 0,
                offset: 0,
                size_on_disk: 100,
                uncompressed_size: 100,
                compression_flags: 0,
                crc: 0,
            },
            package_path: PathBuf::new(),
            legacy_data_offset: 0,
            is_solid: false,
            solid_offset: 0,
        }
    }

    #[test]
    fn test_file_filter_extension() {
        let filter = FileFilter::new().extension("lsf");

        let file1 = make_test_file("Public/Game/test.lsf");
        let file2 = make_test_file("Public/Game/test.txt");

        assert!(filter.matches(&file1));
        assert!(!filter.matches(&file2));
    }

    #[test]
    fn test_file_filter_directory() {
        let filter = FileFilter::new().directory("Public/Game");

        let file1 = make_test_file("Public/Game/test.lsf");
        let file2 = make_test_file("Public/Other/test.lsf");

        assert!(filter.matches(&file1));
        assert!(!filter.matches(&file2));
    }

    #[test]
    fn test_file_filter_exclude() {
        let filter = FileFilter::new()
            .extension("lsf")
            .exclude("**/test_*");

        let file1 = make_test_file("Public/Game/data.lsf");
        let file2 = make_test_file("Public/Game/test_data.lsf");

        assert!(filter.matches(&file1));
        assert!(!filter.matches(&file2));
    }
}
