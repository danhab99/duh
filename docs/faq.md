# Questions and problems

## Common errors

**`not inside a duh space`**
You ran a command outside a space's folder. `cd` into the project, or
start a space there with `duh start`.

**`nothing to save: pick some changes first`**
Nothing is picked. Pick files (`duh pick <file>`, or `duh pick .` for
everything), or use `duh save -a` to pick every changed tracked file in
one go. In duh Studio, **Save** with nothing ticked picks everything for
you.

**`you have unsaved changes; save them, un-pick them or bring back your files before opening another draft`**
Opening a draft replaces your files with that draft's, so duh won't do it
while you have work that isn't saved. Save it (it stays safe on the
current draft), or throw it away with `duh bring-back <file>`.

**`you have unsaved changes; save or bring back your files before receiving`**
The same, for receiving: new saves would change your files. Save first.

**`origin/main has saves you don't; `duh receive origin` first (or --force to discard them)`**
Someone sent saves you haven't received. Receive them first. If
receiving then says *doesn't merge*, both of you saved: see
[When both sides saved](guides/working-together.md#when-both-sides-saved).

**`your draft 'main' has saves that origin/main doesn't; duh doesn't merge…`**
You and the place have both saved since you last met. Nothing is lost;
see [When both sides saved](guides/working-together.md#when-both-sides-saved).

**`there's no draft, label or save called '…'`** /
**`no version, draft or label matches '…'`**
The version name is misspelled, or the hash prefix doesn't match any
save. `duh history --oneline`, `duh drafts` and `duh label` list what
exists.

**`'9298' matches 2 versions; use more of the hash`**
Give a few more characters of the hash.

**`'HEAD~9' goes back past the first save`**
There aren't that many saves on the draft. `duh history --oneline`
counts them.

**`remote 'usb' (…) holds files but no duh space`**
The place's folder or bucket path already has other files in it, so duh
won't write there. Point the place at an empty folder, or a new path in
the bucket: `duh places set-url usb /media/usb/bracket-v3`.

**`another duh is changing this space right now; try again in a moment`**
duh Studio or another terminal is in the middle of a save, send or
similar on the same space, and didn't finish within ten seconds. Wait for
it to finish and try again.

**`--keep-picked needs nothing picked yet; save or un-pick first`**
`rewind --keep-picked` picks the undone changes, which can't be combined
with picks you already have. Save or `duh unpick` them first.

**`chunk_size (bytes) must be a whole number of at least 64`** (and the
same for `max_size` and `solid_after`)
That value would stop the space from opening, so duh didn't write it. See
[Big files and settings](guides/settings.md#all-settings).

## When check finds a problem

`duh check` reads back everything and lists each problem, like
`stored object 5b3e0c2a9b37 is damaged: its bytes have changed`, or a
file in a particular save that no longer rebuilds exactly.

Damage means something outside duh changed the files in `.duh`: a failing
disk, a sync tool rewriting them, an interrupted copy. duh doesn't repair
spaces yet. If you have a place with a good copy (check it with
`duh check <place>`), get a fresh copy from it with `duh get` and carry
on there, or bring individual files back from it.

This is why checking backups matters: run `duh check <place>` now and
then, not only when something has gone wrong.

## Questions

**Can two people edit the same file at once?**
Each person edits their own copy. What duh can't do is combine two
people's edits into one file; nothing can, for most binary formats. See
[Working together](guides/working-together.md) for ways to share a
project.

**Is my work uploaded anywhere?**
No. Everything stays in your project's `.duh` folder until you send it to
a place you added yourself.

**Can I use duh for code too?**
Yes; it keeps any file. But for code, git's line-by-line diffs and
merging are better tools. duh is for files git handles badly.

**Can I use duh and git in the same folder?**
Yes. duh ignores `.git` (it's hidden), and you can add duh's files to
`.gitignore` (`.duh/`). Many projects keep code in git and large assets
in duh.

**How do I move a space to another folder or computer?**
Move or copy the whole project folder, `.duh` included. Or send to a
place and `duh get` it on the other side.

**How do I stop using duh on a folder?**
Delete its `.duh` folder. Your files stay as they are; the history goes
with it, so send it somewhere first if you might want it.

**Does duh delete old data I don't need?**
Not yet. The `.duh` folder only grows, even after you delete a draft.
Clearing out data no save needs is planned.

**Why doesn't my label show up for others?**
Labels aren't sent yet; they stay in the space they were made in.

**What does "solid copy" mean in `duh check`'s output?**
A whole version of a file that duh keeps every `solid_after` saves so old
versions open quickly. See
[Big files and settings](guides/settings.md#solid_after-how-fast-old-versions-open).

**Why does a new file show as all added?**
There's nothing earlier to compare it with: every byte is new.

**Why did a small edit cost a lot of space?**
The file format probably compresses its contents, so a small edit
rewrites the whole file. See
[Files that don't shrink](guides/settings.md#files-that-dont-shrink).
