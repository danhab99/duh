# How duh stores things

This describes a space as it sits on disk or in a bucket, for anyone
writing tools that read or serve spaces (duh.space, backups, viewers).
The reference implementation is `lib/` in this repository; where this
document and the code disagree, the code wins and this document has a
bug.

The format is not yet stable. It may change before 1.0.

## Where a space lives

A space with working files keeps everything in a `.duh` folder at its
top. A *bare* space (`duh start --bare`, or a place created by a first
`send`) is that same content at the root of its folder or bucket path,
with no working files around it.

A location holds a space when it has a `config` file.

```
config            settings (TOML)
index             what's picked for the next save
refs/
  HEAD            the open draft's name (or a save's hash, when not on a draft)
  main            a draft: the hash of its latest save (empty before the first)
  tags/final-v2   a label: the hash of its save
reflog/
  main            the draft's journal
  HEAD
objects/
  0f2/137/92a/a5f1b2…   one object per file, named by its hash
solid/
  <content hash>  the hash of the solid copy of that content
statcache         local only: cached hashes of working files
lock              local only: held while a command changes the space
.tmp/             local only: scratch space for writing files whole
```

`statcache`, `lock` and `.tmp/` are never copied by `send` or `get`.

## Hashes

Every hash is SHA-256, written as 64 lowercase hex characters. Inside
objects a hash is stored as a MessagePack `bin` of those 64 ASCII
characters (not the 32 raw bytes). The empty hash, meaning "none" (no
parent, no file), is 64 zero bytes.

Short forms such as `0f213792aa5f` are any unique prefix of the hex.

## Objects

An object is a MessagePack value, stored at
`objects/<hex 0..3>/<hex 3..6>/<hex 6..9>/<hex 9..>`, where the hex is the
SHA-256 of the stored bytes. Objects are immutable: the same content
always has the same name, and nothing is ever rewritten.

Each object is a map with one key, the object's kind, whose value is the
object's fields as an array, in the order below (Rust serde's externally
tagged enums, with `rmp-serde`'s default struct-as-array encoding).

### Commit: a save

```
{"Commit": [parent, tree, message, committer, author]}
```

| Field | |
|---|---|
| `parent` | hash of the previous save, or the empty hash for the first |
| `tree` | hash of the top Tree |
| `message` | string |
| `committer`, `author` | `[name, email, timestamp]`: strings, and seconds since 1970 (UTC) |

Every save has exactly one parent. There are no merges.

### Tree: a folder

```
{"Tree": [[[name, mode, hash], …]]}
```

Entries are files (`mode` 0o100644, `hash` a File) and subfolders
(`mode` 0o40000, `hash` another Tree).

### File: one version of one file

```
{"File": [content_hash, [fragment, …], base_path?]}
```

| Field | |
|---|---|
| `content_hash` | SHA-256 of the file's whole content, for checking a rebuild |
| `fragments` | hashes of FileDiffFragment objects, in order |
| `base_path` | optional: the path this version was diffed against, when it isn't the file's own path (after a rename). Absent otherwise |

### FileDiffFragment: one step of an edit script

```
{"FileDiffFragment": {"ADDED": [body, len]}}
{"FileDiffFragment": {"UNCHANGED": [len]}}
{"FileDiffFragment": {"DELETED": [len]}}
```

`body` is the hash of a Fragment holding `len` new bytes (at most the
space's `max_size`).

### Fragment: stored bytes

```
{"Fragment": bin}
```

### Solid: a whole version, kept for speed

```
{"Solid": [content_hash, [[fragment, len], …]]}
```

The content's bytes, as Fragment hashes and lengths in order. No save
refers to a Solid; `solid/<content hash>` finds it.

## Rebuilding a file

To read `path` as of save `S`:

1. Walk `S`'s tree to find `path`'s File version `V`.
2. Find the version `V` was diffed against, its **base**: starting from
   `S`'s parent, walk back past every save in which `path` is still `V`
   (saves that carried it forward unchanged). In the first save reached
   where it isn't, the base is the version of `base_path` (or `path`)
   there. If there's no such version, or no such save, the base is empty.
3. Rebuild the base the same way (recursively), then apply `V`'s
   fragments to it in order, with a cursor into the base:
   - `UNCHANGED len`: copy `len` bytes from the base at the cursor;
     advance the cursor by `len`;
   - `DELETED len`: advance the cursor by `len`;
   - `ADDED body len`: output the Fragment `body`'s bytes; the cursor
     doesn't move.
4. The output's SHA-256 must equal `V`'s `content_hash`.

**Shortcut:** before recursing, look for `solid/<content_hash>` of any
version on the way. If one exists, its Solid's fragments, concatenated,
are that version's content, and the walk can stop there.

## How saves are made

For readers this is background; for writers it's what keeps stored data
compatible.

- A file is split into content-defined chunks with a gear hash, aiming at
  `chunk_size` bytes on average and never longer than
  min(8 × `chunk_size`, `max_size`).
- The new version's chunks are matched in order against the base's,
  each against the earliest equal chunk at or after the previous match.
  Skipped base bytes become DELETED, matched ones UNCHANGED, unmatched new
  ones ADDED. Adjacent steps of one kind are joined.
- ADDED runs are stored as Fragments of at most `max_size` bytes, each
  with its own ADDED step.
- After a save, if a file has `solid_after` or more saves since its last
  solid copy, the middle one of them gets a Solid.

## Text files

**`config`** is TOML:

```toml
chunk_size = 4096
max_size = 10485760
solid_after = 100          # optional, default 100

[core]
bare = false

[user]
name = "Ana Lee"
email = ""

[remote.origin]
url = "s3://bucket/path"
[remote.origin.config]     # storage options, given to OpenDAL as-is
region = "eu-west-1"
```

**`index`** lists picked files, one per line, as `path = "<File hash>"`.
A File hash of 64 zero characters means the file's deletion is picked.

**`refs/*`** hold a hash in hex, or (`HEAD` only) a draft name. An empty
draft file means the draft has no saves yet.

**`reflog/<draft>`** has one line per move, oldest first:
`<hex hash> | <why>`, where *why* is `save: <first line of message>`,
`rewind to <hash>`, `receive from <place>/<draft>`, `send: <draft>`,
`draft: created`, `open: new draft` and so on.

## Sharing

A send copies every object a save needs that the place doesn't have, and
any solid copies (with their `solid/` entries) it lacks, then writes the
place's draft ref, then appends to its journal. A receive is
the reverse. Writing objects before the ref means a reader never sees a
draft pointing at a save whose objects are missing. Labels, `index`,
`statcache` and settings are not sent.
