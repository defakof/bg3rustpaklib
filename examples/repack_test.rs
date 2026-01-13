use bg3rustpaklib::{get_package_flags, get_package_priority, Package, PackageBuilder};
use std::env;
use std::path::Path;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        println!("Usage: repack_test <input.pak> <output.pak>");
        return;
    }

    let input = &args[1];
    let output = &args[2];
    let temp_dir = std::env::temp_dir().join("repack_test");

    println!("Opening {}...", input);
    let pkg = Package::open(input).unwrap();
    let meta = pkg.metadata();
    println!("  Version: {:?}, {} files", meta.version, meta.file_count);

    let priority = get_package_priority(Path::new(input)).unwrap_or(meta.priority);
    println!("  Original priority: {}", priority);

    println!("Extracting to {:?}...", temp_dir);
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();
    pkg.extract_all(&temp_dir).unwrap();

    println!("Repacking to {}...", output);
    PackageBuilder::new()
        .priority(priority)
        .add_directory(&temp_dir)
        .unwrap()
        .build(output)
        .unwrap();

    println!("Verifying {}...", output);
    match Package::open(output) {
        Ok(pkg2) => {
            let meta2 = pkg2.metadata();
            println!("  Version: {:?}, {} files", meta2.version, meta2.file_count);
            println!("  Repacked priority: {}", meta2.priority);

            for file in pkg2.files() {
                println!("  - {} ({} bytes)", file.name(), file.size());
            }
            println!("SUCCESS!");
        }
        Err(e) => {
            println!("  ERROR: {}", e);
        }
    }

    let _ = std::fs::remove_dir_all(&temp_dir);
}
