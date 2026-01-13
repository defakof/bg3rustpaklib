use std::env;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Usage: detailed_dump <pak>");
        return;
    }

    for path in &args[1..] {
        println!("\n============================================================");
        println!("=== {} ===", path);
        let mut file = File::open(path).unwrap();
        let file_len = file.metadata().unwrap().len();
        
        // Read header
        let mut header = [0u8; 40];
        file.read_exact(&mut header).unwrap();
        
        let sig = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        let version = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
        let file_list_offset = u64::from_le_bytes([
            header[8], header[9], header[10], header[11],
            header[12], header[13], header[14], header[15]
        ]);
        let file_list_size = u32::from_le_bytes([header[16], header[17], header[18], header[19]]);
        let flags = header[20];
        let priority = header[21];
        let num_parts = u16::from_le_bytes([header[38], header[39]]);
        
        println!("Signature: 0x{:08X} ({})", sig, 
            if sig == 0x4B50534C { "LSPK" } else { "???" });
        println!("Version: {}", version);
        println!("File list offset: {} (0x{:X})", file_list_offset, file_list_offset);
        println!("File list size: {} (0x{:X})", file_list_size, file_list_size);
        println!("Flags: 0x{:02X}", flags);
        println!("Priority: {}", priority);
        println!("Num parts: {}", num_parts);
        
        // Read file list header
        file.seek(SeekFrom::Start(file_list_offset)).unwrap();
        let mut fl_header = [0u8; 8];
        file.read_exact(&mut fl_header).unwrap();
        let num_files = u32::from_le_bytes([fl_header[0], fl_header[1], fl_header[2], fl_header[3]]);
        let compressed_size = u32::from_le_bytes([fl_header[4], fl_header[5], fl_header[6], fl_header[7]]);
        
        println!("\nFile list:");
        println!("  Num files: {}", num_files);
        println!("  Compressed size: {}", compressed_size);
        
        // Read and decompress file list
        let mut compressed = vec![0u8; compressed_size as usize];
        file.read_exact(&mut compressed).unwrap();
        
        // Try to decompress
        println!("  First 32 bytes of compressed list:");
        for b in compressed.iter().take(32) {
            print!("{:02x} ", b);
        }
        println!();
        
        // Try chunked first, then single-block
        let decompressed = match decompress_lz4_chunked(&compressed) {
            Ok(d) => {
                println!("  Chunked decompression succeeded, size: {}", d.len());
                if d.len() != (num_files * 272) as usize {
                    println!("  Size mismatch, trying single-block...");
                    lz4_flex::block::decompress(&compressed, (num_files * 272) as usize).unwrap_or(d)
                } else {
                    d
                }
            }
            Err(_) => {
                println!("  Trying single-block LZ4...");
                lz4_flex::block::decompress(&compressed, (num_files * 272) as usize).unwrap_or_default()
            }
        };
        println!("  Decompressed size: {}", decompressed.len());
        println!("  Expected size for {} V18 entries: {}", num_files, num_files * 272);
        
        // Parse file entries (V18 format: 272 bytes each)
        if decompressed.len() >= 272 {
            println!("\n  File entries:");
            for i in 0..num_files as usize {
                let offset = i * 272;
                if offset + 272 > decompressed.len() {
                    break;
                }
                let entry = &decompressed[offset..offset + 272];
                
                // Name: 256 bytes null-terminated
                let name_end = entry[..256].iter().position(|&b| b == 0).unwrap_or(256);
                let name = String::from_utf8_lossy(&entry[..name_end]);
                
                // Offset: 4 bytes low + 2 bytes high
                let offset_low = u32::from_le_bytes([entry[256], entry[257], entry[258], entry[259]]) as u64;
                let offset_high = u16::from_le_bytes([entry[260], entry[261]]) as u64;
                let data_offset = offset_low | (offset_high << 32);
                
                let archive_part = entry[262];
                let compression_flags = entry[263];
                
                let size_on_disk = u32::from_le_bytes([entry[264], entry[265], entry[266], entry[267]]);
                let uncompressed = u32::from_le_bytes([entry[268], entry[269], entry[270], entry[271]]);
                
                println!("    [{}] {}", i, name);
                println!("        offset={}, part={}, flags=0x{:02X}, disk={}, uncomp={}",
                    data_offset, archive_part, compression_flags, size_on_disk, uncompressed);
            }
        }
    }
}

fn decompress_lz4_chunked(compressed: &[u8]) -> Result<Vec<u8>, String> {
    let mut result = Vec::new();
    let mut pos = 0;
    
    while pos + 4 <= compressed.len() {
        let chunk_size = u32::from_le_bytes([
            compressed[pos], compressed[pos+1], compressed[pos+2], compressed[pos+3]
        ]) as usize;
        pos += 4;
        
        if chunk_size == 0 || pos + chunk_size > compressed.len() {
            break;
        }
        
        let chunk = &compressed[pos..pos + chunk_size];
        pos += chunk_size;
        
        let decompressed = lz4_flex::block::decompress(chunk, 256 * 1024)
            .map_err(|e| format!("LZ4 decompress error: {}", e))?;
        result.extend_from_slice(&decompressed);
    }
    
    Ok(result)
}
