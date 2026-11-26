# duh for git users

duh's model is git's: content-addressed objects, saves (commits) with one
parent, drafts (branches) as named pointers, a staging area, labels
(tags), places (remotes), and a journal (reflog). What's different is
built for large binary files and for people who don't use git.

## Every git command still works

duh's commands use plain words, but each git name is accepted too:

| git | duh | notes |
|---|---|---|
| `git init` | `duh start` | `--bare` for a history-only space to share through |
| `git clone <url> [dir]` | `duh get <url> [dir]` | any storage URL; `-o key=value` for credentials |
| `git status` | `duh look` | |
| `git diff` | `duh compare` | byte-level change bars, not line diffs |
| `git diff --staged` | `duh compare --picked` | |
| `git add` / `git stage` | `duh pick` | `-A` everything, `-u` tracked files only |
| `git restore --staged` / `git unstage` | `duh unpick` | keeps your edits |
| `git commit -m` | `duh save -m` | `-a` picks tracked changes first; `-g` writes the message for you |
| `git restore <file>` / `git checkout -- <file>` | `duh bring-back <file>` | `--from <version>` any version, `--as <path>` a copy beside your work |
| `git mv` | `duh rename` (or `mv`, `move`) | costs nothing in storage |
| `git rm` | `duh delete` (or `rm`) | refuses unsaved changes without `-f` |
| `git rm --cached` | `duh forget` | |
| `git log` | `duh history` | `--oneline`, `-n` |
| `git show` | `duh inspect` | |
| `git reflog` | `duh journal` | per draft, `-b <draft>` for another |
| `git tag` | `duh label` | `-d` to remove, `-l` to list |
| `git reset` | `duh rewind` | default keeps your files; `--keep-picked` = soft, `--discard` (alias `--hard`) = hard |
| `git branch` | `duh drafts` (or `draft`, `branch`) | `-d`, `-m`, `-s <version>` |
| `git switch` / `git switch -c` | `duh open` / `duh open --new` | |
| `git push` | `duh send` | fast-forward only unless `--force` |
| `git pull` | `duh receive` | fast-forward only, never merges |
| `git remote` | `duh places` (or `place`, `remote`) | |
| `git config` | `duh settings` (or `config`) | per space only |
| `git fsck` | `duh check` | rebuilds every version and compares hashes |

Versions are named the way you'd expect: a draft name, a label, a short
hash (any unique prefix), `HEAD`, `HEAD~3`, `final-v2^`.

## What's the same

- **Content addressing.** Every object is named by the SHA-256 of its
  encoding. A save's hash covers its files, its message, its author and
  its parent, so history can't change silently.
- **A staging area.** `pick` stages a snapshot of the file at that moment.
  Edit it again and `look` lists it as both picked and changed, just like
  git.
- **Branches are pointers.** A draft is a file holding a save's hash.
  Making one is free.
- **Fast-forward sharing.** `send` and `receive` move a draft forward and
  refuse anything that would drop someone's saves.
- **`HEAD`.** It names the open draft, so `HEAD~1` is the save before your
  last one.

## What's different

### No merging, ever

Each save has exactly one parent. There are no merge commits, no conflict
markers, no rebase, no cherry-pick. Binary files can't be merged
meaningfully anyway: there's no sensible combination of two edited
Blender scenes.

When two people save on the same draft and one sends first, the other's
`send` is refused and their `receive` is too ("doesn't merge"). They then
choose:

- receive the other person's saves into a **new draft**
  (`duh receive origin --draft main --into theirs`), compare, and carry on from
  whichever is right; or
- overwrite the place with their own (`duh send --force`), which discards
  the other person's saves *on that place*. They still have them locally.

Teams avoid this by taking turns on a draft, or by each working on their
own draft. See [Working together](guides/working-together.md).

### Diffs are byte strips, not lines

`duh compare` shows where in a file bytes were added, removed and kept:

```console
$ duh compare
body.FCStd
Dif[=================-+=========================================]
  +23.8 KB  -23.8 KB  =24.1 MB
```

There's no text diff, even for text files. duh treats every file the
same way.

### Storage: edit scripts, not whole blobs and packfiles

git stores each version of a file whole, then compresses similar objects
into packfiles later. duh stores each version as an *edit script* against
the version before, built from content-defined chunks:

```
UNCHANGED 6.8 MB · DELETED 40 KB · ADDED 2.1 MB · UNCHANGED 15.2 MB
```

Only ADDED bytes take new space, from the moment you save. Every 100
saves of a file (the `solid_after` setting), one version is also stored
whole as a *solid copy*, so reading an old version never replays more
than about 100 scripts. There's no `gc` or `repack`. See
[How duh stores things](FORMAT.md).

### Places are storage, not servers

A remote in git needs a git server: SSH, a forge, or `git daemon`. A duh
place is any storage duh can write to directly: a folder, a USB drive,
S3, Google Cloud Storage, Azure, WebDAV, SFTP, Google Drive, Dropbox and
[many more](../README.md#sharing). Nothing runs on the other end, and
the first `send` to an empty location creates the space there.

Credentials for a place are stored **in plain text** in `.duh/config`.

### Hidden files aren't tracked unless you ask

Anything whose name or folder starts with `.` is skipped by `look` and
`pick .` (so `.duh`, `.DS_Store` and editor folders stay out). Naming one
explicitly, as in `duh pick .duhignore`, tracks it like any other file.

`.duhignore` works like `.gitignore` for the common cases: `*.bak`
anywhere, `renders/` for folders, `/top.txt` anchored to the root, `!` to
let a file back in. Because it starts with a dot, it isn't shared unless
you pick it. See [Keeping files out](guides/ignoring-files.md).

### Smaller differences

- **No detached-HEAD dance.** Look at an old version with
  `duh inspect <version>`, get its files back with
  `duh bring-back <file> --from <version>`, or start a draft there with
  `duh drafts <name> <version>`.
- **`rewind` with no flag is like `reset --mixed`**: the draft moves, your
  files stay as they are, and nothing is picked.
- **`open` refuses unsaved changes** instead of carrying them across. Save,
  un-pick or bring back your files first.
- **Settings are per space.** There's no global config yet. A new space
  takes your account's user name; set `user.name` and `user.email` with
  `duh settings set`.
- **Labels aren't shared.** There's no `push --tags` yet: labels stay in
  the space they were made in.
- **No hooks, submodules or sparse checkouts.**
- **The journal survives deleting a draft.** `duh journal -b <deleted>`
  still lists where it was.
- **There's a desktop app.** [duh Studio](reference/studio.md) runs the
  same engine as the CLI, and the two can work on one space at once.

## A git workflow, in duh

```console
$ duh start
$ duh pick .
$ duh save -m "Import the base model"
$ duh open --new lighting           # git switch -c lighting
$ duh save -a -m "Three-point lighting"
$ duh open main                     # git switch main
$ duh bring-back scene.blend --from lighting   # take one file across
$ duh save -a -m "Lighting from the lighting draft"
$ duh places add origin s3://studio/shot-12 -o region=eu-west-1
$ duh send                          # git push origin main
$ duh label delivered-v1            # git tag
```
