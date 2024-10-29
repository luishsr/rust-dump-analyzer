use std::fs::File;
use std::io::{self, Write};
use rand::{random};

fn main() -> io::Result<()> {
    let mut file = File::create("test_dump.bin")?;

    // Insert a PDF header signature at the beginning
    file.write_all(b"%PDF-1.4\n")?;

    // Fill with random data for padding
    let padding: Vec<u8> = (0..1024).map(|_| random()).collect();
    file.write_all(&padding)?;

    // Insert a JPEG signature at 1KB offset
    file.write_all(b"\xFF\xD8\xFF\xE0")?;

    // More padding
    let padding: Vec<u8> = (0..1024).map(|_| random()).collect();
    file.write_all(&padding)?;

    // Insert an ASCII string at 2KB offset
    file.write_all(b"Hello, this is a test ASCII string.")?;

    // Add more random data to reach a certain size, e.g., 1 MB
    let padding: Vec<u8> = (0..1024 * 1024 - 4096).map(|_| random()).collect();
    file.write_all(&padding)?;

    println!("Generated test_dump.bin with known patterns for testing.");
    Ok(())
}
