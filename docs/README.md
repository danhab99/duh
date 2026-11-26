# duh's documentation

duh keeps every version of your files: Blender scenes, FreeCAD models,
Photoshop and GIMP images, audio and video projects, anything. Each save
stores only what changed, and every version comes back exactly.

## Where to start

**New to version control?** Start with [How duh works](concepts.md): what
a save is, why you pick files, what drafts and places are. Then follow
[Getting started](getting-started.md), which takes one project from its
first save to a shared, checked history.

**Know git?** Read [duh for git users](for-git-users.md). It maps every git
command you know to duh's, and lists the handful of places duh works
differently (mostly: it never merges).

## Guides

Each guide covers one job from start to finish, in the app and on the
command line.

| Guide | For when you want to… |
|---|---|
| [Getting old work back](guides/old-versions.md) | open, copy or restore any earlier version; undo a save; recover something you thought was gone |
| [Drafts](guides/drafts.md) | try an idea without risking the version that works |
| [Sharing and backups](guides/sharing.md) | keep a copy on a USB drive, NAS or cloud storage, and work with other people |
| [Working together](guides/working-together.md) | share one project with a team, taking turns without overwriting each other |
| [Keeping files out](guides/ignoring-files.md) | stop duh from offering backups, renders and caches |
| [Big files and settings](guides/settings.md) | your name on saves, and tuning duh for very large files |

## Reference

- [Commands](reference/commands.md): every command and option on the
  command line.
- [duh Studio](reference/studio.md): the app's window, shortcuts and
  settings.
- [Questions and problems](faq.md): what an error means, and what to do.

## For people working on duh

- [Contributing](../CONTRIBUTING.md): building, testing, and how changes
  are made.
- [How duh stores things](FORMAT.md): the on-disk format, for anyone
  writing tools that read spaces.
- [duh Studio's design](studio/DESIGN.md).
