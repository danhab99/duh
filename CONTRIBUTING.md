# Contributing to duh

Thank you for wanting to help. duh is small, and every kind of
contribution matters: trying it on your own projects and reporting what
breaks, fixing docs, packaging it for macOS, or working on the engine.

## Reporting a problem

Open an issue with:

- what you ran (or clicked), and what you expected;
- what happened, with the exact error;
- your system, and how you installed duh;
- for wrong results, `duh check` output, and whether a fresh copy
  (`duh get`) shows the same.

Please don't attach a space with private files. A small reproduction made
from random bytes (`head -c 1000000 /dev/urandom > a.bin`) is ideal.

## Getting set up

Everything builds with [Nix](https://nixos.org/download). You don't need
Rust, Flutter or anything else installed.

```bash
git clone https://github.com/danhab99/duh
cd duh
nix develop            # Rust, rust-analyzer, just
nix develop .#studio   # also Flutter and the bridge code generator
```

| Command | Does |
|---|---|
| `just build` | build the command line through Nix |
| `just check` | everything CI runs: every test, `demo.sh`, the app starting on Linux and (under Wine) Windows |
| `just demo` | run `demo.sh` against the Nix-built binary |
| `just studio` | run duh Studio |
| `just studio-test` | Studio's widget tests, and the real engine through the bridge |
| `just studio-screens` | render Studio's screens to `studio/test_screens/out/` |
| `just docs-screens` | render them with Nix into `docs/images/` |
| `just studio-codegen` | regenerate the bridge after changing `studio/native/src/api` (slow) |

`nix flake check` is the same as `just check`.

## How the code is laid out

```
lib/              the engine; every operation lives in lib/src/ops
cli/              the command line: parse, call lib::ops, print
studio/           duh Studio (Flutter)
studio/native/    the Rust bridge Studio loads (flutter_rust_bridge)
studio/windows/   the Windows window code, and the one-file launcher
nix/              how Nix builds the Windows app on Linux
docs/             the documentation
demo.sh           the standard (below)
```

**New behaviour goes in the library first.** The command line and the app
are thin shells over `lib::ops`: they parse input, call one operation,
and show the result. If you find yourself deciding *what happens* in
`cli/` or `studio/`, it belongs in `lib/`.

## The standard: demo.sh

[`demo.sh`](demo.sh) runs every command the way a person would and checks
the outcome of each one. It rebuilds every file at every save and
compares them byte for byte, shares through a folder and an S3 server,
and damages a space on purpose to prove `check` notices.

duh is correct when `demo.sh` honestly passes. Changes that alter
behaviour should extend it; a change that makes it fail needs fixing, not
the demo loosening.

## Making a change

- **Tests first for bugs.** Write a test that fails without your fix,
  then make it pass.
- **Small, meaningful commits.** One logical change per commit, with a
  message saying what and why (`fix(lib): …`, `feat(studio): …`,
  `docs: …`).
- **Nix builds everything.** Dependencies come through the flake and the
  lock files; don't add steps that need tools installed by hand.
- **Words matter.** duh is for artists. In anything a person reads, use
  duh's words (space, pick, save, draft, label, place), not git's.
  Errors should say what happened and what to do next.
- **The format.** Anything that changes how data is stored must keep
  existing spaces readable, and update [docs/FORMAT.md](docs/FORMAT.md).
- **The docs.** If you change what a command does or says, update
  [docs/](docs/). If you change how the app looks, rerun
  `just docs-screens`.

Before opening a pull request, run `just check`.

## Releases

Releases are made by pushing an annotated tag:

```bash
git tag -a v0.4.0      # subject: the release title; body: the notes
git push origin v0.4.0
```

CI builds `nix build .#ci` from that commit and attaches its files: the
Linux AppImage and the Windows app and command line. CI only ever runs
Nix; there are no build steps outside the flake.

## Help wanted

- **macOS.** Nobody working on duh uses a Mac. The command line should
  build there with `nix build`; the app needs Xcode. See the
  [README](README.md#macos).
- **Trying duh on real projects**, especially big ones, and reporting
  what's slow or confusing.
- Clearing out stored data no save needs any more (garbage collection),
  sending labels, and fetching only part of a large space.
