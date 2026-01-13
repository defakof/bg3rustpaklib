use bg3rustpaklib::Package;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    
    if args.len() < 2 {
        println!("Usage: compare_paks <pak1> [pak2]");
        return;
    }

    for path in &args[1..] {
        println!("\n=== {} ===", path);
        match Package::open(path) {
            Ok(pkg) => {
                let meta = pkg.metadata();
                println!("Version: {:?}", meta.version);
                println!("File count: {}", meta.file_count);
                println!("Total size: {}", meta.total_size);
                println!("Compressed size: {}", meta.total_compressed_size);
                println!("Priority: {}", meta.priority);
                println!("Is solid: {}", meta.is_solid);
                println!("MD5: {:02x?}", meta.md5);
                
                println!("\nFirst 5 files:");
                for (i, file) in pkg.files().iter().take(5).enumerate() {
                    println!("  {}: {} (size: {}, compressed: {}, method: {:?})", 
                        i, file.name(), file.size(), file.compressed_size(), file.compression_method());
                }
            }
            Err(e) => {
                println!("Error: {}", e);
            }
        }
    }
}
