# bg3rustpaklib

A Rust library for reading, extracting, and creating Baldur's Gate 3 PAK files.

## Overview

This library provides functionality for working with Larian Studios' PAK (LSPK) archive format used in Baldur's Gate 3. It is similar to [lslib](https://github.com/Norbyte/lslib) but written in Rust.

## Features

- **Read PAK files** - Supports versions 15, 16, and 18 (BG3 format)
- **List and search files** - Find files by pattern or search within packages
- **Extract files** - Extract individual files or entire packages to disk
- **Create PAK files** - Build new packages from directories
- **Compression support** - LZ4 and Zstd decompression
- **Solid archives** - Support for solid archive handling
- **Async API** - Optional async support via `async` feature

## Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
bg3rustpaklib = "0.1"
```

### Basic Example

```rust
use bg3rustpaklib::{Package, PackageSearch};

fn main() -> bg3rustpaklib::Result<()> {
    // Open a PAK file
    let package = Package::open("Game.pak")?;

    // Get package metadata
    let metadata = package.metadata();
    println!("Package version: {}", metadata.version);
    println!("File count: {}", metadata.file_count);

    // List all files
    for file in package.files() {
        println!("{} ({} bytes)", file.name(), file.size());
    }

    // Find files by pattern
    let lsf_files = package.find("**/*.lsf");
    println!("Found {} .lsf files", lsf_files.len());

    // Extract a specific file
    if let Some(file) = package.get("Public/Game/GUI/Assets/Tooltips/tooltip.lsf") {
        let contents = package.read_file(file)?;
        println!("File size: {} bytes", contents.len());
    }

    // Extract all files to a directory
    package.extract_all("output/")?;

    Ok(())
}
```

### Async Support

Enable the `async` feature for async APIs:

```toml
[dependencies]
bg3rustpaklib = { version = "0.1", features = ["async"] }
```

```rust
use bg3rustpaklib::AsyncPackage;

#[tokio::main]
async fn main() -> bg3rustpaklib::Result<()> {
    let package = AsyncPackage::open("Game.pak").await?;
    let metadata = package.metadata().await;
    println!("File count: {}", metadata.file_count);
    Ok(())
}
```

## Command-line Tools

Several example tools are included:

### dump_header

Dumps hex view of the first and last 64 bytes of a PAK file:

```bash
cargo run --example dump_header path/to/file.pak
```

### detailed_dump

Shows detailed package information including file list:

```bash
cargo run --example detailed_dump path/to/file.pak
```

### compare_paks

Compares multiple PAK files and shows their metadata:

```bash
cargo run --example compare_paks path/to/pak1.pak path/to/pak2.pak
```

### repack_test

Demonstrates extracting and repacking a PAK file:

```bash
cargo run --example repack_test input.pak output.pak
```

## Building

```bash
# Build the library
cargo build

# Build with async support
cargo build --features async

# Run tests
cargo test

# Build examples
cargo build --examples
```

## API Reference

See [docs.rs](https://docs.rs/bg3rustpaklib) for full API documentation.

## License

MIT License - see LICENSE file for details.

## Credits

Inspired by [lslib](https://github.com/Norbyte/lslib) by Norbyte.
