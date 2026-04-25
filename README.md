# bg3rustpaklib

A Rust library for reading, searching, extracting, and creating Baldur's Gate 3 PAK (`.pak` / LSPK) archives, plus LOCA localization helpers.

## Overview

`bg3rustpaklib` provides a native Rust implementation of common BG3 archive workflows (similar in scope to [lslib](https://github.com/Norbyte/lslib)):

- open and inspect package metadata
- enumerate/search files
- read/extract files
- create new packages from files/directories
- parse and write `.loca` / localization XML data

![bg3rustpaklib vs lslib](./lslib-comparison.svg)

## Features

- **PAK read support** for BG3 package versions **15, 16, and 18**
- **PAK write support** for versions **15, 16, and 18** via `PackageBuilder`
- **File search helpers** (glob by default, regex with feature flag)
- **Extraction APIs** for single files, filtered sets, or full archives
- **Compression support**: LZ4 and Zstd decompression (plus zlib support for formats that need it)
- **Solid archive support**
- **Async wrapper API** with the `async` feature
- **Optional C ABI exports** with the `ffi` feature
- **LOCA utilities** for binary `.loca` and XML localization conversion

## Installation

```toml
[dependencies]
bg3rustpaklib = "0.1.5"
```

### Optional features

```toml
[dependencies]
bg3rustpaklib = { version = "0.1.5", features = ["async", "regex", "ffi"] }
```

- `async`: enables `AsyncPackage` (Tokio-based async wrappers)
- `regex`: enables regex-based search APIs
- `ffi`: enables C-compatible exported functions

## Quick Start (read/search/extract)

```rust
use bg3rustpaklib::{Package, PackageSearch};

fn main() -> bg3rustpaklib::Result<()> {
    let package = Package::open("Game.pak")?;

    let metadata = package.metadata();
    println!("Version: {:?}", metadata.version);
    println!("Files: {}", metadata.file_count);

    for file in package.find("**/*.lsf") {
        println!("{} ({} bytes)", file.name(), file.size());
    }

    if let Some(file) = package.get("Public/Game/GUI/Assets/Tooltips/tooltip.lsf") {
        let bytes = package.read_file(file)?;
        println!("Read {} bytes", bytes.len());
    }

    package.extract_all("output/")?;
    Ok(())
}
```

## Building a PAK

```rust
use bg3rustpaklib::{CompressionMethod, PackageBuilder, PackageVersion};

fn main() -> bg3rustpaklib::Result<()> {
    PackageBuilder::new()
        .version(PackageVersion::V18)
        .compression(CompressionMethod::Lz4)
        .add_directory("./my_mod_files")?
        .build("./MyMod.pak")?;

    Ok(())
}
```

## Async Example

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

## LOCA Utilities

```rust
use bg3rustpaklib::loca::{LocaFormat, LocaUtils};

fn main() -> bg3rustpaklib::loca::Result<()> {
    let resource = LocaUtils::load("Localization/English/my_mod.loca")?;
    LocaUtils::save_with_format(&resource, "Localization/English/my_mod.xml", LocaFormat::Xml)?;
    Ok(())
}
```

## Included examples

Run with `cargo run --example <name> -- <args...>`:

- `dump_header` — print header bytes from a PAK
- `detailed_dump` — show metadata and file list details
- `compare_paks` — compare metadata from multiple PAKs
- `repack_test` — extract and repack to validate writer flow

## Build / Test

```bash
cargo build
cargo build --features async
cargo build --examples
cargo test
```

## FFI / C++ integration

The crate normally builds as an `rlib`. To build a static library for C/C++:

```bash
cargo rustc --release --features ffi --crate-type=staticlib
```

C header: `include/bg3rustpaklib.h`

## Documentation

- API docs: <https://docs.rs/bg3rustpaklib>
- Source: <https://github.com/defakof/bg3rustpaklib>

## License

MIT — see [LICENSE](./LICENSE).

## Credits

Inspired by [lslib](https://github.com/Norbyte/lslib) by Norbyte.
