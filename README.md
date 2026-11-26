# duh

**Version control for files that aren't text.** duh keeps every version of
your Blender scenes, FreeCAD models, GIMP images, audio and video projects.
Each save stores only the bytes that changed, and you get every version back
exactly, byte for byte.

duh stands for *deduplicative update helper*. It works like git (saves,
drafts, history, sharing) but it's built for big binary files and for people
who don't think in terminals. There's a command line, and a desktop app,
**duh Studio**, that does the same things in a window.

> **Status:** young and single-author. The storage engine is tested hard (see
> [How we know it works](#how-we-know-it-works)), but the save format may
> still change before 1.0. There is no merging: two people editing the same
> draft take turns.

---

## Install

Each [release](https://github.com/danhab99/duh/releases) has two files that
need nothing else installed:

| File | For |
|---|---|
| `duh-studio-x86_64-linux.AppImage` | duh Studio, the desktop app, on any 64-bit Linux: `chmod +x` it and run it |
| `duh-x86_64-windows.exe` | duh's command line on Windows (every kind of place except SFTP) |

### macOS, and the desktop app on Windows

I don't use macOS or Windows, and I don't know how to package for them.
Nix builds everything above. It can't build the desktop app for Windows or
macOS, because Flutter needs Visual Studio on Windows and Xcode on macOS.

If you know how, contributions are very welcome. The app is ordinary
Flutter (`studio/`) plus a Rust library it loads (`studio/native/`). The
command line is plain Rust and should build on macOS with `nix build`. The
AppImage and the `.exe` must keep coming out of `nix build .#ci`.

### With Nix

From a checkout:

```bash
nix run .#studio           # the desktop app
nix run .# -- --help       # the command line
```

Or add both to Home Manager. Studio installs a launcher entry, so it appears
in rofi and application menus:

```nix
# flake inputs: duh.url = "github:danhab99/duh";
home.packages = [
  inputs.duh.packages.${pkgs.system}.duh      # `duh`
  inputs.duh.packages.${pkgs.system}.studio   # `duh-studio`
];
```

## duh Studio

One window:
- your unsaved files at the top;
- below them, a timeline of every save, each with a *change strip* that shows
  where bytes were added, removed and kept.

Tick the files you want, write what changed, and press **Save** (Ctrl+S).
Drafts, labels, sharing, recovering lost work and integrity checks are all
there; **Ctrl+K** finds anything else. It follows your system's light or dark
theme, or you can pick one.

Studio calls the same engine as the command line, so the two always agree,
and you can use both on one project at once.

## The command line

duh's words are chosen for artists. If you know git, every git name works
too.

```bash
duh start                        # keep versions of the files in this folder
duh pick .                       # choose what goes into the next save
duh save -m "First sketch"

duh look                         # what changed since the last save?
duh compare                      # how much of each file changed
duh save -a -m "Lighting pass"   # save every tracked file that changed

duh history --oneline            # your saves
duh label final-v1               # name this version
duh bring-back scene.blend --from final-v1
duh open --new experiment        # try something on a new draft
```

| duh | what it does | git |
|---|---|---|
| `start` | start a space in this folder | `init` |
| `get <url> [folder]` | copy a space into a new folder | `clone` |
| `look` | what's changed, picked, missing or new | `status` |
| `compare [--picked]` | how much of each file changed | `diff` |
| `pick <files>` / `-A` / `-u` | choose what goes into the next save | `add` |
| `unpick <files>` | leave files out of the next save, keeping edits | `restore --staged` |
| `save -m "…"` / `save -a` | save a new version | `commit` |
| `bring-back <file> [--from v] [--as path]` | a file as it was at any version | `restore`, `checkout` |
| `rename`, `delete`, `forget` | move, delete, or stop tracking files | `mv`, `rm`, `rm --cached` |
| `history`, `inspect [v]` | your saves; one version in detail | `log`, `show` |
| `journal` | everywhere a draft has pointed, to recover lost work | `reflog` |
| `label <name> [v]` | name a version | `tag` |
| `rewind <v> [--keep-picked \| --discard]` | move your draft to another version | `reset` |
| `drafts`, `open <draft>` / `open --new <draft>` | separate lines of work | `branch`, `switch` |
| `send [place]`, `receive [place]` | share saves with a place | `push`, `pull` |
| `places` | where your space is shared | `remote` |
| `settings` | view or change settings | `config` |
| `check [place]` | prove every save is intact and reads back exactly | `fsck` |

You can name a version by label, by draft, by short hash, or as `HEAD~N` or
`X^`. A `.duhignore` file keeps files out of `look` and `pick`: one glob per
line, `#` for comments and `!` to let a file back in.
Run `duh <command> --help` for every option.

## Sharing

A *place* is anywhere a copy of your space can live: a folder, a USB drive,
a NAS, or cloud storage. There's no server to run, because duh talks to
storage directly through [opendal](https://opendal.apache.org):

```bash
duh places add backup /mnt/nas/my-project
duh places add cloud s3://my-bucket/my-project \
    -o region=eu-west-1 -o access_key_id=… -o secret_access_key=…
duh send cloud            # the first send creates the space there
duh receive cloud
duh get s3://my-bucket/my-project my-copy -o region=… …
```

Supported places:

| Kind | Schemes |
|---|---|
| Object storage | `s3`, `gcs`, `azblob`, `oss`, `obs`, `cos`, `b2`, `swift`, `upyun`, `supabase`, `vercel-blob`, `lakefs` |
| File protocols | local paths and `file://`, `ftp`, `sftp`, `webdav`, `webhdfs`, `hdfs`, `azfile`, `azdls`, `dbfs`, `alluxio`, `ipmfs` |
| Cloud drives | `gdrive`, `onedrive`, `dropbox`, `pcloud`, `koofr`, `yandex-disk`, `aliyun-drive`, `seafile`, `icloud` |
| Read-only | `http` and `https`: `duh get` from a `.duh/` folder on any static web server |

Options use
[opendal's names for each service](https://docs.rs/opendal/0.52.0/opendal/services/index.html).
They're stored **in plain text** in `.duh/config`.

duh never merges. Instead:
- `receive` only takes saves that lose nothing, and refuses to run over unsaved
  changes.
- `send` refuses to drop anyone's saves unless you add `--force`.
- `send` won't move a draft that another copy has open.

## How it works

**Saves are edit scripts.** duh splits each version of a file into chunks
with content-defined chunking: a gear hash finds the cut points, so an edit
only disturbs the chunks around it. The new version is then stored as an
edit script against the one before:

```
before:  [A][B][C][D][E]
after:   [A][B][X][Y][E]

stored:  UNCHANGED A,B · DELETED C,D · ADDED X,Y · UNCHANGED E
```

Only the ADDED bytes take new space. A file moved with `duh rename` is
compared against its old self, so renaming costs next to nothing.

**Chunks have a ceiling.** A chunk is never longer than eight times the
target size, or `max_size`, whichever is smaller. A file with long uniform
stretches, like an image's empty canvas, still gets cut, so a one-byte edit
inside it stays small.

**Old versions stay fast.** Rebuilding a version means replaying its edit
scripts. To stop long histories from getting slow, duh stores one version in
the middle whole as a *solid copy* after every `solid_after` saves of a file
(default 100). Reading any version then replays at most about that many
steps.

Solid copies live beside the history, not inside it, so they never change a
save's identity.

**Everything is content-addressed.** Saves, files, edit scripts and chunks
are [MessagePack](https://msgpack.org) objects named by their SHA-256.
`duh check` reads every object, re-hashes it, follows every link, and
rebuilds every version of every file. Each rebuilt version must match the
hash recorded when it was saved.

## Settings

Each space's settings live in `.duh/config` (TOML). Read or change them with
`duh settings`:

```toml
chunk_size = 4096          # average chunk size, in bytes
max_size = 10485760        # largest stored piece, in bytes
solid_after = 100          # saves of a file between solid copies (optional)

[user]
name = "Your Name"
email = "you@example.com"

[remote.cloud]
url = "s3://my-bucket/my-project"
[remote.cloud.config]      # handed to the storage backend as-is
region = "eu-west-1"
```

## How we know it works

[`demo.sh`](demo.sh) is the standard. It runs every command the way a person
would. It asserts the outcome of each command, and it checks out every file
at every save and compares them byte for byte. It also:
- shares spaces through a folder and an S3 server;
- makes 110 saves to exercise solid copies;
- damages a space on purpose to prove `check` catches it.

`nix flake check` runs it on every build, along with:
- the library and CLI tests;
- a test that drives the real engine from the desktop app's bridge;
- a check that the packaged app starts.

```bash
./demo.sh          # watch it run (in a fresh temporary folder)
just check         # everything, the way CI would
```

## Working on duh

```
lib/            the engine; every operation lives in lib/src/ops
cli/            the command line: parse, call lib::ops, print
studio/         duh Studio (Flutter)
studio/native/  the Rust bridge Studio loads (flutter_rust_bridge)
docs/           design notes
```

The CLI and Studio are thin shells over `lib::ops`. New behaviour goes in the
library first.

| | |
|---|---|
| `nix develop` | Rust toolchain, rust-analyzer, just |
| `nix develop .#studio` | adds Flutter and the bridge code generator |
| `just build` / `just check` | build, or run every test and the demo |
| `just studio-test` | Studio's widget tests and the real-engine test |
| `just studio-screens` | render Studio's screens to `studio/test_screens/out/` |
| `just studio-codegen` | regenerate the bridge after changing `studio/native/src/api` (slow) |

**Releasing.** Push an annotated tag. CI builds `nix build .#ci` from that
exact commit and publishes its files. The tag's first line becomes the
release title and the rest becomes its notes:

```bash
git tag -a v0.3.0       # subject, blank line, notes
git push origin v0.3.0
```

The desktop app's design and plan are in
[docs/studio/DESIGN.md](docs/studio/DESIGN.md). How much of this project was
written with AI help, and how, is in [docs/MEMO-on-ai.md](docs/MEMO-on-ai.md).

## Where it's going

- **duh.space:** a home for spaces, like GitHub is for git.
- Credentials from a secrets manager instead of plain-text settings.
- Clearing out data no save needs any more (garbage collection).
- Fetching only the versions you need from large spaces.
- Image and model previews in Studio, and macOS and Windows builds.

## Related projects

- **git**: the object model and the workflow.
- **rsync**: delta transfer with rolling hashes.
- **Git LFS** and **DVC**: large files stored beside git, as pointers.
- **Perforce** and **Plastic SCM**: commercial version control popular with
  game and film studios.
