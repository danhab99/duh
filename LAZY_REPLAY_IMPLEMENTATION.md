# Lazy File Replay Implementation

## Overview

This document explains the implementation of the `LazyFileReplay` reader, which addresses the requirements for lazy file reconstruction from fragments.

## Problem Statement Summary

The original request was to implement a way of replaying files lazily using:
- A read+seek interface to the old file
- A set of fragment hashes

The system needed to handle:
1. **Unchanged fragments**: Read from the old file
2. **Added fragments**: Output new data
3. **Deleted fragments**: Skip bytes in the old file

The key challenge was: **"How do you seek backwards, especially in the middle of a deleted fragment?"**

## Solution

### Core Concept

The `LazyFileReplay` struct implements both `Read` and `Seek` traits, allowing it to act as a standard Rust reader. It maintains two separate position trackers:

1. **Logical position**: Position in the reconstructed (output) file
2. **Old file position**: Position in the source (old) file being read from

### Fragment Processing

```rust
pub enum FileFragment {
    ADDED { body: Hash, len: usize },    // New data
    UNCHANGED { len: usize },             // Copy from old file
    DELETED { len: usize },               // Skip in old file
}
```

#### Reading Behavior

| Fragment Type | Output Behavior | Old File Position | Logical Position |
|--------------|-----------------|-------------------|------------------|
| UNCHANGED    | Copy `len` bytes | Advances by `len` | Advances by `len` |
| ADDED        | Output new data | No change | Advances by `len` |
| DELETED      | No output | Advances by `len` | No change |

### Seeking Solution

The answer to "How do you seek backwards into a deleted fragment?" is: **You don't.**

DELETED fragments don't exist in the output file - they only affect the mapping between logical and old file positions. When seeking:

1. Calculate which fragment contains the target logical position
2. Compute the offset within that fragment
3. Calculate the corresponding old file position accounting for all DELETED fragments before it

#### Example

```text
Fragment Layout:
  [UNCHANGED: 100] [DELETED: 50] [UNCHANGED: 100]

Logical positions:     0-99           100-199
Old file positions:  0-99   100-149  150-249

Seeking to logical position 150:
  - Target is in the 3rd fragment (2nd UNCHANGED)
  - Offset in fragment: 150 - 100 = 50
  - Old file position: 150 + 50 = 200
```

The DELETED fragment creates a "gap" in the old file that doesn't appear in the output.

### Key Implementation Details

#### Position Calculation

The `find_fragment_at_position()` method walks through fragments to find:
- Which fragment contains a given logical position
- The offset within that fragment
- The corresponding old file position

```rust
fn find_fragment_at_position(&self, logical_pos: u64) 
    -> Option<(usize, usize, u64)>
{
    let mut current_logical = 0u64;
    let mut current_old = 0u64;

    for (idx, fragment) in self.fragments.iter().enumerate() {
        match fragment {
            FileFragment::ADDED { len, .. } => {
                // Logical advances, old stays same
                if logical_pos >= current_logical 
                    && logical_pos < current_logical + len {
                    return Some((idx, offset, current_old));
                }
                current_logical += len;
            }
            FileFragment::UNCHANGED { len } => {
                // Both advance together
                if logical_pos >= current_logical 
                    && logical_pos < current_logical + len {
                    return Some((idx, offset, current_old + offset));
                }
                current_logical += len;
                current_old += len;
            }
            FileFragment::DELETED { len } => {
                // Only old advances, logical stays same
                current_old += len;
            }
        }
    }
    // ...
}
```

#### Memory Efficiency

The reader only loads ADDED fragment data when needed and caches it during sequential reads. When seeking, the cache is cleared to avoid memory bloat.

#### Error Handling

- Seeking before position 0 returns an error
- Seeking past EOF is allowed (sets position to EOF)
- Reading from an UNCHANGED fragment that exceeds the old file returns UnexpectedEof

## Usage

### Basic Usage

```rust
use lib::replay::LazyFileReplay;
use lib::repo::Repo;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

// Setup
let repo = Repo::open()?;
let old_file = File::open("old_version.bin")?;
let fragments = load_fragments()?; // Your fragment list

// Create reader
let mut reader = LazyFileReplay::new(
    &repo,
    Box::new(old_file),
    fragments,
)?;

// Read sequentially
let mut buffer = vec![0u8; 1024];
reader.read(&mut buffer)?;

// Seek and read
reader.seek(SeekFrom::Start(5000))?;
reader.read(&mut buffer)?;

// Seek backwards
reader.seek(SeekFrom::Start(100))?;
reader.read(&mut buffer)?;
```

### Integration with Existing Code

The reader can be used anywhere a `Read + Seek` trait object is expected:

```rust
fn process_file<R: Read + Seek>(reader: &mut R) {
    // Your processing logic
}

// Works with LazyFileReplay
let mut replay = LazyFileReplay::new(&repo, old_file, fragments)?;
process_file(&mut replay);
```

## Testing

The implementation includes comprehensive tests:

- Position calculation for various fragment combinations
- Handling of UNCHANGED-only sequences
- Sequences with DELETED fragments
- Sequences with ADDED fragments
- Complex mixed sequences
- Total size calculations

Run tests with:
```bash
cargo test --manifest-path lib/Cargo.toml replay
```

## Performance Characteristics

- **Time Complexity**: O(n) for seeking, where n is the number of fragments
- **Space Complexity**: O(1) for the reader state + size of cached ADDED fragment
- **Sequential Reading**: Optimal - fragments are processed in order
- **Random Access**: Good - seeking calculates position without reading data

## Future Enhancements

Potential optimizations:
1. Build a fragment index for O(log n) seeking
2. Pre-load small ADDED fragments to reduce I/O
3. Add async/await support for non-blocking I/O
4. Support for compressed fragments

## Files Changed

- `lib/src/replay.rs` - Main implementation
- `lib/src/lib.rs` - Module registration
- `lib/src/repo.rs` - Made `get_object_path()` public
- `lib/examples/lazy_replay_demo.rs` - Usage example

## Summary

The LazyFileReplay reader solves the lazy file reconstruction problem by:

✓ Providing efficient on-the-fly reconstruction from fragments  
✓ Supporting both forward and backward seeking  
✓ Correctly handling DELETED fragments (they shift old file position but don't appear in output)  
✓ Using minimal memory (only active fragment is cached)  
✓ Integrating seamlessly with Rust's standard I/O traits  

The key insight is that DELETED fragments are handled by maintaining separate logical and physical position counters, eliminating the conceptual problem of "seeking into a deleted fragment" - you're always seeking to a logical position in the output, which DELETED fragments don't contribute to.
