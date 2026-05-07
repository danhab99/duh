---

kanban-plugin: board

---

## Backlog

- [ ] `duh diff` CLI subcommand (lib algorithm exists in `diff.rs`/`dedup.rs`, no CLI entry)
- [ ] `duh rm` — untrack a file from the index
- [ ] `duh reset` — hard/soft/mixed reset of HEAD or index
- [ ] `duh revert` — reverse-commit creation
- [ ] `duh tag` — lightweight and annotated tags
- [ ] Tree objects — commits store flat `HashMap<String, Hash>`; subdirectory structure not modeled
- [ ] Pack files / GC — objects stored individually, no packing or garbage collection
- [ ] Short hash resolution — abbreviated hashes rejected everywhere
- [ ] `duh remote add/remove/list`
- [ ] `duh clone`
- [ ] `duh push` / `duh pull` / `duh fetch`

## In Progress

## Bug Fixes

- [ ] `switch` panics on detached HEAD (`panic!("cannot switch to commit rn")`)
- [ ] `switch` panics on missing branch (`panic!("ref does not exist")`) — should return error
- [ ] `branch --rename` writes new ref as a `Hash` instead of a `Ref`, breaking the ref chain

## Done

- [ ] `.duhignore` support
- [ ] Recursive/wildcard staging — `duh stage .` or glob patterns
- [ ] `duh config` — CLI for reading/writing `.duh/config` (currently must edit manually)
- [ ] `switch --create` — struct field exists in `SwitchArgs` but has no `#[arg]` annotation; unreachable from CLI
- [ ] `duh show <ref>` — currently fixed to HEAD only; needs to accept hash or branch name
- [ ] `duh log <ref>` — currently fixed to HEAD only; needs to accept hash or branch name
- [x] `duh init` — create `.duh/` metadata directory
- [x] `duh stage <file>` — CDC + rolling-hash dedup, delta fragments to object store, progress bar
- [x] `duh unstage <file>` — remove file from staging index
- [x] `duh commit` — create commit object, `-m` message, `-g` auto-generate message, `$EDITOR` fallback
- [x] `duh log [-n]` — walk parent-linked commit chain from HEAD
- [x] `duh status` — compare working tree against HEAD + staged index
- [x] `duh show` — display HEAD commit hash, parent, author, message, file list
- [x] `duh checkout <file> [-c commit]` — restore single file from a commit via fragment replay
- [x] `duh branch [-d|-r|-s]` — list, delete, rename, point branch ref at commit
- [x] `duh switch <branch>` — move HEAD to branch ref and check out all files

## Rejected

- [ ] `duh checkout` — only accepts full 64-char hash or `HEAD`; branch names and short hashes rejected
- [ ] `duh stash` / `duh stash pop`
- [ ] `duh rebase`
- [ ] `duh cherry-pick`
- [ ] `duh merge` — three-way merge, conflict detection, conflict markers

%% kanban:settings
```json
{"kanban-plugin":"board","list-collapse":[false,false,false,false]}
```
%%
