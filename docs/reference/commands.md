# Commands

Every command, with every option. On the command line,
`duh <command> --help` prints the same, and with duh installed through Nix
(or Home Manager), `man duh` and `man duh-<command>` (for example
`man duh-save`) are the manual pages.

Each command also answers to its git name, shown in
brackets.

Paths are relative to the folder you run the command in, and can be
anywhere inside the space. **Versions** can be written as a label, a
draft, a short hash, `HEAD`, `HEAD~N` or `X^`; see
[Naming a version](../guides/old-versions.md#naming-a-version).

- [Starting](#starting): `start`, `get`
- [Seeing what changed](#seeing-what-changed): `look`, `compare`
- [Saving](#saving): `pick`, `unpick`, `save`
- [Files](#files): `bring-back`, `rename`, `delete`, `forget`
- [History](#history): `history`, `inspect`, `journal`, `label`, `rewind`
- [Drafts](#drafts): `drafts`, `open`
- [Sharing](#sharing): `send`, `receive`, `places`
- [Upkeep](#upkeep): `settings`, `check`

---

## Starting

### duh start  [init]

Start a space in the current folder.

| Option | |
|---|---|
| `--bare` | history only, no working files: for a place to share through |

### duh get <url> [folder]  [clone]

Copy a space into a new folder. The new space has the source added as the
place `origin`.

| Argument / option | |
|---|---|
| `<url>` | where the space is: a folder, `file://`, or a storage URL such as `s3://bucket/path` |
| `[folder]` | the folder to create (default: named after the URL) |
| `-d, --draft <draft>` | the draft to open (default: the one the space is on) |
| `--bare` | history only, no working files |
| `-o, --option <key=value>` | a storage option or credential, saved in the new space's settings; repeat for more |

```console
$ duh get /mnt/nas/poster
$ duh get s3://studio/poster my-poster -o region=eu-west-1 -o access_key_id=… -o secret_access_key=…
```

## Seeing what changed

### duh look  [status]

List files by state: picked for the next save, changed but not picked,
missing, new (not tracked yet), and saved and unchanged. Hidden files and
files matched by `.duhignore` aren't listed.

### duh compare [paths…]  [diff]

How much of each file changed: a bar of 60 columns (`=` kept, `-`
removed, `+` added) and the totals.

| Argument / option | |
|---|---|
| `[paths…]` | only these files |
| `--picked` | compare what's picked with the last save, instead of your un-picked edits |

```console
$ duh compare
body.FCStd
Dif[=================-+=========================================]
  +23.8 KB  -23.8 KB  =24.1 MB
```

## Saving

### duh pick [paths…]  [add, stage]

Choose what goes into the next save. Picking takes the file as it is
now; edit it again and `look` shows it as both picked and changed.

| Argument / option | |
|---|---|
| `[paths…]` | files, folders (`.` for everything) or patterns like `*.psd` |
| `-A, --all` | every change: new, changed and deleted files |
| `-u, --update` | changed and deleted tracked files only, no new ones |

Naming a missing tracked file picks its deletion. Naming a hidden or
ignored file picks it anyway.

### duh unpick <paths…>  [unstage, restore --staged]

Take files out of the next save. Your edits stay.

### duh save [file]  [commit]

Save a new version of what's picked.

| Argument / option | |
|---|---|
| `[file]` | pick this one file and save it, in one step |
| `-m, --message <text>` | what changed, in a sentence |
| `-g, --generate` | write the message from what changed ("Update 2 files" and a line per file) |
| `-a, --all` | pick every changed or deleted tracked file first |
| `--allow-empty` | save even if nothing changed |

Without `-m`, duh opens the editor named in your `EDITOR` environment
variable for the message, as git does. If `EDITOR` isn't set, or with
`-g`, it writes the message itself and shows it.

```console
$ duh save -a -m "Thicker walls on the body"
$ duh save notes.txt -m "Notes from the client call"
```

## Files

### duh bring-back <paths…>  [restore, checkout]

Bring back files as they were. With no `--from`: the picked version if
the file is picked, otherwise the last save.

| Argument / option | |
|---|---|
| `--from <version>` | bring back the file as it was in this version |
| `--as <path>` | write it here instead of over your copy (one file only) |

```console
$ duh bring-back body.FCStd                    # undo unsaved edits
$ duh bring-back body.FCStd --from final-v1 --as body-final-v1.FCStd
```

### duh rename <from> <to>  [mv, move]

Rename or move a tracked file, on disk and in the next save. Costs almost
nothing in storage. `<to>` can be an existing folder.

### duh delete <paths…>  [rm]

Delete files from the folder and stop tracking them. Earlier versions
stay in history.

| Option | |
|---|---|
| `-f, --force` | delete even if the files have unsaved changes |
| `--cached`, `--keep` | keep the files on disk (same as `forget`) |

### duh forget <paths…>  [rm --cached]

Stop tracking files, but leave them on disk. Add them to `.duhignore` to
keep them out of `look`.

## History

### duh history [version]  [log]

Your saves, newest first.

| Argument / option | |
|---|---|
| `[version]` | start from this version instead of the last save |
| `-n, --max <n>` | show at most this many |
| `--oneline` | one line per save: short hash and message |

### duh inspect [version]  [show]

One save in detail: its hash, the save before it, who made it and when,
its message, and every file in it. Default: the last save.

### duh journal  [reflog]

Every point the draft has been at, oldest first, with why it moved:
`save: …`, `rewind to …`, `receive from …`. Use it to find saves that
are no longer on any draft.

| Option | |
|---|---|
| `-b, --draft <draft>` | another draft's journal, including a deleted one |

### duh label [name] [version]  [tag]

Name a version (default: the last save). With no name, list labels.

| Option | |
|---|---|
| `-l, --list` | list labels |
| `-d, --delete <name>` | remove a label (the save stays) |

Labels stay in this space: sending doesn't copy them.

### duh rewind [version]  [reset]

Move the open draft to another version (default: the last save).

| Option | Your files | Picked |
|---|---|---|
| *(none)* | unchanged | nothing |
| `--keep-picked` (`--soft`) | unchanged | the undone changes, ready to save again; needs nothing picked beforehand |
| `--discard` (`--hard`) | made to match the version: **unsaved work is lost** | nothing |

The saves rewound past stay in the journal.

## Drafts

### duh drafts [name] [start]  [draft, branch]

With no arguments, list drafts (`*` marks the open one). With a name,
start a draft there without opening it.

| Argument / option | |
|---|---|
| `[name]` | start a draft with this name |
| `[start]` | the version to start it at (default: where you are) |
| `-m, --move <name>` (`--rename`) | rename the open draft |
| `-d, --delete <name>` | delete a draft (not the open one); its journal stays |
| `-s, --set <version>` | point the open draft at another version without touching your files |

### duh open <draft>  [switch]

Open a draft, changing your files to match it. Refuses if you have
unsaved changes.

| Option | |
|---|---|
| `-n, --new` | start the draft where you are, then open it |

## Sharing

### duh send [place]  [push]

Send your saves to a place (default: `origin`). Only ever moves the
place's draft forward, unless you force it. The first send to an empty
folder or bucket creates the space there.

| Option | |
|---|---|
| `-d, --draft <draft>` | the draft to update on the place (default: the same name as yours) |
| `--force` | overwrite the place's draft even if that throws away saves made there |

### duh receive [place]  [pull]

Receive new saves from a place (default: `origin`). Only ever moves your
draft forward, never merges, and refuses to run over unsaved changes in
the draft you have open. Receiving into the open draft updates your
files.

| Option | |
|---|---|
| `-d, --draft <draft>` | the draft to receive from the place (default: the same name as yours) |
| `--into <draft>` | your draft to update (default: the open one); a new name starts a draft |

### duh places  [place, remote]

| Command | |
|---|---|
| `duh places list` (`ls`) | list places; `-v` shows where each one is |
| `duh places add <name> <url> [-o key=value]…` | add a place; options and credentials are stored **in plain text** |
| `duh places remove <name>` (`rm`) | forget a place; nothing stored there is deleted |
| `duh places rename <old> <new>` | rename a place |
| `duh places get-url <name>` | show where a place is |
| `duh places set-url <name> <url>` | change where a place is |

Supported storage: see [Sharing and backups](../guides/sharing.md#cloud-storage).

## Upkeep

### duh settings  [config]

| Command | |
|---|---|
| `duh settings get <key>` | show a setting, e.g. `user.name` |
| `duh settings set <key> <value>` | change one |

Every setting is listed in [Big files and settings](../guides/settings.md#all-settings).

### duh check [place]  [fsck]

Prove every save is intact: read every stored piece and re-hash it,
follow every link, and rebuild every version of every file, comparing
each with the hash recorded when it was saved. Also checks picked files
and solid copies. Exits with an error and lists the problems if anything
is wrong.

| Argument | |
|---|---|
| `[place]` | check a place instead of this space, e.g. a backup |

## Exit status and output

duh exits with 0 on success, 1 when a command fails (with the reason on
standard error), and 2 when the command itself is mistyped.

Colour, and the live progress bars while picking, are used only when
output goes to a terminal, and never when the `NO_COLOR` environment
variable is set.

Only one command changes a space at a time. If another is busy with it
(a second terminal, or duh Studio), duh waits up to ten seconds for it to
finish.
