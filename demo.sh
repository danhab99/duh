#!/usr/bin/env bash
# demo.sh — the standard duh must meet.
#
# Walks through everything you can do to a version-controlled file and
# asserts the outcome of every step. Any mismatch exits non-zero (and fails
# `nix flake check`). At the end, every file is checked out at every commit
# ever made and compared byte-for-byte with what was committed — first in
# this space, then in a clone of it.
set -euo pipefail

td="${1:?Usage: demo.sh <test-dir>}"
DUH="${DUH:-duh}"

VERIFICATION_DIR="$td/.verification"
EXPECT="$VERIFICATION_DIR/expected"   # lines: <commit>|<path>|<sha256>

dim()   { printf '\033[2m%s\033[0m\n' "$*"; }
green() { printf '\033[32m%s\033[0m\n' "$*"; }
red()   { printf '\033[31m%s\033[0m\n' "$*"; }
plain() { sed 's/\x1b\[[0-9;]*m//g'; }

fail() {
    red "  ✗ $*"
    exit 1
}
ok() { green "  ✓ $*"; }

# Wrapper: runs duh under GNU time and prints peak RSS after each command.
# time stats go to a temp file (-o) so duh's own stdout/stderr are unaffected.
run() {
    local timefile
    timefile=$(mktemp)

    echo
    echo "=== duh $* ==="

    command time -v -o "$timefile" "$DUH" "$@"
    local kb
    kb=$(grep "Maximum resident" "$timefile" | awk '{print $NF}')
    dim "  ↳ peak RAM: $((kb / 1024)) MiB"
    rm -f "$timefile"
}

# gen <letter> <offset> <len>: bytes [offset, offset+len) of a deterministic
# pseudo-random stream named <letter>. Slices of the same stream share content,
# slices of different streams share nothing — so every "unchanged" region below
# is genuinely identical data and every "added" region is genuinely new.
gen() {
    python3 -c "import random, sys; r = random.Random('$1'); sys.stdout.buffer.write(r.randbytes($2 + $3)[$2:])"
}

sha() { sha256sum "$1" | cut -d' ' -f1; }

head_commit() { "$DUH" log -n 1 | plain | awk '/^commit /{print $2; exit}'; }

section() {
    echo ""
    echo "--------------------------------------------------------------------"
    for line in "$@"; do echo " $line"; done
    echo "--------------------------------------------------------------------"
}

part() {
    echo ""
    echo "================================================================"
    echo " $*"
    echo "================================================================"
}

# expect_bar <spec> <stage output>: the Dif bar must consist of exactly the
# segments in <spec> (e.g. "=20 +20 =20"), each within ±2 columns. A single
# column is ignored: it is a boundary chunk (see the note in Part 1).
expect_bar() {
    local spec="$1" output="$2" bar
    bar=$(printf '%s\n' "$output" | plain | sed -n 's/^Dif\[\(.*\)\]$/\1/p' | tail -n1)
    [[ -n "$bar" ]] || fail "no diff bar in stage output"
    python3 - "$spec" "$bar" <<'EOF' || fail "diff bar [$bar] doesn't match expected '$spec'"
import itertools, sys
spec, bar = sys.argv[1].split(), sys.argv[2]
runs = []
for k, g in itertools.groupby(bar):
    n = len(list(g))
    if n <= 1:
        continue
    if runs and runs[-1][0] == k:
        runs[-1] = (k, runs[-1][1] + n)
    else:
        runs.append((k, n))
want = [(s[0], int(s[1:])) for s in spec]
sys.exit(0 if len(runs) == len(want) and all(
    a == b and abs(n - m) <= 2 for (a, n), (b, m) in zip(runs, want)) else 1)
EOF
    ok "diff bar is '$spec'"
}

# stage_expect <spec> <path>: stage a file and check its diff bar.
stage_expect() {
    local spec="$1" path="$2" out
    out=$(run stage "$path")
    printf '%s\n' "$out"
    expect_bar "$spec" "$out"
}

# status_is <summary>: `duh status` must report exactly these groups, written
# as "staged: a b; modified: c; missing: d; clean: e" (empty groups omitted,
# paths as status prints them). "nothing" means no tracked files.
status_is() {
    local want="$1" got
    got=$("$DUH" status | plain | awk '
        /^Staged for commit:/      { s = "staged";   next }
        /^Modified \(not staged\):/ { s = "modified"; next }
        /^Deleted \/ missing:/      { s = "missing";  next }
        /^Up to date:/              { s = "clean";    next }
        /^No tracked files/         { none = 1 }
        /^  / && s { sub(/^  /, ""); g[s] = g[s] (g[s] ? " " : "") $0 }
        END {
            n = split("staged modified missing clean", order, " ")
            for (i = 1; i <= n; i++) if (order[i] in g) {
                out = out (out ? "; " : "") order[i] ": " g[order[i]]
            }
            print (none ? "nothing" : out)
        }')
    [[ "$got" == "$want" ]] || fail "status is '$got', expected '$want'"
    ok "status: $want"
}

# expect_fail <message fragment> <duh args...>: the command must fail
# (non-zero exit) with an error mentioning <message fragment>.
expect_fail() {
    local msg="$1" out
    shift
    if out=$("$DUH" "$@" 2>&1); then
        fail "'duh $*' succeeded but should have failed"
    fi
    [[ "$out" == *"$msg"* ]] || fail "'duh $*' failed with '$out', expected '$msg'"
    ok "'duh $*' is refused: $msg"
}

# same_file <a> <b> <what>
same_file() {
    [[ "$(sha "$1")" == "$(sha "$2")" ]] || fail "$3"
    ok "$3"
}

# record: the working tree must be clean; remember the sha256 of every file
# in the HEAD commit so it can be checked out and compared later.
record() {
    status_clean
    local h
    h=$(head_commit)
    "$DUH" show "$h" | plain | sed -n '/^files:/,$s/^  \(.*\) -> [0-9a-f]*$/\1/p' |
    while IFS= read -r path; do
        echo "$h|$path|$(sha "$path")" >> "$EXPECT"
    done
    dim "  ✓ recorded ${h:0:12}: $(grep -c "^$h|" "$EXPECT") file(s)"
}

status_clean() {
    local out
    out=$("$DUH" status | plain)
    if grep -qE '^(Staged for commit|Modified \(not staged\)|Deleted / missing):' <<<"$out"; then
        fail "working tree not clean:"$'\n'"$out"
    fi
}

commit_msg() {
    run commit -m "$1"
    record
}

echo "TEST PATH $td"
echo "duh binary: $(command -v "$DUH")"

cd "$td"
mkdir -p "$VERIFICATION_DIR"
: > "$EXPECT"

SIZE=1000000
HALF=$((SIZE / 2))
QUARTER=$((SIZE / 4))

run init
status_is "nothing"

# ===========================================================================
part "Part 1: every way to edit a file"
# Streams A, B, C, D are unrelated random data; slices of one stream match.
#   +  = ADDED     (new bytes not present in previous version)
#   =  = UNCHANGED (bytes identical to previous version)
#   -  = DELETED   (bytes present in previous version, now gone)
# Chunk boundaries are content-defined (~2-4 KiB), so a region edge can be
# off by one chunk; at 60 columns that is far below one column.
# ===========================================================================

section "NEW FILE : start tracking a file" \
        "new: A[0:1M]" \
        "expect bar: all added"
gen a 0 $SIZE > project
run status
stage_expect "+60" project
status_is "staged: project"
commit_msg "add project: all A"
status_is "clean: project"
run show

section "REWRITE : replace the whole file with unrelated content" \
        "old: A[0:1M]   new: B[0:1M]" \
        "expect bar: -...- (1M A deleted) +...+ (1M B added)"
gen b 0 $SIZE > project
status_is "modified: project"
stage_expect "-30 +30" project
commit_msg "rewrite: all B"

section "REPLACE FIRST HALF : first half new (C), second half kept (B)" \
        "old: B[0:1M]   new: C[0:500k] + B[500k:1M]" \
        "expect bar: -...- (500k B deleted) +...+ (500k C added) =...= (500k B unchanged)"
{ gen c 0 $HALF; gen b $HALF $HALF; } > project
stage_expect "-20 +20 =20" project
commit_msg "half C, half B"

section "REWRITE : unrelated content again" \
        "old: C[0:500k] + B[500k:1M]   new: D[0:1M]" \
        "expect bar: -...- (1M deleted) +...+ (1M D added)"
gen d 0 $SIZE > project
stage_expect "-30 +30" project
commit_msg "rewrite: all D"

section "APPEND : add new content at the end" \
        "old: D[0:1M]   new: D[0:1M] + A[0:500k]" \
        "expect bar: =...= (1M D, unchanged) +...+ (500k A, added)"
{ gen d 0 $SIZE; gen a 0 $HALF; } > project
stage_expect "=40 +20" project
commit_msg "append A"

section "TRUNCATE : trim content from the end" \
        "old: D[0:1M] + A[0:500k]   new: D[0:1M]" \
        "expect bar: =...= (1M D, unchanged) -...- (500k A, deleted)"
gen d 0 $SIZE > project
stage_expect "=40 -20" project
commit_msg "drop appended A"

section "PREPEND : add new content at the start" \
        "old: D[0:1M]   new: B[0:500k] + D[0:1M]" \
        "expect bar: +...+ (500k B, added) =...= (1M D, unchanged)"
{ gen b 0 $HALF; gen d 0 $SIZE; } > project
stage_expect "+20 =40" project
commit_msg "prepend B"

section "DROP PREFIX : remove content from the start" \
        "old: B[0:500k] + D[0:1M]   new: D[0:1M]" \
        "expect bar: -...- (500k B, deleted) =...= (1M D, unchanged)"
gen d 0 $SIZE > project
stage_expect "-20 =40" project
commit_msg "drop prepended B"

section "INSERT : insert content in the middle" \
        "old: D[0:1M]   new: D[0:500k] + B[0:500k] + D[500k:1M]" \
        "expect bar: =...= (500k D) +...+ (500k B, inserted) =...= (500k D)"
{ gen d 0 $HALF; gen b 0 $HALF; gen d $HALF $HALF; } > project
stage_expect "=20 +20 =20" project
commit_msg "insert B in middle"

section "CUT : remove content from the middle" \
        "old: D[0:500k] + B[0:500k] + D[500k:1M]   new: D[0:1M]" \
        "expect bar: =...= (500k D) -...- (500k B, deleted) =...= (500k D)"
gen d 0 $SIZE > project
stage_expect "=20 -20 =20" project
commit_msg "remove inserted B"

section "REPLACE PREFIX : overwrite the first half" \
        "old: D[0:1M]   new: A[0:500k] + D[500k:1M]" \
        "expect bar: -...- (500k D, deleted) +...+ (500k A, added) =...= (500k D)"
{ gen a 0 $HALF; gen d $HALF $HALF; } > project
stage_expect "-20 +20 =20" project
commit_msg "replace first half with A"

section "DROP PREFIX + APPEND : remove prefix, keep core, add suffix" \
        "old: A[0:500k] + D[500k:1M]   new: D[500k:1M] + C[0:250k]" \
        "expect bar: -...- (500k A, deleted) =...= (500k D) +...+ (250k C, added)"
{ gen d $HALF $HALF; gen c 0 $QUARTER; } > project
stage_expect "-24 =24 +12" project
commit_msg "drop A prefix, keep D, add C suffix"

section "SHRINK + REPLACE : three sections from two" \
        "old: D[500k:1M] + C[0:250k]   new: D[500k:750k] + A[0:250k] + C[0:250k]" \
        "expect bar: =...= (250k D) -...- (250k D) +...+ (250k A) =...= (250k C)"
{ gen d $HALF $QUARTER; gen a 0 $QUARTER; gen c 0 $QUARTER; } > project
stage_expect "=15 -15 +15 =15" project
commit_msg "D-A-C three sections"

section "REPLACE MIDDLE : swap the middle section" \
        "old: D[500k:750k] + A[0:250k] + C[0:250k]   new: D[500k:750k] + B[0:250k] + C[0:250k]" \
        "expect bar: =...= (250k D) -...- (250k A, deleted) +...+ (250k B, added) =...= (250k C)"
{ gen d $HALF $QUARTER; gen b 0 $QUARTER; gen c 0 $QUARTER; } > project
stage_expect "=15 -15 +15 =15" project
commit_msg "replace A section with B"

FIRST=$(head -n1 "$EXPECT" | cut -d'|' -f1)
run log -n 3

# ===========================================================================
part "Part 2: everyday work on a tracked file"
# ===========================================================================

cp project "$VERIFICATION_DIR/committed"

section "STATUS : every state a tracked file can be in"
gen a 0 1000 >> project
status_is "modified: project"
"$DUH" stage project > /dev/null
status_is "staged: project"
gen b 0 1000 >> project
status_is "staged: project; modified: project"
rm project
status_is "staged: project; missing: project"

section "UNSTAGE : take a file out of the next commit"
gen a 0 1000 > project
run stage project > /dev/null
run unstage project
status_is "modified: project"
run commit -m "commit with nothing staged"
same_file project project "working copy keeps its uncommitted edit"

section "DISCARD : throw away local edits by checking out HEAD"
run checkout project
same_file project "$VERIFICATION_DIR/committed" "project is back to the committed version"
status_is "clean: project"
record

section "DELETE + RESTORE : recover a deleted file"
rm project
status_is "missing: project"
run checkout project
same_file project "$VERIFICATION_DIR/committed" "deleted file restored"
status_is "clean: project"

section "PEEK : read an old version without touching the working copy"
run checkout -c "$FIRST" --as "$VERIFICATION_DIR/first" project
gen a 0 $SIZE > "$VERIFICATION_DIR/expected-first"
same_file "$VERIFICATION_DIR/first" "$VERIFICATION_DIR/expected-first" "first version reconstructed"
status_is "clean: project"

section "REVERT : bring back an old version as a new commit" \
        "expect bar: everything deleted, all of A re-added"
run checkout -c "$FIRST" project
status_is "modified: project"
stage_expect "-27 +33" project
commit_msg "revert to the first version"
same_file project "$VERIFICATION_DIR/expected-first" "project matches the first version again"

section "MISTAKES : duh refuses and explains"
expect_fail "did not match any files" stage no-such-file
expect_fail "file not found in commit" checkout no-such-file
expect_fail "outside the worktree" stage /etc/hostname
expect_fail "outside the worktree" checkout ../escape.bin

# ===========================================================================
part "Part 3: several files and directories"
# ===========================================================================

section "ADD A SECOND FILE IN A SUBDIRECTORY : later in history"
mkdir -p assets
gen t 0 $HALF > assets/texture.bin
stage_expect "+60" assets/texture.bin
status_is "staged: assets/texture.bin; clean: project"
commit_msg "add assets/texture.bin"

section "EDIT ONE FILE : the other carries forward unchanged" \
        "old: A[0:1M]   new: D[0:250k] + A[0:500k]" \
        "expect bar: +...+ (250k D added) =...= (500k A) -...- (500k A deleted)"
{ gen d 0 $QUARTER; gen a 0 $HALF; } > project
stage_expect "+12 =24 -24" project
status_is "staged: project; clean: assets/texture.bin"
commit_msg "edit project only"
run show

section "WORK FROM A SUBDIRECTORY : paths are relative to where you are"
cd assets
{ gen t 0 $HALF; gen u 0 $QUARTER; } > texture.bin
status_is "modified: texture.bin; clean: ../project"
stage_expect "=40 +20" texture.bin
status_is "staged: texture.bin; clean: ../project"
cd ..
commit_msg "append to texture from inside assets/"

section "ABSOLUTE PATHS : stage and check out by full path"
gen e 0 $QUARTER > notes.bin
stage_expect "+60" "$PWD/notes.bin"
commit_msg "add notes.bin by absolute path"
rm assets/texture.bin
run checkout "$PWD/assets/texture.bin"
status_is "clean: assets/texture.bin notes.bin project"

# ===========================================================================
part "Part 4: tags, branches and resets"
# ===========================================================================

section "TAG : name a commit and use the name anywhere a commit is accepted"
run tag -n release-1
cp project "$VERIFICATION_DIR/release-1"
run tag
gen r 0 $QUARTER >> project
"$DUH" stage project > /dev/null
commit_msg "post-release edit"
run checkout -c release-1 --as "$VERIFICATION_DIR/from-tag" project
same_file "$VERIFICATION_DIR/from-tag" "$VERIFICATION_DIR/release-1" "tag resolves to the tagged version"

section "BRANCH : diverge, then switch back and forth"
cp project "$VERIFICATION_DIR/main-project"
run switch experiment true
run branch
gen x 0 $HALF > project
gen y 0 $QUARTER > assets/experiment-only.bin
"$DUH" stage project assets/experiment-only.bin > /dev/null
commit_msg "experiment: rewrite project, add a file"
cp project "$VERIFICATION_DIR/experiment-project"
run switch main
same_file project "$VERIFICATION_DIR/main-project" "main's version of project is back"
[[ ! -e assets/experiment-only.bin ]] || fail "experiment-only file still present on main"
ok "experiment-only file is gone on main"
status_is "clean: assets/texture.bin notes.bin project"
run switch experiment
same_file project "$VERIFICATION_DIR/experiment-project" "experiment's version of project is back"
[[ -f assets/experiment-only.bin ]] || fail "experiment-only file missing on experiment"
ok "experiment-only file is back"
gen z 0 100 >> project
expect_fail "uncommitted changes" switch main
run checkout project
run switch main
expect_fail "checked out" branch -d main

section "RESET --hard : move the branch back and restore the files"
run reset -m hard release-1
same_file project "$VERIFICATION_DIR/release-1" "project is at release-1"
[[ "$(head_commit)" == "$(grep -m1 '' <("$DUH" log -n 1 release-1 | plain | awk '/^commit /{print $2}'))" ]] ||
    fail "HEAD is not at release-1"
ok "main points at release-1"
status_is "clean: assets/texture.bin notes.bin project"

section "RESET --soft : undo a commit but keep its changes staged"
gen s 0 $QUARTER >> project
"$DUH" stage project > /dev/null
commit_msg "commit to undo"
UNDONE=$(head_commit)
cp project "$VERIFICATION_DIR/undone"
run reset -m soft release-1
status_is "staged: project; clean: assets/texture.bin notes.bin"
run commit -m "redo the undone commit"
same_file project "$VERIFICATION_DIR/undone" "re-committing restores the same content"
record

section "RESET (mixed) : undo a commit, keep the edits unstaged"
run reset release-1
status_is "modified: project; clean: assets/texture.bin notes.bin"
run checkout project
same_file project "$VERIFICATION_DIR/release-1" "discarding returns to release-1"

# ===========================================================================
part "Part 5: clone"
# ===========================================================================

# Cloning happens outside any duh space (inside one, --from names a remote).
CLONE="${td%/}-clone"
section "CLONE : copy the repository and check out its files"
(cd "$(dirname "$td")" && run copy --from "$td" --as "$CLONE")
for f in project notes.bin assets/texture.bin; do
    same_file "$CLONE/$f" "$f" "clone has the same $f"
done
(cd "$CLONE" && status_is "clean: assets/texture.bin notes.bin project")

# ===========================================================================
part "VERIFICATION: every file at every commit, byte for byte"
# ===========================================================================

out="$VERIFICATION_DIR/checkout.bin"

# verify_commits <dir> <commit...>: check out every recorded file of each
# commit inside <dir> and compare it with the recorded sha256.
verify_commits() {
    local dir="$1" commits=0 files=0 c path want
    shift
    for c in "$@"; do
        while IFS='|' read -r _ path want; do
            rm -f "$out"
            (cd "$dir" && "$DUH" checkout -c "$c" --as "$out" "$path" > /dev/null) ||
                fail "checkout of $path at ${c:0:12} failed"
            [[ "$(sha "$out")" == "$want" ]] || fail "$path at ${c:0:12} differs"
            files=$((files + 1))
        done < <(grep "^$c|" "$EXPECT")
        commits=$((commits + 1))
    done
    ok "$commits commits, $files file versions reconstructed exactly"
}

mapfile -t all_commits < <(cut -d'|' -f1 "$EXPECT" | awk '!seen[$0]++')
echo "In this space (all branches, including commits reset away):"
verify_commits "$td" "${all_commits[@]}"

mapfile -t clone_commits < <(cd "$CLONE" && "$DUH" log | plain | awk '/^commit /{print $2}')
echo "In the clone (its history):"
verify_commits "$CLONE" "${clone_commits[@]}"

rm -f "$out"
echo ""
echo "  object store size: $(du -sh .duh/objects | cut -f1)"
echo ""
green "SUCCESS: every outcome above was checked and held."
