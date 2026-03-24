#  T10-DIF Separate Mode (Split Metadata)
The standard calculates an additional 8 bytes of protection metadata for each data block (4096 bytes), but stores it separately from the raw data on the disk.

Record structure:
1. Guard Tag (2 bytes): CRC16 of the entire data block.
2. Application Tag (2 bytes): to indicate the data type.
3. Reference Tag (4 bytes): part of the LBA (Logical Block Address), to prevent writing a block to the wrong location.

# Run
```bash
cargo build
sudo ./target/debug/DIF
```

# Test
```bash
cargo test
```
