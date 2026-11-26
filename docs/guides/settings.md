# Big files and settings

Each space keeps its settings in `.duh/config`. Read and change them with
`duh settings`:

```console
$ duh settings get user.name
dan
$ duh settings set user.name "Dan Habot"
set user.name = Dan Habot
```

Settings belong to one space. There's no global setting yet, so a new
space starts with the defaults below.

## Your name on saves

Every save records who made it. A new space uses your computer account's
name, with no email. To change them:

```console
$ duh settings set user.name "Ana Lee"
$ duh settings set user.email "ana@example.com"
```

The new name applies to saves from now on. Saves already made keep the
name they were made with, since changing them would change their hashes.

## How big files are stored

When you save, duh cuts each file into pieces at points chosen by its
content (see [How duh works](../concepts.md#how-saving-only-the-changes-works)),
and stores a new version as "these pieces unchanged, these removed, these
added". Three settings control that:

| Setting | Default | What it is |
|---|---|---|
| `chunk_size` | `4096` (4 KB) | the average piece size, in bytes |
| `max_size` | `10485760` (10 MB) | the largest piece of new data stored as one object |
| `solid_after` | `100` | saves of a file between *solid copies* (whole versions kept so old versions stay fast to read) |

The defaults suit files from kilobytes to a few gigabytes. You can change
any of them at any time. Old saves stay readable, and each new save uses
the settings in force when it's made.

### chunk_size: precision against speed

Smaller pieces mean a small edit costs less: change one byte and duh
stores about one piece. Larger pieces mean fewer of them to keep track of,
so saving very large files is faster and needs less memory.

| chunk_size | One small edit costs about | Good for |
|---|---|---|
| 4096 (default) | 4 KB | most creative files |
| 65536 | 64 KB | files of tens of GB |
| 262144 to 1048576 | 256 KB to 1 MB | single files of hundreds of GB or more |

```console
$ duh settings set chunk_size 65536
```

Saving a change to a file holds a list of every piece of it in memory.
Measured at the default size, a small edit to a 1 GB file needed about
170 MB of memory; that grows with the file, so a 100 GB file would need
around 16 GB. At 64 KB pieces it's about a sixteenth of that. Saving a
*new* file needs very little memory at any size.

### solid_after: how fast old versions open

Each version is stored as changes to the one before, so opening an old
version replays those changes. Every `solid_after` saves of a file, duh
also keeps one version whole, so opening any version replays at most
about that many steps.

A solid copy costs the whole size of the file. For very large files that
change often, raise it to save space (`duh settings set solid_after 500`);
lower it if opening old versions feels slow. The smallest allowed value
is 2.

### max_size

New data is stored in objects of at most `max_size` bytes. Most people
never need to change it. Raising it means fewer, larger files inside
`.duh`, which some cloud storage handles faster.

## Files that don't shrink

Saving only the changes works when an edit changes a small part of the
file. Some formats rewrite everything on every save:

- compressed archives (`.zip`, `.7z`, `.tar.gz`);
- compressed media (`.mp4`, `.mov`, `.jpg`, `.mp3`);
- some application formats that compress internally.

duh keeps every version of those correctly, but each costs close to its
full size. Where your program offers it, saving without compression (for
example an uncompressed `.tar`, or a "don't compress" option) lets duh
store only what changed.

## How much space is duh using?

Everything duh stores is in the `.duh` folder of your space. Its size on
disk is the cost of your whole history. duh doesn't delete stored data
yet, even after you delete a draft or rewind, so the folder only grows;
clearing out data no save needs is planned.

## All settings

| Key | Example | Notes |
|---|---|---|
| `user.name` | `Ana Lee` | name on new saves |
| `user.email` | `ana@example.com` | optional |
| `chunk_size` | `4096` | bytes, at least 64 |
| `max_size` | `10485760` | bytes, at least 4096 |
| `solid_after` | `100` | at least 2 |
| `remote.<place>.url` | `s3://bucket/path` | a place's location (`duh places set-url` does the same) |
| `remote.<place>.config.<option>` | `region` = `eu-west-1` | storage options for a place, as given with `-o` |
| `core.bare` | `false` | `true` for a history-only space; set by `duh start --bare` |
