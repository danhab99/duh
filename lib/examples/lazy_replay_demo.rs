/// Example demonstrating LazyFileReplay usage
/// 
/// This example shows how to use the LazyFileReplay reader to reconstruct
/// files from fragments without loading the entire file into memory.
///
/// Note: This is a conceptual example. In practice, you would use this
/// with a real Repo instance and fragment data.

use lib::objects::FileFragment;
use lib::hash::Hash;

fn main() {
    println!("LazyFileReplay Conceptual Example");
    println!("===================================\n");
    
    println!("The LazyFileReplay reader allows you to reconstruct files from fragments lazily.");
    println!("This is useful when working with large files and you only need to read portions of them.\n");
    
    println!("Usage Pattern:");
    println!("--------------");
    println!("1. Create a LazyFileReplay instance with:");
    println!("   - A reference to the repository");
    println!("   - A Read+Seek reader for the old file");
    println!("   - A list of FileFragment entries\n");
    
    println!("2. The reader handles three types of fragments:");
    println!("   - UNCHANGED: Reads bytes from the old file");
    println!("   - ADDED: Outputs new bytes stored in the repository");
    println!("   - DELETED: Skips bytes in the old file (doesn't output)\n");
    
    println!("3. You can:");
    println!("   - Read sequentially with reader.read()");
    println!("   - Seek to any position with reader.seek()");
    println!("   - The reader tracks positions automatically\n");
    
    println!("Example Fragment Sequence:");
    println!("-------------------------");
    
    // Create example fragments (conceptual, not executable)
    let hash = Hash::from_str("1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef");
    
    let _fragments = vec![
        FileFragment::UNCHANGED { len: 1000 },       // Copy first 1000 bytes
        FileFragment::DELETED { len: 500 },          // Skip next 500 bytes of old file
        FileFragment::ADDED { body: hash, len: 250 }, // Add 250 new bytes
        FileFragment::UNCHANGED { len: 750 },        // Copy 750 more bytes
    ];
    
    println!("Fragment 0: UNCHANGED {{ len: 1000 }}");
    println!("  Output: bytes 0-999 from old file");
    println!("  Old file position: 0-999\n");
    
    println!("Fragment 1: DELETED {{ len: 500 }}");
    println!("  Output: (nothing - this data is removed)");
    println!("  Old file position: 1000-1499 (skipped)\n");
    
    println!("Fragment 2: ADDED {{ body: <hash>, len: 250 }}");
    println!("  Output: bytes 1000-1249 from new fragment data");
    println!("  Old file position: 1500 (no change)\n");
    
    println!("Fragment 3: UNCHANGED {{ len: 750 }}");
    println!("  Output: bytes 1250-1999 from old file position 1500-2249");
    println!("  Old file position: 1500-2249\n");
    
    println!("Reconstructed File Properties:");
    println!("-----------------------------");
    println!("Total size: 2000 bytes (1000 + 0 + 250 + 750)");
    println!("Old file size needed: 2250 bytes");
    println!("Deleted bytes: 500");
    println!("Added bytes: 250");
    println!("Net change: -250 bytes\n");
    
    println!("Seeking Examples:");
    println!("----------------");
    println!("Seek to position 500:");
    println!("  - Within Fragment 0 (UNCHANGED)");
    println!("  - Reads from old file position 500\n");
    
    println!("Seek to position 1100:");
    println!("  - Within Fragment 2 (ADDED)");
    println!("  - Reads from added fragment data, offset 100");
    println!("  - Old file is at position 1500\n");
    
    println!("Seek to position 1500:");
    println!("  - Within Fragment 3 (UNCHANGED)");
    println!("  - Reads from old file position 1750 (1500 + 250 offset)\n");
    
    println!("Key Benefits:");
    println!("------------");
    println!("✓ Memory efficient - only loads fragments as needed");
    println!("✓ Seekable - can jump to any position without reading everything");
    println!("✓ Handles complex fragment sequences automatically");
    println!("✓ Transparent to users - works like any Read+Seek reader");
}
