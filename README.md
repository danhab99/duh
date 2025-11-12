# duh - Binary-Optimized Version Control

A version control system designed to handle large binary files efficiently using rolling hash algorithms, inspired by rsync's delta synchronization approach.

## What is duh?

**duh** is a Git-inspired version control system that excels at managing large binary project files. While Git stores complete snapshots of binary files at each commit (leading to repository bloat), duh uses **rolling hash algorithms** similar to rsync to store only the differences between file versions.

### The Problem with Git and Binary Files

Git is excellent for text files but struggles with binary files because:
- Binary files can't be meaningfully diff'd line-by-line
- Each change to a binary file requires storing the entire file again
- Large binary files quickly bloat the repository size
- Cloning and pulling become slow as history grows

### How duh Solves This

duh uses a **rolling hash algorithm** (Rabin-Karp) to:
1. Break files into chunks based on content boundaries
2. Identify which chunks have changed between versions
3. Store only the changed chunks (delta encoding)
4. Reconstruct any version by applying the appropriate deltas

This is the same approach rsync uses to efficiently synchronize files over a network—when rsync runs a rolling hash and the destination has a different hash than the source, only the changed portions are transferred. duh applies this same principle to version control.

## Project Status

duh is actively under development with the goal of achieving **feature parity with Git** where it makes sense. The core functionality is in place, including:

- ✅ Repository initialization
- ✅ File tracking and staging
- ✅ Rolling hash-based delta storage
- ✅ Commit creation with metadata
- ✅ Status reporting
- ✅ Diff computation using block signatures
- 🚧 Branch management
- 🚧 Merge operations
- 🚧 Remote repositories
- 🚧 History traversal

## Architecture

### Core Components

```
duh/
├── lib/              # Core library implementing version control logic
│   ├── diff.rs       # Rolling hash algorithm and delta generation
│   ├── hash.rs       # Content-addressable hashing (SHA-256)
│   ├── objects.rs    # Object model (Commit, Tree, File, Fragment)
│   ├── repo.rs       # Repository management and operations
│   └── utils.rs      # Utilities and helpers
└── cli/              # Command-line interface
    ├── init.rs       # Repository initialization
    ├── status.rs     # Working directory status
    ├── track.rs      # File staging
    ├── diff.rs       # Difference visualization
    └── commit.rs     # Commit creation
```

### Object Model

duh uses a content-addressable storage system similar to Git, with the following object types:

1. **Fragment**: A diff fragment representing added, unchanged, or deleted bytes
2. **File**: References to content hash and delta fragments
3. **Tree**: Directory structure with file and subtree references
4. **Commit**: Snapshot of the tree with metadata (author, message, parent, timestamp)
5. **StagedFile**: Temporary representation of files being prepared for commit

### Rolling Hash Algorithm

The heart of duh's efficiency is its rolling hash implementation:

```
1. Divide file into overlapping windows
2. Calculate hash for each window position
3. Create "block signatures" of stable regions
4. Compare signatures between versions
5. Generate diff fragments:
   - ADDED: New bytes not in previous version
   - UNCHANGED: Bytes matching a known block
   - DELETED: Bytes present in old but not new version
```

This allows duh to:
- Detect moved/copied content within files
- Store only actual changes, not entire files
- Efficiently reconstruct any historical version
- Keep repository size manageable even with large binaries

## Getting Started

### Installation

```bash
# Build from source
cd cli
cargo build --release

# The binary will be at cli/target/release/duh
```

### Basic Usage

```bash
# Initialize a new repository
duh init

# Track files for commit
duh track file1.bin file2.bin

# Check status
duh status

# View differences
duh diff

# Commit changes
duh commit -m "Initial commit"
```

### Configuration

Repository configuration is stored in `.duh/config.toml`:

```toml
chunk_size = 4096.0  # Size of blocks for rolling hash

[user]
name = "Your Name"
email = "your.email@example.com"
```

## How It Works: Rolling Hash Delta Storage

### Example: Editing a Large Binary File

Imagine you have a 100MB binary file and you edit 1MB in the middle:

**Git's approach:**
- Original version: 100MB stored
- After edit: 100MB stored again
- Total: 200MB

**duh's approach:**
- Original version: 100MB stored as fragments
- After edit: Only the changed ~1MB stored as new fragments
- Unchanged fragments: Referenced from original version
- Total: ~101MB

### Block Signature Matching

```
Original file blocks:  [A][B][C][D][E]
Modified file blocks:  [A][B][X][Y][E]

duh stores:
- Reference to blocks A, B (unchanged)
- New data for blocks X, Y (added/changed)
- Reference to block E (unchanged)
- Note that C, D were deleted
```

This allows efficient storage and reconstruction of any version in the history.

## Goals and Roadmap

### Core Goals
1. **Efficient binary file handling**: Store only deltas, not full copies
2. **Git-like workflow**: Familiar commands and concepts for easy adoption
3. **Content integrity**: Cryptographic hashing ensures data validity
4. **Performance**: Fast operations even with large files and deep history

### Planned Features
- [ ] Branch and tag management
- [ ] Merge strategies for binary content
- [ ] Remote repository support (push/pull)
- [ ] Repository compression and garbage collection
- [ ] Partial clone/checkout for large repositories
- [ ] Web interface for repository browsing
- [ ] Plugin system for custom diff/merge handlers

## Technical Details

### Storage Format
- Objects stored in MessagePack format for efficiency
- Content-addressable storage using SHA-256 hashes
- Rolling hash parameters: Base 256, large prime modulus

### Performance Characteristics
- **Time complexity**: O(n) for diff generation (n = file size)
- **Space complexity**: O(changed blocks) not O(file size)
- **Best case**: Files with localized changes
- **Worst case**: Completely rewritten files (falls back to full storage)

## Contributing

This project is under active development. Contributions are welcome! Areas where help is needed:

- Merge algorithm development for binary files
- Remote repository protocols
- Performance optimization
- Documentation and examples
- Test coverage

## License

[Add your license here]

## Related Projects

- **Git**: The inspiration for the object model and workflow
- **rsync**: The inspiration for rolling hash delta encoding
- **Git-LFS**: Alternative approach using pointer files and external storage
- **Perforce**: Commercial VCS with good binary file support
