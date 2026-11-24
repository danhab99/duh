# D[eduplicative] U[pdate] H[elper]

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

duh covers the everyday git workflow for a single project directory.
Everything below is exercised end to end by [`demo.sh`](demo.sh), which
`nix flake check` runs on every build:

- ✅ `init`, `clone`
- ✅ `add` (files, directories, globs, `-A`, `-u`), `rm` (`--cached`), `mv`
- ✅ `status` (staged / modified / missing / untracked), `diff` (`--staged`)
- ✅ `commit` (`-a`, `--allow-empty`), `log` (`--oneline`), `show`, `reflog`
- ✅ `restore` (`--staged`, `--source`), `checkout <file>`
- ✅ `branch`, `switch` (`-c`), `tag`, `reset` (`--soft` / `--mixed` / `--hard`)
- ✅ Commits named by branch, tag, short hash, `HEAD~N` or `X^`
- ✅ `.duhignore`
- 🚧 Pushing and pulling between remotes (`copy`, `remote`) — implemented, not yet covered by the demo
- ❌ Merging, rebasing, stashing — out of scope for now

## Architecture

### Core Components

```
duh/
├── lib/              # Core library implementing version control logic
│   ├── dedup.rs      # Content-defined chunking and the edit script between versions
│   ├── replay.rs     # Lazily reconstructs a file version from its fragments
│   ├── file.rs       # Staging, committing and reading file versions
│   ├── space.rs      # A repository ("space"): objects, refs, index, paths
│   ├── objects.rs    # Object model (Commit, Tree, File, Fragment)
│   ├── hash.rs       # Content-addressable hashing (SHA-256)
│   └── remote.rs     # Copying commits between spaces
└── cli/              # Command-line interface, one module per command
```

### Object Model

duh uses a content-addressable storage system similar to Git, with the following object types:

1. **Fragment**: raw bytes added in some version
2. **FileDiffFragment**: one step of a version's edit script — ADDED (a fragment), UNCHANGED or DELETED (a length)
3. **File**: a file version — its content hash and edit script against the previous version of the file
4. **Tree**: directory structure with file and subtree references
5. **Commit**: a tree plus metadata (author, message, parent)

### Content-Defined Chunking

The heart of duh's efficiency is how it compares versions:

```
1. Split both versions into chunks at content-defined boundaries (gear hash)
2. Hash each chunk
3. Walk the new version's chunks in order, matching them against the old one
4. Emit an edit script:
   - UNCHANGED: bytes reused from the previous version
   - DELETED:   bytes of the previous version that are gone
   - ADDED:     new bytes, stored once
```

Because boundaries depend on content, an insertion only disturbs the chunks
around it, so duh can:
- Store only actual changes, not entire files (even renamed ones)
- Efficiently reconstruct any historical version
- Keep repository size manageable even with large binaries

## Getting Started

### Installation

```bash
nix build .#duh          # binary at ./result/bin/duh
# or run it directly
nix run .# -- --help
```

### Basic Usage

The workflow is git's:

```bash
duh init                       # create a space in the current directory
duh add .                      # stage every file
duh commit -m "Initial commit"

# ...edit files...
duh status                     # what changed?
duh diff                       # how much of each file changed (a diff bar per file)
duh add -A                     # stage new, modified and deleted files
duh commit -m "Update assets"  # or: duh commit -a -m "..." for tracked files

duh log --oneline              # history
duh restore --source HEAD~1 big.bin   # bring back an older version
duh switch -c experiment       # branch off
duh clone ../project copy      # clone a space
```

Run `duh <command> --help` for every option.

### Configuration

Each space's configuration lives in `.duh/config` (TOML), editable with
`duh config get <key>` and `duh config set <key> <value>`:

```toml
chunk_size = 4096          # target chunk size for content-defined chunking
max_size = 10485760        # largest stored fragment

[core]
bare = false

[user]
name = "Your Name"
email = "your.email@example.com"
```

Patterns in a `.duhignore` file at the top of the worktree keep files out of
`status` and `add`; lines starting with `!` re-include them.

## Build & development (Nix + justfile) 🔧

Crates are fetched by Nix from `cli/Cargo.lock`; there's no vendoring step.

- `just build` — `nix build .#duh`
- `just check` — `nix flake check`: builds, runs the unit tests, and runs `demo.sh` in the sandbox
- `just demo` — just the sandboxed demo
- `just demo_script <dir>` — run `demo.sh` in `<dir>` against the Nix-built binary, to watch it
- `nix develop` — a shell with the pinned toolchain, rust-analyzer and friends

`demo.sh` is the standard duh is held to: it asserts the outcome of every
command it runs and checks out every file at every commit to compare it
byte for byte.

---

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
- [ ] Demo coverage for pushing and pulling between remotes
- [ ] Repository garbage collection
- [ ] Partial clone/checkout for large repositories
- [ ] Merge strategies for binary content

## Technical Details

### Storage Format
- Objects stored in MessagePack format for efficiency
- Content-addressable storage using SHA-256 hashes
- Chunk boundaries from a gear hash over the content (`chunk_size` sets the average)

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
