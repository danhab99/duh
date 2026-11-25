# duh Studio — design and plan

duh Studio is the desktop app for duh: everything the CLI does, for people
who'd rather not open a terminal. Flutter, standalone, built and launched by
one command:

```sh
nix run .#studio
```

This document is the spec. Its plan (at the end) has checkboxes and is kept
up to date as work lands, so it can be picked up across sessions.

---

## 1. Principles

1. **One engine.** The CLI and Studio do things the same way because they run
   the same code: every operation lives in `lib`, and both are thin shells over
   it. Nothing that decides *what happens* lives in the CLI or the app.
2. **duh's own words.** Spaces, drafts, saves, picks, labels, places, the
   journal. Never "commit", "branch", "stage" or "push" in the interface.
3. **Show, don't make people read.** Every change is drawn as a *change strip*
   (the CLI's `=+-` bar, made visual), every save has a time and a face, and
   every risky action says in a sentence what it will do before it does it.
4. **Nothing is lost by accident.** Anything that throws work away confirms
   first and names what goes; the journal is one click away to recover.
5. **Fast with big files.** The app never blocks on a 2 GB file: long work
   runs off the UI thread, shows progress, and the file list stays responsive.

---

## 2. What SmartGit teaches (and where duh should differ)

From SmartGit's design (summarised from general knowledge of the product,
not a fresh review):

| SmartGit idea | Keep? | For duh |
|---|---|---|
| **Files view** filtered by state (changed, untracked, ignored…) | Yes | "Your files": changed / new / missing / picked, unchanged hidden by default |
| **Single window**: history graph with a "Local Files" row on top | Yes, the core idea | The timeline's top card is *Your files (not saved)*; every save below it |
| **Journal**: local commits not yet pushed | Yes | Saves not yet sent show a "not sent" mark, and the sync pill counts them |
| **Branches view** with checkboxes to show in the graph | Simplified | A drafts list; the open draft is highlighted, others show where they split off |
| **Changes view**: side-by-side text diff | Replaced | Binary files have no lines: a change strip (where bytes were added, removed, kept) plus an image preview when the file is an image |
| Merge, rebase, cherry-pick, stash, conflict solver | No | duh doesn't merge; diverged drafts get a clear explanation instead |
| Dense toolbars, dozens of menu items, tiny type | No | Few, large, labelled actions; a command palette (Ctrl+K) for the rest |
| Separate Log window | No | One window: selecting a save shows it in place |

### git vs duh, as it affects the GUI

| | git | duh | Consequence |
|---|---|---|---|
| History shape | DAG with merges | One parent per save | A timeline, not a graph; drafts drawn as simple lanes |
| Diffs | Lines of text | Bytes of any file | Change strips with byte counts, not line diffs |
| Conflicts | Central to the UI | Impossible | No conflict UI at all |
| Sharing | push/pull with merge | send/receive, fast-forward only | Sync pill: ↑ to send, ↓ to receive, or "diverged" with choices |
| Remotes | URLs over git protocols | Places on 34 storage backends with credentials | A place editor with fields per kind (folder, S3, …) |
| Integrity | `git fsck` (rarely used) | `duh check` | A health badge; check runs on demand and per place |
| Undo | reflog (expert tool) | journal | "Recover" view: everywhere a draft has been, in plain words |

---

## 3. Architecture

```
┌──────────────────────────────┐   ┌──────────────────────────────┐
│ duh (CLI)                    │   │ duh Studio (Flutter)         │
│ parse args, print results    │   │ screens, state, animation    │
└──────────────┬───────────────┘   └──────────────┬───────────────┘
               │ Rust calls                       │ Dart ⇄ Rust (flutter_rust_bridge)
               │                    ┌─────────────┴────────────────┐
               │                    │ studio/native (cdylib)       │
               │                    │ thin bridge: types + streams │
               │                    └─────────────┬────────────────┘
┌──────────────┴──────────────────────────────────┴────────────────┐
│ lib::ops — every operation, returning plain data, never printing │
├──────────────────────────────────────────────────────────────────┤
│ lib — space, objects, chunking, replay, transfer, check, store   │
└──────────────────────────────────────────────────────────────────┘
```

**Decision: bind `lib` directly with flutter_rust_bridge, not the CLI over
stdio.** This was your preference, and it's also the better design:

- One engine by construction: Studio and the CLI call the same `lib::ops`
  functions, so they can't drift apart.
- Typed data (a look report, a save, a change strip) crosses the boundary as
  structs, with no output format to design, parse and keep stable.
- Progress streams back naturally (`StreamSink`) for long picks, sends and checks.
- No child processes to manage, and no PATH problem (your machine has an old
  `duh` 0.2.1 on PATH; the app would have to avoid it).

The cost is that `lib` has to grow the operations the CLI does today (§4). That
work is worth doing anyway: it's what makes "GUI and CLI behave the same" true
rather than hoped for.

### Rules for `lib::ops`

- **Stateless calls on a path.** Each call opens the space at a path, does its
  work, and saves its state (including the picked set; today `main.rs` saves
  the index after each command, and `ops` takes that over). Studio holds no
  `Space` between calls, so the CLI and the app can be used on the same space
  at the same time.
- **A lock for writers.** Calls that change a space take an exclusive lock on
  `.duh/lock` (std `File::lock`); readers don't. A second writer waits briefly,
  then gets a plain error ("another duh is saving in this space").
- **Repo paths in, repo paths out.** Turning "relative to where you are" or a
  glob into repo paths stays in the CLI, because that's about the shell. The
  app passes repo paths directly.
- **Progress by callback**, as `FnMut(Progress)`, with one `Progress` enum for
  every long step (hashing, chunking, writing, transferring, checking).
- **Errors as sentences.** `lib`'s errors already read well; `ops` keeps them
  plain enough to show in a dialog as they are.

### Live updates

Studio watches the space's folder (and `.duh/`) with `Directory.watch`,
debounced, and refreshes the look report. A save made from the CLI appears in
the app within a second.

Looking at a folder full of big files hashes every one, every time, which is
too slow to repeat on each change. `lib::ops::look` gets a **stat cache**
(`.duh/statcache`: path → size, mtime, hash), like git's index, so unchanged
files aren't read again. The CLI's `look` gets faster too.

---

## 4. Audit: what the CLI does that `lib` doesn't

Every command was read. Most of duh's behaviour currently lives in `cli/src`,
mixed in with printing. Each item moves to `lib::ops` as a function returning
data; the CLI keeps only argument parsing, cwd/glob handling and printing.

| Behaviour (today in) | Moves to | Notes |
|---|---|---|
| Look report: picked/changed/missing/new/unchanged (`status.rs`) | `ops::look` | + stat cache |
| Folder scan, hidden files, `.duhignore` (`worktree.rs`) | `ops::worktree` | |
| Pick a file / a folder / everything / tracked only (`stage.rs`) | `ops::pick` | returns a change strip per file |
| Un-pick (`unstage.rs`) | `ops::unpick` | |
| Save, `-a`, empty-save rule, generated message (`commit.rs`) | `ops::save`, `ops::suggest_message` | `$EDITOR` stays in the CLI |
| Compare files / picks, computing strips (`diff.rs`) | `ops::compare` | |
| Bring back from picks, last save or any version, `--as` (`restore_cmd.rs`, `checkout.rs`) | `ops::bring_back` | |
| Make files match a version (`checkout.rs::checkout_tree`) | `ops::worktree` | used by open, rewind, receive |
| Rename, with no extra storage (`mv.rs`) | `ops::rename` | |
| Delete / forget, unsaved-changes safety (`rm.rs`) | `ops::delete`, `ops::forget` | |
| History walk (`log.rs`), inspect (`show.rs`) | `ops::history`, `ops::inspect` | + labels/drafts per save, file sizes, per-file strips |
| Journal (`reflog.rs`) | `ops::journal` | parsed into entries, not raw text |
| Labels (`tag.rs`) | `ops::labels` | |
| Rewind and its three modes (`reset.rs`) | `ops::rewind` | |
| Drafts: start, delete, rename, move (`branch.rs`) | `ops::drafts` | |
| Open a draft (`switch.rs`) | `ops::open_draft` | dirty check included |
| Get, send, receive, fast-forward rules (`copy.rs`) | `ops::get`, `ops::send`, `ops::receive` | |
| *(new)* How far ahead/behind a place is | `ops::sync_status` | the app needs it; the CLI can show it in `look` |
| Places (`remote.rs`), settings (`config.rs`) | `ops::places`, `ops::settings` | mostly wrappers already |
| Check (`check.rs`) | already in `lib` | progress moves to the shared enum |

Found along the way, fixed as part of the move:

- `drafts --set <version>` stores the text it was given without resolving it to a version.
- `history` and `inspect` print raw Unix timestamps.
- Two duh processes writing to one space at once aren't prevented (the lock fixes this).
- The `$EDITOR` message file is a fixed path in the temp folder, so two saves at once would collide.

---

## 5. The app

### Layout: one window

```
┌────────────────────────────────────────────────────────────────────────────┐
│ ◆ bracket-v3   ⎇ main ▾        ↑2 ↓0 origin  [Send] [Receive]   ● healthy  ⌘K │
├───────────────┬───────────────────────────────────┬────────────────────────┤
│ SPACES        │ TIMELINE                          │ INSPECTOR              │
│ ● bracket-v3  │ ┌───────────────────────────────┐ │ Your files             │
│   poster      │ │ Your files · 3 changed        │ │ ☑ body.FCStd  ▮▮▯▯▮▯   │
│ + Add space   │ │ ▮▮▯▯▮▯▯▯▯▮  +1.2 MB  −40 KB    │ │ ☑ hinge.FCStd ▮▯▯▯▯▯   │
│               │ └───────────────────────────────┘ │ ☐ notes.xcf   ▯▯▯▮▮▯   │
│ DRAFTS        │ ● Thicker walls        2h ago ↑   │                        │
│ ● main        │ ● Hinge pin 3 mm       yesterday  │ ┌────────────────────┐ │
│   experiment  │ ◆ final-v2  Fillets    Mon        │ │ What changed?      │ │
│ LABELS        │ ● First draft          Nov 12     │ └────────────────────┘ │
│   final-v2    │                                   │ [ Save 2 files ]       │
│ PLACES        │                                   │                        │
│   origin (S3) │                                   │                        │
└───────────────┴───────────────────────────────────┴────────────────────────┘
```

- **Top bar**: space name; draft switcher; **sync pill** (↑ saves to send, ↓
  saves to receive, per place) with Send and Receive; **health badge** (last
  check result; click to check now); command palette.
- **Sidebar**: spaces you've opened (recent, pinned), then the open space's
  drafts, labels and places.
- **Timeline**: *Your files* on top, then the open draft's saves, newest first:
  message, relative time, author initial, label chips, a "not sent" mark, and a
  thin strip summarising how much changed.
- **Inspector**: what's selected.
  - *Your files*: tick boxes to pick, a strip per file, a message box with a
    suggested message, and the **Save** button.
  - *A save*: message, who and when, files with sizes and strips, and actions:
    bring back a file (or save a copy as…), label this, start a draft here,
    rewind to here.
  - *A file in a save*: a larger strip with byte counts; an image preview for
    PNG/JPEG/WebP (a later version could read the thumbnail inside `.FCStd`
    files, which are zip archives with a preview image).

### Workflows

1. **First run**: two big choices, *Start a space in a folder* (folder picker)
   or *Get a copy from a place* (folder, or S3 with fields). Recent spaces after that.
2. **The daily loop**: edit in FreeCAD/GIMP → Studio notices within a second →
   your files show as changed with strips → tick what to save (all ticked by
   default) → write or accept the message → **Save**. Ctrl+S saves.
3. **Going back**: click a save → click a file → *Bring back* (replace your
   copy, confirming first if it has unsaved changes) or *Save a copy as…*
   (`bring-back --as`), which is the safe way to peek.
4. **Drafts**: *Start a draft here* from any save; switching drafts with
   unsaved work is blocked with a sentence saying why and two buttons (save
   first / discard).
5. **Sharing**: *Send* and *Receive*. If a draft has diverged, a sheet explains
   it in plain words: "origin has 2 saves you don't, and you have 1 it doesn't;
   duh never merges". Choices: receive into a new draft, send to a new draft,
   or (with a red confirm) overwrite the place's draft.
6. **Recover**: the journal as a list ("Rewound to 'Fillets', 3 days ago");
   each entry can be opened as a draft.
7. **Check**: the health badge runs `check` with a progress ring and lists
   problems in plain words; it can check any place too (your S3 backup).

### Look and feel

- **Dark first, light too**, following the system.
- **Material 3 as a base, restyled heavily**: no stock look. Deep ink
  background (`#0E1116`), raised panels a step lighter, one vivid accent:
  violet `#7C3AED` for the primary action and the selection, with cyan for
  everything about places and sending.
- **Mockups**: <https://claude.ai/artifact/Ne3YwXqey1dTpp3eyzRXF6> (the daily
  loop in dark; a save and the diverged sheet in light).
- **Change strip colours**: added = mint `#34D399`, removed = red `#E5484D`
  (darker than the mint, so the two differ in lightness as well as hue, for
  colour-blind eyes), unchanged = slate. They're the same everywhere: the timeline, the inspector
  and the CLI's terminal bar.
- **Type**: Inter for the interface and JetBrains Mono only for hashes,
  bundled with the app so it works offline and inside the Nix sandbox.
- **Motion**: 150–250 ms ease-out on selection, panel changes and new saves
  sliding into the timeline; a soft pulse on the sync pill when there's
  something to receive. Respects the system's reduce-motion setting.
- **Density**: comfortable by default, with a compact toggle; everything
  usable by keyboard (↑/↓ through saves, Space to pick, Ctrl+S to save, Ctrl+K
  for everything).

---

## 6. Build and packaging

- **Layout**: `studio/` holds the Flutter app; `studio/native/` holds the
  bridge crate (`crate-type = ["cdylib"]`, depends on `../../lib`, with its own
  `Cargo.lock` like `cli/`).
- **Rust side**: built by `rustPlatform.buildRustPackage` with `cargoLock`,
  exactly like the CLI: every crate fetched by Nix, no vendoring, no cargokit.
- **Generated bindings are committed** (`studio/lib/src/rust/`,
  `studio/native/src/frb_generated.rs`), so the Nix build never runs codegen.
  Regenerating is a dev-shell step (`just studio-codegen`) using nixpkgs'
  `flutter_rust_bridge_codegen` 2.12.0, pinned to the same version as the Dart package.
- **Flutter side**: `flutter.buildFlutterApplication` (Flutter 3.41.9 in the
  flake's nixpkgs) with the pub lock converted for Nix, no git-hosted packages,
  and the bridge library copied into the bundle's `lib/`, where the app loads
  it at start.
- **One command**: `nix run .#studio`, or `nix run github:danhab99/duh#studio`
  from anywhere. `nix flake check` builds it too.
- **Platforms**: Linux first; this is what Nix builds and what you use.
  Flutter can target macOS and Windows later, but packaging those is out of
  scope for this plan.

## 7. Testing

- `lib::ops` gets unit tests per operation; the CLI becomes a thin wrapper, so
  **`demo.sh` keeps proving both** (it drives the CLI, which drives `ops`).
- The bridge crate gets a Rust test that drives a full loop through its
  exported functions (start, pick, save, history, bring back, check).
- Flutter widget tests run against a fake engine (a Dart interface with a real
  implementation and a fake one), covering every screen state: empty, first
  run, dirty, diverged, error, check problems.
- An end-to-end smoke test in `nix flake check`: launch the built app headless
  (Xvfb), open a demo space, confirm it lists the saves. *Attempted in
  phase 4; if Flutter won't run under Xvfb in the sandbox, it becomes a
  documented manual check instead.*

---

## 8. Plan

Ordered to retire the biggest risk first: proving Flutter + Rust + Nix builds
in one command before investing in screens. Each phase ends with everything
passing and committed, so work can stop and resume between any two.

### Phase 0: prove the toolchain (small)
- [x] `studio/` skeleton app and `studio/native` crate exposing one function (`engine_version()`)
- [x] Bindings generated and committed; app shows the version from Rust
- [x] `nix run .#studio` opens the window; `nix flake check` builds it and runs a headless smoke test (`studioSmoke`)
- **If this fails** in a way that can't be fixed within the phase, fall back
  to the CLI with a JSON-lines mode over stdio (the design in §3 survives;
  only the bridge layer changes). Decided with you before going further.

### Phase 1: one engine — `lib::ops` (large; the most important phase)
- [x] `ops` module: lock, stateless open-by-path (progress is a per-operation callback, not one shared enum)
- [x] Move each behaviour in §4 into `ops`, with unit tests
- [x] CLI commands rewritten as parse → `ops` → print (output unchanged; CLI is 1,572 lines, down from 2,430, with no low-level calls left)
- [x] `sync_status`; the four fixes from §4
- [x] Stat cache for `look` (`.duh/statcache`; a 400 MB file: 2.35 s → instant)
- [x] `demo.sh` passes unchanged at every step (it's the proof the CLI still behaves)

### Phase 2: the bridge (medium)
- [x] `studio/native` exposes `ops` with Dart-friendly types (no progress streams: a stream-returning function loses its result in flutter_rust_bridge, so `pick` returns results and the app shows a spinner)
- [x] Rust test driving the full loop through the bridge (`core.rs`)
- [x] Dart `Engine` interface with a bridge implementation and a fake one

### Phase 3: the app (large)
- [x] Theme, typography, change-strip widget, app shell (top bar, sidebar, timeline, inspector)
- [x] First run (start / get / open), recent spaces, folder watching (stat polling)
- [x] Your files: pick, compare, save, suggested message
- [x] Timeline and save inspector: bring back, save a copy as…, label, start a draft, rewind
- [x] Drafts switcher, labels, journal / recover
- [x] Places editor, sync pill, send / receive, the diverged sheet
- [x] Health badge and check view (here and per place)
- [x] Command palette, keyboard shortcuts, light theme
- [x] Reduce motion (animations drop to zero when the system asks)
- [x] Widget tests for the main screen states (first run, empty space, pick and save, save inspector and rewind, diverged, not sent, check, palette)

### Phase 4: polish and proof (small)
- [x] Headless smoke test in `nix flake check`
- [ ] README: screenshots, `nix run .#studio`
- [ ] Final pass with you on look and wording

### Size and tokens

Rough size: phase 1 moves and tests about 1,500 lines of Rust; phase 2 is
about 500 lines plus generated code; phase 3 is about 4,000 lines of Dart.
That's several sessions of work. I can't see your plan's limits, so the plan
is built so that running out mid-way costs nothing: every phase (and most
single checkboxes) ends committed and passing, and this file records exactly
where things stand.

## 9. Risks

| Risk | Mitigation |
|---|---|
| Flutter + FRB under Nix doesn't build cleanly | Phase 0 proves it first, with the CLI JSON-lines mode as a fallback |
| Pub packages need fetching for the Nix lock | Only pub.dev packages, few of them; lock converted once and committed |
| Moving CLI logic into `lib` changes behaviour | `demo.sh` must pass unchanged after every step |
| Big folders make live refresh slow | Stat cache, plus refresh off the UI thread with debouncing |
| CLI and app writing to one space at once | `.duh/lock` for writers; stateless reads |
