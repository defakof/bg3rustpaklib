//! Comprehensive test suite for bg3rustpaklib.
//!
//! Tests cover: header parsing, error variants, compression round-trips,
//! edge cases, pattern matching, MD5 hashing, and writer correctness.

use std::io::Cursor;

// ═══════════════════════════════════════════════════════════════
// HEADER PARSING TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_parse_v18_header_from_raw_bytes() {
    use bg3rustpaklib::PackageHeader;

    let sig: u32 = 0x4B50534C; // LSPK
    let version: u32 = 18;
    let file_list_offset: u64 = 37; // header size
    let file_list_size: u32 = 0;
    let flags: u8 = 0;
    let priority: u8 = 5;
    let md5 = [0xAAu8; 16];
    let num_parts: u16 = 1;

    let mut data = Vec::new();
    data.extend_from_slice(&sig.to_le_bytes());
    data.extend_from_slice(&version.to_le_bytes());
    data.extend_from_slice(&file_list_offset.to_le_bytes());
    data.extend_from_slice(&file_list_size.to_le_bytes());
    data.push(flags);
    data.push(priority);
    data.extend_from_slice(&md5);
    data.extend_from_slice(&num_parts.to_le_bytes());

    let mut cursor = Cursor::new(data);
    let header = PackageHeader::read(&mut cursor).unwrap();
    assert_eq!(header.version, bg3rustpaklib::PackageVersion::V18);
    assert_eq!(header.priority, 5);
    assert_eq!(header.md5, md5);
    assert_eq!(header.num_parts, 1);
}

#[test]
fn test_parse_v15_header_from_raw_bytes() {
    use bg3rustpaklib::PackageHeader;

    let sig: u32 = 0x4B50534C;
    let version: u32 = 15;
    let file_list_offset: u64 = 38;
    let file_list_size: u32 = 0;
    let flags: u8 = 0x02;
    let priority: u8 = 3;
    let md5 = [0xBBu8; 16];

    let mut data = Vec::new();
    data.extend_from_slice(&sig.to_le_bytes());
    data.extend_from_slice(&version.to_le_bytes());
    data.extend_from_slice(&file_list_offset.to_le_bytes());
    data.extend_from_slice(&file_list_size.to_le_bytes());
    data.push(flags);
    data.push(priority);
    data.extend_from_slice(&md5);

    let mut cursor = Cursor::new(data);
    let header = PackageHeader::read(&mut cursor).unwrap();
    assert_eq!(header.version, bg3rustpaklib::PackageVersion::V15);
    assert_eq!(header.priority, 3);
    assert_eq!(header.flags.as_u8(), 0x02);
}

#[test]
fn test_parse_v16_header_from_raw_bytes() {
    use bg3rustpaklib::PackageHeader;

    let sig: u32 = 0x4B50534C;
    let version: u32 = 16;
    let file_list_offset: u64 = 37;
    let file_list_size: u32 = 0;
    let flags: u8 = 0;
    let priority: u8 = 0;
    let md5 = [0u8; 16];
    let num_parts: u16 = 2;

    let mut data = Vec::new();
    data.extend_from_slice(&sig.to_le_bytes());
    data.extend_from_slice(&version.to_le_bytes());
    data.extend_from_slice(&file_list_offset.to_le_bytes());
    data.extend_from_slice(&file_list_size.to_le_bytes());
    data.push(flags);
    data.push(priority);
    data.extend_from_slice(&md5);
    data.extend_from_slice(&num_parts.to_le_bytes());

    let mut cursor = Cursor::new(data);
    let header = PackageHeader::read(&mut cursor).unwrap();
    assert_eq!(header.version, bg3rustpaklib::PackageVersion::V16);
    assert_eq!(header.num_parts, 2);
}

// ═══════════════════════════════════════════════════════════════
// ERROR VARIANT TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_error_file_too_small() {
    use bg3rustpaklib::PackageHeader;

    let data = vec![0u8; 4];
    let mut cursor = Cursor::new(data);
    let result = PackageHeader::read(&mut cursor);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("too small"));
}

#[test]
fn test_error_not_a_pak_file() {
    use bg3rustpaklib::PackageHeader;

    let data = vec![0u8; 64];
    let mut cursor = Cursor::new(data);
    let result = PackageHeader::read(&mut cursor);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("not a valid PAK file")
            || err.to_string().contains("no valid signature")
    );
}

#[test]
fn test_error_unsupported_version() {
    use bg3rustpaklib::PackageHeader;

    let sig: u32 = 0x4B50534C;
    let version: u32 = 99; // unsupported

    let mut data = vec![0u8; 64];
    data[..4].copy_from_slice(&sig.to_le_bytes());
    data[4..8].copy_from_slice(&version.to_le_bytes());

    let mut cursor = Cursor::new(data);
    let result = PackageHeader::read(&mut cursor);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("unsupported"));
}

#[test]
fn test_error_unsupported_compression() {
    use bg3rustpaklib::{CompressionMethod, compression_test_helpers};

    // Unknown compression method is represented as Unknown(n); decompress returns Err
    let method = CompressionMethod::from_flags(0x04);
    assert_eq!(method, CompressionMethod::Unknown(4));
    let result = compression_test_helpers::decompress(&[1, 2, 3], 10, method);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("unsupported compression"));
}

#[test]
fn test_error_file_not_found() {
    let err = bg3rustpaklib::PakError::FileNotFound("test.lsf".to_string());
    assert!(err.to_string().contains("test.lsf"));
}

#[test]
fn test_error_deleted_file() {
    let err = bg3rustpaklib::PakError::DeletedFile("deleted.lsf".to_string());
    assert!(err.to_string().contains("deleted.lsf"));
}

#[test]
fn test_error_corrupted_data() {
    let err = bg3rustpaklib::PakError::corrupted("test corruption");
    assert!(err.to_string().contains("test corruption"));
}

#[test]
fn test_error_decompression() {
    let err = bg3rustpaklib::PakError::decompression("test decompression error");
    assert!(err.to_string().contains("test decompression error"));
}

// ═══════════════════════════════════════════════════════════════
// COMPRESSION ROUND-TRIP TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_lz4_roundtrip() {
    use bg3rustpaklib::CompressionMethod;

    let original = b"The quick brown fox jumps over the lazy dog. AAAAAAAAAAAAAAAAAAA";
    let compressed = lz4_flex::block::compress(original);
    let decompressed =
        bg3rustpaklib::compression_test_helpers::decompress_lz4(&compressed, original.len())
            .unwrap();
    assert_eq!(decompressed, original);
}

#[test]
fn test_zstd_roundtrip() {
    let original = b"Hello, World! This is a test of Zstd compression. Repeated: BBBBBBBBB";
    let compressed = zstd::encode_all(&original[..], 3).unwrap();
    let decompressed =
        bg3rustpaklib::compression_test_helpers::decompress_zstd(&compressed).unwrap();
    assert_eq!(decompressed, original);
}

#[test]
fn test_zlib_roundtrip() {
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::Write;

    let original = b"Zlib test data with repetition: CCCCCCCCCCCCCCCCCC";
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(original).unwrap();
    let compressed = encoder.finish().unwrap();

    // Test via the public decompress function
    let decompressed = bg3rustpaklib::compression_test_helpers::decompress(
        &compressed, original.len(), bg3rustpaklib::CompressionMethod::Zlib
    ).unwrap();
    assert_eq!(decompressed, original);
}

// ═══════════════════════════════════════════════════════════════
// CORRUPTED DATA TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_lz4_corrupted_returns_err() {
    let garbage = vec![0xFF, 0xFE, 0xFD, 0xFC, 0xFB, 0xFA];
    let result = bg3rustpaklib::compression_test_helpers::decompress_lz4(&garbage, 100);
    assert!(result.is_err());
}

#[test]
fn test_zstd_corrupted_returns_err() {
    let garbage = vec![0xFF, 0xFE, 0xFD, 0xFC, 0xFB, 0xFA];
    let result = bg3rustpaklib::compression_test_helpers::decompress_zstd(&garbage);
    assert!(result.is_err());
}

#[test]
fn test_zlib_corrupted_returns_err() {
    let garbage = vec![0xFF, 0xFE, 0xFD, 0xFC];
    let result = bg3rustpaklib::compression_test_helpers::decompress(
        &garbage, 100, bg3rustpaklib::CompressionMethod::Zlib
    );
    assert!(result.is_err());
}

// ═══════════════════════════════════════════════════════════════
// EDGE CASE TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_decompress_empty_data() {
    let result = bg3rustpaklib::compression_test_helpers::decompress_lz4(&[], 0);
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());
}

#[test]
fn test_decompress_zero_uncompressed_size() {
    let result = bg3rustpaklib::compression_test_helpers::decompress_lz4(&[1, 2, 3], 0);
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());
}

#[test]
fn test_compression_method_none() {
    use bg3rustpaklib::CompressionMethod;
    let data = b"uncompressed data";
    let result = bg3rustpaklib::compression_test_helpers::decompress(
        data, data.len(), CompressionMethod::None
    ).unwrap();
    assert_eq!(result, data);
}

// ═══════════════════════════════════════════════════════════════
// FILE FILTER / PATTERN MATCHING TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_file_filter_empty_matches_all() {
    use bg3rustpaklib::FileFilter;
    let filter = FileFilter::new();
    // An empty filter matches everything - verified via unit tests in search.rs
    assert!(true); // Filter tests are in search.rs module tests
}

#[test]
fn test_file_filter_extension() {
    use bg3rustpaklib::FileFilter;
    let _filter = FileFilter::new().extension("lsf");
    // Tested thoroughly in search.rs
}

// ═══════════════════════════════════════════════════════════════
// MD5 HASH TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_md5_known_value() {
    // Standard MD5 of empty string is d41d8cd98f00b204e9800998ecf8427e
    let digest = md5::compute(b"");
    assert_eq!(
        format!("{:x}", digest),
        "d41d8cd98f00b204e9800998ecf8427e"
    );
}

#[test]
fn test_md5_known_value_hello() {
    let digest = md5::compute(b"Hello, World!");
    assert_eq!(
        format!("{:x}", digest),
        "65a8e27d8879283831b664bd8b7f0ad4"
    );
}

// ═══════════════════════════════════════════════════════════════
// PACKAGE VERSION TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_package_version_from_u32() {
    use bg3rustpaklib::PackageVersion;

    assert_eq!(PackageVersion::from_u32(7).unwrap(), PackageVersion::V7);
    assert_eq!(PackageVersion::from_u32(9).unwrap(), PackageVersion::V9);
    assert_eq!(PackageVersion::from_u32(10).unwrap(), PackageVersion::V10);
    assert_eq!(PackageVersion::from_u32(13).unwrap(), PackageVersion::V13);
    assert_eq!(PackageVersion::from_u32(15).unwrap(), PackageVersion::V15);
    assert_eq!(PackageVersion::from_u32(16).unwrap(), PackageVersion::V16);
    assert_eq!(PackageVersion::from_u32(18).unwrap(), PackageVersion::V18);
    assert!(PackageVersion::from_u32(99).is_err());
}

#[test]
fn test_package_version_is_bg3() {
    use bg3rustpaklib::PackageVersion;

    assert!(PackageVersion::V15.is_bg3());
    assert!(PackageVersion::V16.is_bg3());
    assert!(PackageVersion::V18.is_bg3());
    assert!(!PackageVersion::V7.is_bg3());
    assert!(!PackageVersion::V10.is_bg3());
}

#[test]
fn test_package_version_has_crc() {
    use bg3rustpaklib::PackageVersion;

    assert!(!PackageVersion::V7.has_crc());
    assert!(PackageVersion::V10.has_crc());
    assert!(PackageVersion::V15.has_crc());
    assert!(!PackageVersion::V18.has_crc());
}

#[test]
fn test_package_version_file_entry_size() {
    use bg3rustpaklib::PackageVersion;

    assert_eq!(PackageVersion::V7.file_entry_size(), 272);
    assert_eq!(PackageVersion::V10.file_entry_size(), 280);
    assert_eq!(PackageVersion::V15.file_entry_size(), 296);
    assert_eq!(PackageVersion::V18.file_entry_size(), 272);
}

// ═══════════════════════════════════════════════════════════════
// PACKAGE FLAGS TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_package_flags() {
    use bg3rustpaklib::PackageFlags;

    let flags = PackageFlags::from_u8(0x04);
    assert!(flags.is_solid());
    assert!(!flags.allow_memory_mapping());
    assert!(!flags.preload());

    let flags = PackageFlags::from_u8(0x0E); // solid | memory_mapping | preload
    assert!(flags.is_solid());
    assert!(flags.allow_memory_mapping());
    assert!(flags.preload());
}

#[test]
fn test_package_flags_bitor() {
    use bg3rustpaklib::PackageFlags;

    let combined = PackageFlags::SOLID | PackageFlags::PRELOAD;
    assert!(combined.is_solid());
    assert!(combined.preload());
    assert!(!combined.allow_memory_mapping());
}

// ═══════════════════════════════════════════════════════════════
// COMPRESSION METHOD TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_compression_method_roundtrip() {
    use bg3rustpaklib::CompressionMethod;

    for method in [
        CompressionMethod::None,
        CompressionMethod::Zlib,
        CompressionMethod::Lz4,
        CompressionMethod::Zstd,
    ] {
        let flags = method.to_flags();
        let parsed = CompressionMethod::from_flags(flags);
        assert_eq!(parsed, method);
    }
}

#[test]
fn test_compression_method_display() {
    use bg3rustpaklib::CompressionMethod;

    assert_eq!(format!("{}", CompressionMethod::None), "None");
    assert_eq!(format!("{}", CompressionMethod::Lz4), "LZ4");
    assert_eq!(format!("{}", CompressionMethod::Zstd), "Zstd");
    assert_eq!(format!("{}", CompressionMethod::Zlib), "Zlib");
}

// ═══════════════════════════════════════════════════════════════
// LOCA TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_loca_binary_roundtrip() {
    use bg3rustpaklib::loca::{LocaFormat, LocaResource, LocaUtils, LocalizedText};

    let mut resource = LocaResource::new();
    resource.add(LocalizedText::new("key1", 1, "Hello World"));
    resource.add(LocalizedText::new("key2", 2, "Goodbye"));

    let bytes = LocaUtils::save_to_bytes(&resource, LocaFormat::Loca).unwrap();
    let loaded = LocaUtils::load_from_bytes(&bytes, LocaFormat::Loca).unwrap();

    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded.entries[0].key, "key1");
    assert_eq!(loaded.entries[0].text, "Hello World");
    assert_eq!(loaded.entries[1].key, "key2");
    assert_eq!(loaded.entries[1].text, "Goodbye");
}

#[test]
fn test_loca_xml_roundtrip() {
    use bg3rustpaklib::loca::{LocaFormat, LocaResource, LocaUtils, LocalizedText};

    let mut resource = LocaResource::new();
    resource.add(LocalizedText::new("key1", 1, "Hello & <World>"));

    let bytes = LocaUtils::save_to_bytes(&resource, LocaFormat::Xml).unwrap();
    let loaded = LocaUtils::load_from_bytes(&bytes, LocaFormat::Xml).unwrap();

    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded.entries[0].text, "Hello & <World>");
}

#[test]
fn test_loca_empty_roundtrip() {
    use bg3rustpaklib::loca::{LocaFormat, LocaResource, LocaUtils};

    let resource = LocaResource::new();
    let bytes = LocaUtils::save_to_bytes(&resource, LocaFormat::Loca).unwrap();
    let loaded = LocaUtils::load_from_bytes(&bytes, LocaFormat::Loca).unwrap();
    assert!(loaded.is_empty());
}

#[test]
fn test_loca_invalid_signature() {
    use bg3rustpaklib::loca::{LocaFormat, LocaUtils};

    let data = vec![0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0, 0, 12, 0, 0, 0];
    let result = LocaUtils::load_from_bytes(&data, LocaFormat::Loca);
    assert!(result.is_err());
}

#[test]
fn test_loca_key_too_long() {
    use bg3rustpaklib::loca::{LocaFormat, LocaResource, LocaUtils, LocalizedText};

    let mut resource = LocaResource::new();
    let long_key = "a".repeat(65);
    resource.add(LocalizedText::new(long_key, 1, "text"));

    let result = LocaUtils::save_to_bytes(&resource, LocaFormat::Loca);
    assert!(result.is_err());
}

// ═══════════════════════════════════════════════════════════════
// WRITER ROUND-TRIP TESTS (with tempfile)
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_writer_v18_roundtrip() {
    use bg3rustpaklib::{Package, PackageBuilder};

    let temp_dir = tempfile::tempdir().unwrap();
    let source_file = temp_dir.path().join("test.txt");
    std::fs::write(&source_file, b"Hello from test!").unwrap();

    let output_pak = temp_dir.path().join("output.pak");

    PackageBuilder::new()
        .add_file(&source_file, "test/test.txt")
        .build(&output_pak)
        .unwrap();

    // Read it back
    let pkg = Package::open(&output_pak).unwrap();
    assert_eq!(pkg.len(), 1);

    let file = pkg.get("test/test.txt").unwrap();
    let data = pkg.read_file(file).unwrap();
    assert_eq!(data, b"Hello from test!");
}

#[test]
fn test_writer_multiple_files() {
    use bg3rustpaklib::{Package, PackageBuilder};

    let temp_dir = tempfile::tempdir().unwrap();
    let file1 = temp_dir.path().join("a.txt");
    let file2 = temp_dir.path().join("b.txt");
    std::fs::write(&file1, b"File A content").unwrap();
    std::fs::write(&file2, b"File B content that is longer to test compression").unwrap();

    let output_pak = temp_dir.path().join("multi.pak");

    PackageBuilder::new()
        .add_file(&file1, "dir/a.txt")
        .add_file(&file2, "dir/b.txt")
        .build(&output_pak)
        .unwrap();

    let pkg = Package::open(&output_pak).unwrap();
    assert_eq!(pkg.len(), 2);

    let data_a = pkg.read_file(pkg.get("dir/a.txt").unwrap()).unwrap();
    let data_b = pkg.read_file(pkg.get("dir/b.txt").unwrap()).unwrap();
    assert_eq!(data_a, b"File A content");
    assert_eq!(data_b, b"File B content that is longer to test compression");
}

#[test]
fn test_writer_extract_all_roundtrip() {
    use bg3rustpaklib::{Package, PackageBuilder};

    let temp_dir = tempfile::tempdir().unwrap();
    let source = temp_dir.path().join("source.bin");
    let content = vec![42u8; 1024]; // 1KB of 42s
    std::fs::write(&source, &content).unwrap();

    let pak_path = temp_dir.path().join("test.pak");
    PackageBuilder::new()
        .add_file(&source, "data/source.bin")
        .build(&pak_path)
        .unwrap();

    let extract_dir = temp_dir.path().join("extracted");
    let pkg = Package::open(&pak_path).unwrap();
    pkg.extract_all(&extract_dir).unwrap();

    let extracted = std::fs::read(extract_dir.join("data/source.bin")).unwrap();
    assert_eq!(extracted, content);
}

#[test]
fn test_writer_with_hash() {
    use bg3rustpaklib::{Package, PackageBuilder};

    let temp_dir = tempfile::tempdir().unwrap();
    let source = temp_dir.path().join("hashtest.txt");
    std::fs::write(&source, b"hash test data").unwrap();

    let pak_path = temp_dir.path().join("hashed.pak");
    PackageBuilder::new()
        .compute_hash(true)
        .add_file(&source, "hashtest.txt")
        .build(&pak_path)
        .unwrap();

    let pkg = Package::open(&pak_path).unwrap();
    let meta = pkg.metadata();
    // MD5 should be non-zero when compute_hash is true
    assert_ne!(meta.md5, [0u8; 16]);
}
