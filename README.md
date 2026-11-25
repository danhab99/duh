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

duh has its own vocabulary, chosen to read naturally to artists rather than
to programmers; if you know git, every git command name works too. Everything
below is exercised end to end by [`demo.sh`](demo.sh), which
`nix flake check` runs on every build.

| duh | what it does | git |
|---|---|---|
| `start` | start a space in this folder | `init` |
| `get <url> [folder]` | copy a space into a new folder | `clone` |
| `look` | what's changed, picked, missing or new | `status` |
| `compare [--picked]` | how much of each file changed (a diff bar per file) | `diff` |
| `pick <files>` / `pick -A` / `pick -u` | choose what goes into the next save | `add` |
| `unpick <files>` | leave files out of the next save, keeping edits | `restore --staged` |
| `save -m "…"` / `save -a` | save a new version | `commit` |
| `bring-back <file> [--from v] [--as path]` | a file as it was in your picks, the last save, or any version | `restore`, `checkout` |
| `rename`, `delete`, `forget` | move, delete, or stop tracking files | `mv`, `rm`, `rm --cached` |
| `history [--oneline]`, `inspect [v]` | your saves; one version in detail | `log`, `show` |
| `journal` | everywhere a draft has pointed, to recover lost work | `reflog` |
| `label <name> [v]` | name a version, e.g. `final-v2` | `tag` |
| `rewind <v> [--keep-picked \| --discard]` | move your draft to another version | `reset` |
| `drafts [name]`, `open <draft>` / `open --new <draft>` | alternative lines of work | `branch`, `switch` |
| `send [place]`, `receive [place]` | share saves with a place (never merges) | `push`, `pull` |
| `places` | folders, buckets and drives your space is shared to | `remote` |
| `settings` | view or change settings | `config` |
| `check [place]` | check that every save is intact and reads back exactly, here or in a place | `fsck` |

Versions can be named by label, draft, short hash, `HEAD~N` or `X^`.
`.duhignore` keeps files out of `look` and `pick`. Places work on 34 storage
backends (S3, GCS, Azure, WebDAV, SFTP, Google Drive, …). Merging, rebasing
and stashing are out of scope for now.

## duh Studio, the desktop app

Everything above, for people who'd rather not open a terminal:

```bash
nix run .#studio              # or: nix run .#studio -- /path/to/a/space
```

One window: your unsaved files at the top of a timeline of saves, each drawn
with a change strip (where bytes were added, removed and kept). Tick files to
pick them, write what changed, press Save (Ctrl+S). Drafts, labels, places,
send/receive, recovering lost work and `check` are all there, and Ctrl+K
finds anything else.

Studio and the CLI run the same engine (`lib::ops`, reached from Dart through
flutter_rust_bridge), so they always agree, and you can use both on one space
at once. Design and plan: [docs/studio/DESIGN.md](docs/studio/DESIGN.md).

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
│   ├── store.rs      # Storage backends: any opendal service, built from a URL + options
│   ├── transfer.rs   # Spaces copying objects and commits to one another
│   ├── check.rs      # duh check: is every save intact?
│   └── ops/          # Every operation, returning data; the CLI and Studio call these
├── cli/              # Command-line interface: parse, call lib::ops, print
└── studio/           # duh Studio (Flutter) and its Rust bridge (studio/native)
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

```bash
duh start                        # start a space in this folder
duh pick .                       # pick every file
duh save -m "First sketch"

# ...work on your files...
duh look                         # what changed?
duh compare                      # how much of each file changed
duh pick -A                      # pick new, changed and deleted files
duh save -m "Lighting pass"      # or: duh save -a -m "..." for tracked files

duh history --oneline            # your saves
duh label final-v1               # name this version
duh bring-back scene.blend --from final-v1   # bring back an older version
duh open --new experiment        # try something on a new draft
duh get ../project my-copy       # copy a space
```

### Places and storage backends

A space keeps its history in a *store*: any filesystem-like opendal backend.
A place is a URL plus whatever options that backend needs, stored in plain
text in the space's settings (secrets-manager integration is planned):

```bash
duh places add origin ../shared-hub          # a folder (absolute path saved)
duh places add warehouse s3://my-bucket/game \
    -o region=eu-west-1 -o access_key_id=AKIA… -o secret_access_key=…
duh send warehouse        # the first send creates the space in an empty prefix
duh receive warehouse     # never merges: only takes saves that lose nothing
duh get s3://my-bucket/game game -o region=eu-west-1 -o access_key_id=… -o secret_access_key=…
```

In `.duh/config` that looks like:

```toml
[remote.warehouse]
url = "s3://my-bucket/game"

[remote.warehouse.config]        # passed to the backend as-is
region = "eu-west-1"
access_key_id = "AKIA…"
secret_access_key = "…"
```

The URL scheme picks the backend and the options use
[opendal's names for that service](https://docs.rs/opendal/0.52.0/opendal/services/index.html);
`duh settings set remote.NAME.config.KEY VALUE` changes one later. Included:

- **Object storage:** `s3`, `gcs`, `azblob`, `oss`, `obs`, `cos`, `b2`, `swift`, `upyun`, `supabase`, `vercel-blob`, `lakefs`
- **File protocols:** local paths / `file://`, `ftp`, `sftp`, `webdav`, `webhdfs`, `hdfs`, `azfile`, `azdls`, `dbfs`, `alluxio`, `ipmfs`
- **Cloud drives:** `gdrive`, `onedrive`, `dropbox`, `pcloud`, `koofr`, `yandex-disk`, `aliyun-drive`, `seafile`, `icloud`
- **Read-only:** `http`/`https` — `duh get` a `.duh/` folder served by any static web server
- **Testing:** `memory`

Databases and caches aren't offered: caches evict data and databases cap
value sizes, neither of which a repository can survive. `send` and `receive`
refuse to drop anyone's saves (`send --force` overrides), `receive` refuses to
run over unsaved changes, and `send` won't move the draft another space has
open.

Run `duh <command> --help` for every option.

### Configuration

Each space's settings live in `.duh/config` (TOML), editable with
`duh settings get <key>` and `duh settings set <key> <value>`:

```toml
chunk_size = 4096          # target chunk size for content-defined chunking
max_size = 10485760        # largest stored fragment

[core]
bare = false

[user]
name = "Your Name"
email = "your.email@example.com"
```

Patterns in a `.duhignore` file at the top of the space keep files out of
`look` and `pick`; lines starting with `!` re-include them.

## Build & development (Nix + justfile) 🔧

Crates are fetched by Nix from `cli/Cargo.lock`; there's no vendoring step.

- `just build` — `nix build .#duh`
- `just check` — `nix flake check`: builds, runs the unit tests, and runs `demo.sh` in the sandbox
- `just demo` — just the sandboxed demo
- `./demo.sh [dir]` (or `just demo_script [dir]`) — run `demo.sh` against the Nix-built binary, to watch it; with no `dir` it uses a fresh temporary folder
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
- [ ] Credentials from secrets managers instead of plain-text config
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
