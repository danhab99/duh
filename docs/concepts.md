# How duh works

This page is for anyone who has never used version control. If you know
git, [duh for git users](for-git-users.md) is shorter.

## The problem

You've probably done this:

```
poster.psd
poster-v2.psd
poster-v2-final.psd
poster-v2-final-REALLY.psd
poster-v3-client-changes.psd
```

Every copy is a whole file. Five copies of a 400 MB poster is 2 GB, most
of it identical. You can't tell which one went to the printer, and the
one you need is always the one you overwrote.

duh replaces that with one file, `poster.psd`, and a list of every version
of it you chose to keep, each with a note about what changed. Any version
can come back, exactly as it was. And keeping a version only costs the
bytes that changed, so a hundred versions of a 400 MB file can take far
less than a hundred times 400 MB.

## A space

A **space** is a folder duh looks after. When you start one, duh adds a
hidden `.duh` folder inside it. Every version lives there. Your own files
stay exactly where they were, and you keep working on them in your usual
programs. duh never changes them unless you ask it to.

Nothing leaves your computer unless you send it somewhere (see
[Places](#places)).

## Saving: pick, then save

Saving a version takes two steps:

1. **Pick** the files that go into it.
2. **Save**, with a sentence about what changed.

Why pick? Because you don't always want everything. Maybe you changed the
model *and* fiddled with a render setting you're not sure about. Pick just
the model, save it as "Thicker walls", and the render setting waits for
another day. In duh Studio you pick with the tick boxes beside each file;
**Pick all** picks everything.

A **save** is a snapshot of every file duh tracks, at that moment: the
files you picked as they are now, and every other file as it was in the
save before. Saves are never edited once made. Each one gets:

- your note ("Thicker walls on the body");
- your name and the time;
- a **hash**, a long code like `9298ff3be620…` that names that exact
  version. You can use the first few characters of it anywhere duh asks
  for a version.

Picking takes a copy of the file as it is when you pick it. If you keep
editing after picking, duh tells you ("changed again since you picked
it"); pick it again to include the newer edits.

## What changed, and how much

duh can't show you a line-by-line difference the way text tools do, since
a 3D model or an image isn't made of lines. Instead it shows a **change
strip**: a bar as long as the file, coloured by what happened to each part
of it.

- **green**: bytes that are new;
- **red**: bytes that were removed;
- **grey**: bytes that didn't change.

A strip that's nearly all grey with a sliver of green means a small edit.
An all-green strip means a new file, or one that changed completely.

On the command line the strip is a row of characters: `+` added, `-`
removed, `=` unchanged.

## How saving only the changes works

When you save, duh cuts each file into small pieces, a few kilobytes each,
at points chosen by the file's own content. If you change something in the
middle of a file, only the pieces around the change are different. duh
keeps the new pieces and, for everything else, just a note saying "same
as before".

Two things make this work well for creative files:

- **Edits anywhere.** Inserting something near the start of a file
  doesn't make every later piece look different. The cut points move with
  the content.
- **Renames are free.** Moving `body.FCStd` to `parts/body.FCStd` with
  duh's rename costs almost nothing.

It works less well for files that are compressed as a whole, like `.zip`,
`.mp4` or `.jpg`. A small change in them reshuffles every byte after it,
so each version costs close to its full size. duh still keeps them
correctly, just without the savings.

## Drafts

Your saves form a line, newest first. That line is a **draft**. Every
space starts with one, called `main`.

When you want to try something risky, start a new draft. Saves on it don't
touch `main`. If the idea works, keep going there. If it doesn't, open
`main` again and your files go back to how they were, with the experiment
still kept in its draft in case you change your mind.

duh never combines two drafts into one. You choose which one to continue,
and you can bring individual files across from one to the other. See
[Drafts](guides/drafts.md).

## Labels

A **label** is a name for one save: `final-v2`, `sent-to-printer`,
`approved`. Labels never move. Use them for the versions you'll want to
find again.

## Places

A **place** is somewhere else a copy of your space lives: a folder on a
USB drive or NAS, or cloud storage like Amazon S3 or Google Cloud
Storage. You **send** your saves to a place and **receive** other people's
from it. There's no server to set up; duh writes to the storage directly.

Places are how you back up and how you share. See
[Sharing and backups](guides/sharing.md).

## Nothing is lost by accident

- duh never changes your files without asking, and warns before anything
  that would throw away work.
- Every time a draft moves (a save, an undo, a receive), duh writes it down
  in that draft's **journal**. If you undo too far or delete a draft, the
  journal still knows where the work was. See
  [Getting old work back](guides/old-versions.md#recover-lost-work).
- **Check** reads back every version of every file and confirms each is
  exactly what was saved, byte for byte. Run it on a place before you
  trust a backup.

## The words, all together

| Word | Meaning |
|---|---|
| space | a folder duh keeps versions of |
| pick | choose a file for the next save |
| save | a kept version, with a note, a name and a time |
| change strip | what was added, removed and kept in a file |
| hash | the code naming one exact version, like `9298ff3be620` |
| draft | a line of saves; `main` is the first |
| label | a fixed name for one save |
| place | another copy of the space: a drive, a NAS, the cloud |
| send / receive | copy saves to / from a place |
| journal | every point a draft has been at, for recovering work |
| check | prove every save reads back exactly |

Next: [Getting started](getting-started.md).
