//! Integration tests against real BG3 mod PAK files.
//!
//! PAK files live in tests/paks/ and are NOT committed to git.
//! Tests are skipped automatically if the files are missing.

use bg3rustpaklib::{FileFilter, Package, PackageSearch, PackageVersion};

// ── helpers ──────────────────────────────────────────────────────────────────

const ACHERON: &str = concat!(
    "tests/paks/new/Acheron - Clothing and Light Armor-21776-1-0-0-1-1773421378/",
    "ONYX_Acheron_203a60f0-f5d9-6643-2e3e-ca68c44e2b2f.pak"
);
const ROMANCE: &str = concat!(
    "tests/paks/new/Even Better Romance - Straight-21826-0-2-0-1773097318/",
    "Even Better Romance - S.pak"
);
const MINTHARA: &str = concat!(
    "tests/paks/new/Minthara Auto-KO - Full Auto-KO-21836-1-0-1-2-1774418866/",
    "Minthara_Auto-KO_Full_Auto.pak"
);
const MINTHARA_MOD_PREFIX: &str =
    "Mods/Minthara_Recruitment_AutoKnockout_95e17e85-53a9-e747-68eb-26f0a0e222f2";

/// Returns Some(Package) if the file exists, None to skip the test.
fn open_if_present(path: &str) -> Option<Package> {
    if !std::path::Path::new(path).exists() {
        eprintln!("SKIP: {} not found", path);
        return None;
    }
    Some(Package::open(path).expect("failed to open PAK"))
}

// ── version / metadata ───────────────────────────────────────────────────────

#[test]
fn test_acheron_metadata() {
    let Some(pkg) = open_if_present(ACHERON) else { return };
    let m = pkg.metadata();
    assert_eq!(m.version, PackageVersion::V18);
    assert_eq!(m.file_count, 739);
    assert!(!m.is_solid);
    assert!(m.total_size > 0);
    assert!(m.total_compressed_size > 0);
    assert!(m.total_compressed_size < m.total_size, "compressed should be smaller");
}

#[test]
fn test_romance_metadata() {
    let Some(pkg) = open_if_present(ROMANCE) else { return };
    let m = pkg.metadata();
    assert_eq!(m.version, PackageVersion::V18);
    assert_eq!(m.file_count, 135);
    assert!(!m.is_solid);
}

#[test]
fn test_minthara_metadata() {
    let Some(pkg) = open_if_present(MINTHARA) else { return };
    let m = pkg.metadata();
    assert_eq!(m.version, PackageVersion::V18);
    assert_eq!(m.file_count, 10);
    assert!(!m.is_solid);
}

// ── get() — O(1) exact path lookup ───────────────────────────────────────────

#[test]
fn test_get_meta_lsx_acheron() {
    let Some(pkg) = open_if_present(ACHERON) else { return };
    let file = pkg.get("Mods/ONYX_Acheron_203a60f0-f5d9-6643-2e3e-ca68c44e2b2f/meta.lsx")
        .expect("meta.lsx not found");
    assert_eq!(file.size(), 4849);
    assert!(file.is_compressed());
}

#[test]
fn test_get_meta_lsx_romance() {
    let Some(pkg) = open_if_present(ROMANCE) else { return };
    let file = pkg.get("Mods/BetterRomance_93f26b26-a531-f099-58c8-4a23d2acaf9c/meta.lsx")
        .expect("meta.lsx not found");
    assert_eq!(file.size(), 5092);
}

#[test]
fn test_get_nonexistent_returns_none() {
    let Some(pkg) = open_if_present(ACHERON) else { return };
    assert!(pkg.get("this/does/not/exist.lsf").is_none());
}

// ── read_file — decompression correctness ────────────────────────────────────

#[test]
fn test_read_meta_lsx_is_valid_xml() {
    let Some(pkg) = open_if_present(ACHERON) else { return };
    let file = pkg.get("Mods/ONYX_Acheron_203a60f0-f5d9-6643-2e3e-ca68c44e2b2f/meta.lsx").unwrap();
    let data = pkg.read_file(file).unwrap();
    assert_eq!(data.len(), 4849);
    // meta.lsx is UTF-8 XML — must start with <?xml or <save
    let start = std::str::from_utf8(&data[..5]).unwrap();
    assert!(start.starts_with("<?xml") || start.starts_with("<save"),
        "unexpected start: {:?}", start);
}

#[test]
fn test_read_english_xml_minthara() {
    let Some(pkg) = open_if_present(MINTHARA) else { return };
    let file = pkg
        .get(&format!("{MINTHARA_MOD_PREFIX}/Localization/English/english.xml"))
        .unwrap();
    let data = pkg.read_file(file).unwrap();
    let text = std::str::from_utf8(&data).expect("english.xml should be valid UTF-8");
    assert!(text.contains("Minthara") || text.contains("minthara") || text.contains("contentuid"),
        "unexpected content: {}", &text[..200.min(text.len())]);
}

#[test]
fn test_read_story_header_is_nonempty() {
    let Some(pkg) = open_if_present(MINTHARA) else { return };
    let file = pkg
        .get(&format!("{MINTHARA_MOD_PREFIX}/Story/RawFiles/story_header.div"))
        .unwrap();
    let data = pkg.read_file(file).unwrap();
    assert_eq!(data.len(), 127964);
}

#[test]
fn test_read_logo_png_has_png_magic() {
    let Some(pkg) = open_if_present(MINTHARA) else { return };
    let file = pkg
        .get(&format!("{MINTHARA_MOD_PREFIX}/mod_publish_logo.png"))
        .unwrap();
    let data = pkg.read_file(file).unwrap();
    assert_eq!(data.len(), 239249);
    // PNG magic: 89 50 4E 47 0D 0A 1A 0A
    assert_eq!(&data[..8], &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A],
        "logo.png does not start with PNG magic");
}

#[test]
fn test_all_files_readable_minthara() {
    let Some(pkg) = open_if_present(MINTHARA) else { return };
    for file in pkg.files() {
        if file.is_deleted() { continue; }
        let data = pkg.read_file(file)
            .unwrap_or_else(|e| panic!("failed to read {}: {}", file.name(), e));
        assert!(!data.is_empty() || file.size() == 0,
            "read returned empty data for {}", file.name());
    }
}

#[test]
fn test_all_files_readable_romance() {
    let Some(pkg) = open_if_present(ROMANCE) else { return };
    for file in pkg.files() {
        if file.is_deleted() { continue; }
        pkg.read_file(file)
            .unwrap_or_else(|e| panic!("failed to read {}: {}", file.name(), e));
    }
}

// ── glob search with ** ───────────────────────────────────────────────────────

#[test]
fn test_find_double_star_lsf_romance() {
    let Some(pkg) = open_if_present(ROMANCE) else { return };
    let lsf_files = pkg.find("**/*.lsf");
    assert!(!lsf_files.is_empty(), "expected .lsf files");
    for f in &lsf_files {
        assert!(f.name().ends_with(".lsf"), "unexpected file: {}", f.name());
    }
}

#[test]
fn test_find_mods_subtree_double_star() {
    let Some(pkg) = open_if_present(MINTHARA) else { return };
    let mods_files = pkg.find("Mods/**");
    assert!(!mods_files.is_empty());
    for f in &mods_files {
        assert!(f.name().starts_with("Mods/"), "unexpected: {}", f.name());
    }
}

#[test]
fn test_find_flags_lsf_romance() {
    let Some(pkg) = open_if_present(ROMANCE) else { return };
    let flags = pkg.find("Public/**/Flags/*.lsf");
    // Romance pak has many flag .lsf files
    assert!(flags.len() >= 10, "expected at least 10 flag .lsf files, got {}", flags.len());
}

#[test]
fn test_find_no_match_returns_empty() {
    let Some(pkg) = open_if_present(ACHERON) else { return };
    let result = pkg.find("**/*.nonexistent_extension_xyz");
    assert!(result.is_empty());
}

// ── find_by_extension ────────────────────────────────────────────────────────

#[test]
fn test_find_by_extension_gr2() {
    let Some(pkg) = open_if_present(ACHERON) else { return };
    let gr2_files = pkg.find_by_extension("gr2");
    assert!(!gr2_files.is_empty(), "expected .GR2 files in Acheron pak");
    for f in &gr2_files {
        assert!(f.name().to_lowercase().ends_with(".gr2"));
    }
}

#[test]
fn test_find_by_extension_lsx() {
    let Some(pkg) = open_if_present(MINTHARA) else { return };
    let lsx = pkg.find_by_extension("lsx");
    assert_eq!(lsx.len(), 1); // only meta.lsx
    assert!(lsx[0].name().ends_with("meta.lsx"));
}

// ── find_containing ──────────────────────────────────────────────────────────

#[test]
fn test_find_containing_mods() {
    let Some(pkg) = open_if_present(MINTHARA) else { return };
    let mods = pkg.find_containing("mods");
    assert!(!mods.is_empty());
    for f in &mods {
        assert!(f.name().to_lowercase().contains("mods"));
    }
}

#[test]
fn test_find_containing_case_insensitive() {
    let Some(pkg) = open_if_present(ACHERON) else { return };
    let lower = pkg.find_containing("onyx_acheron");
    let upper = pkg.find_containing("ONYX_ACHERON");
    assert_eq!(lower.len(), upper.len(), "find_containing should be case-insensitive");
    assert!(!lower.is_empty());
}

// ── FileFilter ───────────────────────────────────────────────────────────────

#[test]
fn test_file_filter_extension_lsf_romance() {
    let Some(pkg) = open_if_present(ROMANCE) else { return };
    let filter = FileFilter::new().extension("lsf");
    let matched: Vec<_> = pkg.files().iter().filter(|f| filter.matches(f)).collect();
    assert!(!matched.is_empty());
    for f in &matched {
        assert!(f.name().to_lowercase().ends_with(".lsf"));
    }
}

#[test]
fn test_file_filter_directory_public() {
    let Some(pkg) = open_if_present(ROMANCE) else { return };
    let filter = FileFilter::new()
        .directory("Public/BetterRomance_93f26b26-a531-f099-58c8-4a23d2acaf9c/Flags");
    let matched: Vec<_> = pkg.files().iter().filter(|f| filter.matches(f)).collect();
    assert!(!matched.is_empty());
    for f in &matched {
        assert!(f.name().contains("Flags/"), "unexpected: {}", f.name());
    }
}

#[test]
fn test_file_filter_exclude_png() {
    let Some(pkg) = open_if_present(ACHERON) else { return };
    let filter = FileFilter::new().exclude("**/*.png");
    for f in pkg.files() {
        if filter.matches(f) {
            assert!(!f.name().to_lowercase().ends_with(".png"),
                "filter should have excluded {}", f.name());
        }
    }
}

// ── extract_all ──────────────────────────────────────────────────────────────

#[test]
fn test_extract_all_minthara() {
    let Some(pkg) = open_if_present(MINTHARA) else { return };
    let temp = tempfile::tempdir().unwrap();
    pkg.extract_all(temp.path()).unwrap();

    // Verify every non-deleted file was extracted
    for file in pkg.files() {
        if file.is_deleted() { continue; }
        let out = temp.path().join(file.name());
        assert!(out.exists(), "missing extracted file: {}", file.name());
        let on_disk = std::fs::read(&out).unwrap();
        assert_eq!(on_disk.len() as u64, file.size(),
            "size mismatch for {}", file.name());
    }
}

#[test]
fn test_extract_filtered_lsf_only() {
    let Some(pkg) = open_if_present(ROMANCE) else { return };
    let temp = tempfile::tempdir().unwrap();
    let filter = FileFilter::new().extension("lsf");
    pkg.extract_filtered(temp.path(), |f| filter.matches(f)).unwrap();

    // Only .lsf files should exist
    for entry in walkdir(temp.path()) {
        let name = entry.file_name().to_string_lossy().to_lowercase();
        assert!(name.ends_with(".lsf"), "non-.lsf file extracted: {}", name);
    }
}

/// Recursively collect all files under `dir`.
fn walkdir(dir: &std::path::Path) -> Vec<std::fs::DirEntry> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            if e.path().is_dir() {
                out.extend(walkdir(&e.path()));
            } else {
                out.push(e);
            }
        }
    }
    out
}
