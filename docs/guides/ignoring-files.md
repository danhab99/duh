# Keeping files out

Most project folders hold things you don't need versions of: autosaves and
backups your programs write, renders you can make again, caches. Keep
them out of duh with a `.duhignore` file, so they don't fill up `look`,
**Your files** in Studio, or `duh pick .`.

## Make a .duhignore

Create a plain-text file called `.duhignore` in the top folder of your
space. Each line is a pattern:

```
# FreeCAD and Blender backups
*.FCBak
*.blend1

# Renders: made from the scene, no need to keep them…
renders/*
# …except the cover
!renders/cover.png

# Only this file in the top folder, not ones with the same name deeper down
/scratch.txt
```

| Pattern | Leaves out |
|---|---|
| `*.FCBak` | every file ending in `.FCBak`, in any folder |
| `renders/` | every folder called `renders`, and everything in it |
| `renders/*` | everything inside `renders`, while still allowing exceptions |
| `/scratch.txt` | `scratch.txt` in the top folder only |
| `cache/*.tmp` | `.tmp` files inside a `cache` folder |
| `!keep.FCBak` | lets `keep.FCBak` back in, even though `*.FCBak` matches it |
| `# …` | a comment; blank lines are ignored too |

The rules are the same as git's `.gitignore`, if you know those,
including one catch: a file can't be let back in with `!` if its whole
folder is left out. Write `renders/*` rather than `renders/` when you want
exceptions inside it, as above.

## Hidden files are left out already

Files and folders whose names start with a dot (`.DS_Store`, `.git`,
`.vscode`, `.duh` itself) are never offered. You don't need to list them.

## What ignoring does and doesn't do

- Ignored files don't appear in `look` or in Studio, and `pick .` and
  **Pick all** skip them.
- **Naming a file still picks it.** `duh pick renders/cover.png` picks it
  even if a pattern matches. Use that for the odd exception.
- **Already-tracked files stay tracked.** Adding a pattern doesn't remove
  a file duh is already keeping versions of. To stop tracking one but keep
  it on disk: `duh forget <file>`.

## Sharing your .duhignore

`.duhignore` starts with a dot, so it's hidden too: duh doesn't save it
unless you ask. To share it with everyone who gets a copy of the space,
pick it like any file:

```console
$ duh pick .duhignore
$ duh save -m "Ignore backups and renders"
```
