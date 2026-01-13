use std::env;
use std::fs::File;
use std::io::Read;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        println!("Usage: dump_header <pak>");
        return;
    }

    for path in &args[1..] {
        println!("\n=== {} ===", path);
        let mut file = File::open(path).unwrap();
        let file_len = file.metadata().unwrap().len();

        println!("File size: {} bytes", file_len);

        let mut start = [0u8; 64];
        file.read_exact(&mut start).unwrap();
        println!("\nFirst 64 bytes:");
        for (i, chunk) in start.chunks(16).enumerate() {
            print!("{:04x}: ", i * 16);
            for b in chunk {
                print!("{:02x} ", b);
            }
            print!(" |");
            for b in chunk {
                if *b >= 32 && *b < 127 {
                    print!("{}", *b as char);
                } else {
                    print!(".");
                }
            }
            println!("|");
        }

        let mut end = [0u8; 64];
        let end_offset = file_len.saturating_sub(64);
        use std::io::{Seek, SeekFrom};
        file.seek(SeekFrom::Start(end_offset)).unwrap();
        let bytes_read = file.read(&mut end).unwrap();
        println!("\nLast {} bytes (from offset {}):", bytes_read, end_offset);
        for (i, chunk) in end[..bytes_read].chunks(16).enumerate() {
            print!("{:04x}: ", end_offset as usize + i * 16);
            for b in chunk {
                print!("{:02x} ", b);
            }
            print!(" |");
            for b in chunk {
                if *b >= 32 && *b < 127 {
                    print!("{}", *b as char);
                } else {
                    print!(".");
                }
            }
            println!("|");
        }
    }
}
