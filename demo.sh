#!/usr/bin/env bash
# demo.sh — the standard duh must meet.
#
# Walks through everything you can do to a version-controlled file and
# asserts the outcome of every step. Any mismatch exits non-zero (and fails
# `nix flake check`). At the end, every file is brought back at every save
# ever made and compared byte-for-byte with what was saved — in this space,
# in a copy made with `duh get`, and in a copy fetched from S3.
#
# Usage: ./demo.sh [test-dir]
#   With no test-dir, it runs in a fresh temporary folder and prints where.
#   Unless DUH names a binary, it restarts itself in the flake's `duh` shell,
#   which provides the Nix-built duh (never one that's merely on PATH),
#   rclone and time.
set -euo pipefail

# The space lives at td; sibling folders (td-clone, td-outside.bin, …) are
# made beside it. With no argument, both go in one fresh temporary folder.
td="${1:-$(mktemp -d -t duh-demo.XXXXXX)/space}"

if [[ -z "${DUH:-}" && -z "${DUH_DEMO_SHELL:-}" ]]; then
    exec nix develop "$(dirname "$(realpath "$0")")#duh" \
        --command env DUH_DEMO_SHELL=1 bash "$(realpath "$0")" "$td"
fi
# Absolute, so those siblings land beside the test dir, not inside it once
# we cd there.
mkdir -p "$td"
td=$(cd "$td" && pwd)
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

head_commit() { "$DUH" history -n 1 | plain | awk '/^version /{print $2; exit}'; }

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
    out=$(run pick "$path")
    printf '%s\n' "$out"
    expect_bar "$spec" "$out"
}

# status_is <summary>: `duh status` must report exactly these groups, written
# as "staged: a b; modified: c; missing: d; untracked: e; clean: f" (empty
# groups omitted, entries as status prints them, e.g. "x.bin (new)").
# "nothing" means no files at all.
status_is() {
    local want="$1" got
    got=$("$DUH" look | plain | awk '
        /^Picked for the next save:/ { s = "staged";   next }
        /^Changed, not picked:/    { s = "modified"; next }
        /^Missing:/                { s = "missing";  next }
        /^New, not tracked yet:/   { s = "untracked"; next }
        /^Saved, unchanged:/       { s = "clean";    next }
        /^Nothing here yet/         { none = 1 }
        /^  / && s { sub(/^  /, ""); g[s] = g[s] (g[s] ? " " : "") $0 }
        END {
            n = split("staged modified missing untracked clean", order, " ")
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

# store_bytes: total size of the object store's files.
store_bytes() { find .duh/objects -type f -printf '%s\n' | awk '{s += $1} END {print s + 0}'; }

# in_commit <commit> <path>: whether the commit's tree contains the path.
in_commit() {
    "$DUH" inspect "$1" | plain | sed -n '/^files:/,$s/^  \(.*\) -> [0-9a-f]*$/\1/p' | grep -qxF "$2"
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
    "$DUH" inspect "$h" | plain | sed -n '/^files:/,$s/^  \(.*\) -> [0-9a-f]*$/\1/p' |
    while IFS= read -r path; do
        echo "$h|$path|$(sha "$path")" >> "$EXPECT"
    done
    dim "  ✓ recorded ${h:0:12}: $(grep -c "^$h|" "$EXPECT") file(s)"
}

status_clean() {
    local out
    out=$("$DUH" look | plain)
    if grep -qE '^(Picked for the next save|Changed, not picked|Missing):' <<<"$out"; then
        fail "working tree not clean:"$'\n'"$out"
    fi
}

commit_msg() {
    run save -m "$1"
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

run start
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
run look
status_is "untracked: project"
stage_expect "+60" project
status_is "staged: project (new)"
commit_msg "add project: all A"
status_is "clean: project"
run inspect

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
run history -n 3

# ===========================================================================
part "Part 2: everyday work on a tracked file"
# ===========================================================================

cp project "$VERIFICATION_DIR/committed"

section "LOOK : every state a tracked file can be in"
gen a 0 1000 >> project
status_is "modified: project"
"$DUH" pick project > /dev/null
status_is "staged: project"
gen b 0 1000 >> project
status_is "staged: project; modified: project"
rm project
status_is "staged: project; missing: project"

section "UNPICK : take a file out of the next save"
gen a 0 1000 > project
run pick project > /dev/null
run unpick project
status_is "modified: project"
expect_fail "nothing to save" save -m "commit with nothing staged"

section "DISCARD : throw away local edits by bringing back the last save"
run bring-back project
same_file project "$VERIFICATION_DIR/committed" "project is back to the committed version"
status_is "clean: project"
record

section "MISSING + BRING BACK : recover a deleted file"
rm project
status_is "missing: project"
run bring-back project
same_file project "$VERIFICATION_DIR/committed" "deleted file restored"
status_is "clean: project"

section "PEEK : read an old version without touching the working copy"
run bring-back --from "$FIRST" --as "$VERIFICATION_DIR/first" project
gen a 0 $SIZE > "$VERIFICATION_DIR/expected-first"
same_file "$VERIFICATION_DIR/first" "$VERIFICATION_DIR/expected-first" "first version reconstructed"
status_is "clean: project"

section "COMPARE : see what changed before picking, and what is picked" \
        "old: D[500k:750k] + B[0:250k] + C[0:250k]   new: old + F[0:250k]" \
        "expect bar (both): =...= (750k unchanged) +...+ (250k F appended)"
gen f 0 $QUARTER >> project
out=$(run compare)
printf '%s\n' "$out"
expect_bar "=45 +15" "$out"
run pick project > /dev/null
out=$(run compare --picked)
printf '%s\n' "$out"
expect_bar "=45 +15" "$out"
[[ -z "$("$DUH" compare)" ]] || fail "diff should be empty once everything is staged"
ok "diff is empty once everything is staged"

section "BRING BACK : un-pick, discard, or bring back any version"
run unpick project
status_is "modified: project"
run bring-back project
same_file project "$VERIFICATION_DIR/committed" "restore brings back HEAD's version"
status_is "clean: project"
run bring-back --from "${FIRST:0:12}" project
same_file project "$VERIFICATION_DIR/expected-first" "restore --source (abbreviated hash) brings back the first version"
run bring-back project
status_is "clean: project"

section "NEW + IGNORED : new files show up; hidden and ignored ones don't"
echo "*.tmp" > .duhignore
echo "scratch" > scratch.tmp
echo "hidden" > .hidden-note
echo "draft" > draft.bin
status_is "untracked: draft.bin; clean: project"
rm draft.bin .hidden-note

section "REVERT : bring back an old version as a new save" \
        "expect bar: everything deleted, all of A re-added"
run bring-back --from "$FIRST" project
status_is "modified: project"
stage_expect "-27 +33" project
commit_msg "revert to the first version"
same_file project "$VERIFICATION_DIR/expected-first" "project matches the first version again"

section "MISTAKES : duh refuses and explains"
expect_fail "no files match" pick no-such-file
expect_fail "isn't a file duh knows" bring-back no-such-file
OUTSIDE="${td%/}-outside.bin"
echo "not in the space" > "$OUTSIDE"
expect_fail "outside this space" pick "$OUTSIDE"
expect_fail "outside this space" bring-back ../escape.bin

# ===========================================================================
part "Part 3: several files and directories"
# ===========================================================================

section "ADD A SECOND FILE IN A SUBDIRECTORY : later in history"
mkdir -p assets
gen t 0 $HALF > assets/texture.bin
stage_expect "+60" assets/texture.bin
status_is "staged: assets/texture.bin (new); clean: project"
commit_msg "add assets/texture.bin"

section "EDIT ONE FILE : the other carries forward unchanged" \
        "old: A[0:1M]   new: D[0:250k] + A[0:500k]" \
        "expect bar: +...+ (250k D added) =...= (500k A) -...- (500k A deleted)"
{ gen d 0 $QUARTER; gen a 0 $HALF; } > project
stage_expect "+12 =24 -24" project
status_is "staged: project; clean: assets/texture.bin"
commit_msg "edit project only"
run inspect

section "WORK FROM A SUBDIRECTORY : paths are relative to where you are"
cd assets
{ gen t 0 $HALF; gen u 0 $QUARTER; } > texture.bin
status_is "modified: texture.bin; clean: ../project"
stage_expect "=40 +20" texture.bin
status_is "staged: texture.bin; clean: ../project"
cd ..
commit_msg "append to texture from inside assets/"

section "ABSOLUTE PATHS : pick and bring back by full path"
gen e 0 $QUARTER > notes.bin
stage_expect "+60" "$PWD/notes.bin"
commit_msg "add notes.bin by absolute path"
rm assets/texture.bin
run bring-back "$PWD/assets/texture.bin"
status_is "clean: assets/texture.bin notes.bin project"

# ===========================================================================
part "Part 4: labels, drafts and rewinding"
# ===========================================================================

section "LABEL : name a version and use the name anywhere a version is accepted"
run label release-1
cp project "$VERIFICATION_DIR/release-1"
run label
gen r 0 $QUARTER >> project
"$DUH" pick project > /dev/null
commit_msg "post-release edit"
run bring-back --from release-1 --as "$VERIFICATION_DIR/from-tag" project
same_file "$VERIFICATION_DIR/from-tag" "$VERIFICATION_DIR/release-1" "tag resolves to the tagged version"

section "REFER TO VERSIONS : short hashes, HEAD~N, X^, and history --oneline"
run history --oneline -n 3
[[ "$("$DUH" history --oneline -n 3 | plain | grep -cE '^[0-9a-f]{12} .+')" == 3 ]] ||
    fail "log --oneline should print 3 '<hash> <subject>' lines"
ok "log --oneline prints one '<hash> <subject>' line per commit"
REL=$("$DUH" history -n 1 release-1 | plain | awk '/^version /{print $2}')
for ref in HEAD~1 HEAD^ "${REL:0:7}" "${REL:0:12}"; do
    rm -f "$VERIFICATION_DIR/ref.bin"
    "$DUH" bring-back --from "$ref" --as "$VERIFICATION_DIR/ref.bin" project > /dev/null
    same_file "$VERIFICATION_DIR/ref.bin" "$VERIFICATION_DIR/release-1" "'$ref' names the release-1 commit"
done
expect_fail "past the first save" inspect HEAD~999

section "DRAFTS : diverge, then open each draft in turn"
cp project "$VERIFICATION_DIR/main-project"
run open --new experiment
run drafts
gen x 0 $HALF > project
gen y 0 $QUARTER > assets/experiment-only.bin
"$DUH" pick project assets/experiment-only.bin > /dev/null
commit_msg "experiment: rewrite project, add a file"
cp project "$VERIFICATION_DIR/experiment-project"
run open main
same_file project "$VERIFICATION_DIR/main-project" "main's version of project is back"
[[ ! -e assets/experiment-only.bin ]] || fail "experiment-only file still present on main"
ok "experiment-only file is gone on main"
status_is "clean: assets/texture.bin notes.bin project"
run open experiment
same_file project "$VERIFICATION_DIR/experiment-project" "experiment's version of project is back"
[[ -f assets/experiment-only.bin ]] || fail "experiment-only file missing on experiment"
ok "experiment-only file is back"
gen z 0 100 >> project
expect_fail "unsaved changes" open main
run bring-back project
run open main
expect_fail "the draft you have open" drafts -d main
run drafts maintenance release-1
"$DUH" drafts | plain | grep -qx '  maintenance' || fail "new branch not listed"
ok "branch NAME START creates a branch without switching to it"
run drafts -d maintenance
! "$DUH" drafts | plain | grep -q maintenance || fail "deleted branch still listed"
ok "branch -d deletes it"

section "REWIND --discard : move the draft back and make the files match"
run rewind --discard release-1
same_file project "$VERIFICATION_DIR/release-1" "project is at release-1"
[[ "$(head_commit)" == "$(grep -m1 '' <("$DUH" history -n 1 release-1 | plain | awk '/^version /{print $2}'))" ]] ||
    fail "HEAD is not at release-1"
ok "main points at release-1"
status_is "clean: assets/texture.bin notes.bin project"

section "REWIND --keep-picked : undo a save but keep its changes picked"
gen s 0 $QUARTER >> project
"$DUH" pick project > /dev/null
commit_msg "commit to undo"
UNDONE=$(head_commit)
cp project "$VERIFICATION_DIR/undone"
run rewind --keep-picked release-1
status_is "staged: project; clean: assets/texture.bin notes.bin"
run save -m "redo the undone commit"
same_file project "$VERIFICATION_DIR/undone" "re-committing restores the same content"
record

section "REWIND : undo a save, keep the edits as un-picked changes"
run rewind release-1
status_is "modified: project; clean: assets/texture.bin notes.bin"
run bring-back project
same_file project "$VERIFICATION_DIR/release-1" "discarding returns to release-1"

# ===========================================================================
part "Part 5: renaming, removing and bulk changes"
# ===========================================================================

section "RENAME : rename a file; its content isn't stored again"
before=$(store_bytes)
cp assets/texture.bin "$VERIFICATION_DIR/texture"
run rename assets/texture.bin media/texture.bin
status_is "staged: assets/texture.bin (deleted) media/texture.bin (new); clean: notes.bin project"
commit_msg "rename texture into media/"
same_file media/texture.bin "$VERIFICATION_DIR/texture" "renamed file has the same content"
in_commit HEAD assets/texture.bin && fail "old path still in the commit"
ok "old path is gone from the commit"
growth=$(( $(store_bytes) - before ))
size=$(stat -c %s media/texture.bin)
(( growth * 20 < size )) || fail "renaming a ${size}-byte file grew the store by $growth bytes"
ok "renaming a ${size}-byte file grew the store by only $growth bytes"

section "DELETE : stop tracking a file and delete it"
cp notes.bin "$VERIFICATION_DIR/notes"
run delete notes.bin
[[ ! -e notes.bin ]] || fail "notes.bin still on disk"
status_is "staged: notes.bin (deleted); clean: media/texture.bin project"
commit_msg "remove notes.bin"
in_commit HEAD notes.bin && fail "notes.bin still in the commit"
ok "notes.bin is gone from the commit"
run bring-back --from HEAD~1 --as "$VERIFICATION_DIR/notes-back" notes.bin
same_file "$VERIFICATION_DIR/notes-back" "$VERIFICATION_DIR/notes" "history still has notes.bin"

section "FORGET : stop tracking a file but keep it on disk"
gen k 0 1000 > keep.bin
run pick keep.bin > /dev/null
commit_msg "add keep.bin"
echo "local edit" >> keep.bin
expect_fail "unsaved changes" delete keep.bin
run forget keep.bin
status_is "staged: keep.bin (deleted); untracked: keep.bin; clean: media/texture.bin project"
commit_msg "untrack keep.bin"
status_is "untracked: keep.bin; clean: media/texture.bin project"
rm keep.bin

section "PICK -u / PICK -A : pick everything at once"
gen n 0 1000 > new-a.bin
gen m 0 1000 > new-b.bin
gen p 0 1000 >> project
run pick -u
status_is "staged: project; untracked: new-a.bin new-b.bin; clean: media/texture.bin"
run pick -A
status_is "staged: new-a.bin (new) new-b.bin (new) project; clean: media/texture.bin"
commit_msg "add -A: two new files and an edit"

section "SAVE -a : save every tracked change without picking first"
gen q 0 1000 >> project
rm new-b.bin
run save -a -m "commit -a: an edit and a deletion"
record
in_commit HEAD new-b.bin && fail "new-b.bin still in the commit"
status_is "clean: media/texture.bin new-a.bin project"

section "GIT NAMES : every git command name still works"
cp project "$VERIFICATION_DIR/before-alias"
gen g 0 1000 >> project
run add project > /dev/null
status_is "staged: project; clean: media/texture.bin new-a.bin"
run diff --staged > /dev/null
run commit -m "saved with git's names"
record
[[ "$("$DUH" log --oneline -n 1 | plain)" == *"saved with git's names" ]] || fail "log alias"
ok "add, status, diff, commit and log work as aliases"
run checkout -c HEAD~1 --as "$VERIFICATION_DIR/alias.bin" project
same_file "$VERIFICATION_DIR/alias.bin" "$VERIFICATION_DIR/before-alias" "checkout -c still works"

# ===========================================================================
part "Part 6: get a copy"
# ===========================================================================

CLONE="${td%/}-clone"
section "GET : copy the space and its files into a new folder"
run get "$td" "$CLONE"
for f in project new-a.bin media/texture.bin; do
    same_file "$CLONE/$f" "$f" "clone has the same $f"
done
(cd "$CLONE" && status_is "clean: media/texture.bin new-a.bin project")

# ===========================================================================
part "Part 7: places — a shared hub and real S3"
# A remote is any store opendal can reach. Here: a bare space in a local
# directory, then an S3 bucket served over the real S3 protocol by rclone.
# Copying only ever fast-forwards: duh never merges.
# ===========================================================================

head_of() { (cd "$1" && "$DUH" history -n 1 | plain | awk '/^version /{print $2; exit}'); }

HUB="${td%/}-hub"
MATE="${td%/}-teammate"

section "SEND : send your draft to a bare hub"
mkdir -p "$HUB"
(cd "$HUB" && run start --bare)
run places add origin "$HUB"
run send
[[ "$(head_of "$HUB")" == "$(head_commit)" ]] || fail "hub's main isn't at our HEAD"
ok "hub's main is at our HEAD"

section "RECEIVE : a teammate's save comes back"
run get "$HUB" "$MATE"
(cd "$MATE" && gen t1 0 1000 >> project && "$DUH" save -a -m "teammate edit" > /dev/null && run send)
run receive origin
same_file project "$MATE/project" "the teammate's edit arrived"
record

section "DIVERGE : duh refuses to drop anyone's saves"
gen l1 0 1000 >> project
"$DUH" save -a -m "our edit" > /dev/null
record
(cd "$MATE" && gen t2 0 1000 >> project && "$DUH" save -a -m "teammate edit 2" > /dev/null && run send)
expect_fail "has saves you don't" send
expect_fail "doesn't merge" receive origin
run send --force
[[ "$(head_of "$HUB")" == "$(head_commit)" ]] || fail "--force didn't move the hub"
ok "copy --force overwrote the hub"
(cd "$MATE" && expect_fail "doesn't merge" receive origin)
(cd "$MATE" && run rewind --discard HEAD~1 && echo "dirty" >> project &&
    expect_fail "unsaved changes" receive origin &&
    run bring-back project && run receive origin)
same_file "$MATE/project" project "teammate caught up after dropping their commit"

section "CREDENTIALS : storage options live in the settings, in plain text"
expect_fail "isn't a backend duh supports" places add cache redis://localhost:6379

command -v rclone > /dev/null || fail "this part needs rclone (nix flake check and \`just demo_script\` provide it)"
S3_DATA="${td%/}-s3"
mkdir -p "$S3_DATA/duh-bucket"
S3_PORT=$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')
HOME="$S3_DATA" rclone serve s3 --addr "127.0.0.1:$S3_PORT" --auth-key duh-key,duh-secret \
    "$S3_DATA" > "$VERIFICATION_DIR/rclone.log" 2>&1 &
RCLONE_PID=$!
trap 'kill $RCLONE_PID 2> /dev/null || true' EXIT
python3 - "$S3_PORT" <<'PY' || fail "S3 server didn't start"
import socket, sys, time
for _ in range(100):
    try:
        socket.create_connection(("127.0.0.1", int(sys.argv[1])), 0.2).close(); sys.exit(0)
    except OSError:
        time.sleep(0.1)
sys.exit(1)
PY
S3_OPTS=(-o "endpoint=http://127.0.0.1:$S3_PORT" -o region=us-east-1
         -o access_key_id=duh-key -o secret_access_key=duh-secret)
run places add warehouse s3://duh-bucket/game "${S3_OPTS[@]}"
[[ "$("$DUH" settings get remote.warehouse.config.secret_access_key)" == "duh-secret" ]] ||
    fail "secret not stored"
ok "credentials stored under [remote.warehouse.config]"

section "S3 SEND : the first send to an empty bucket prefix creates the space"
run send warehouse
[[ -f "$S3_DATA/duh-bucket/game/config" ]] || fail "no space created in the bucket"
ok "a bare space now lives at s3://duh-bucket/game"

S3_CLONE="${td%/}-from-s3"
section "S3 GET : get a copy straight from the bucket"
run get s3://duh-bucket/game "$S3_CLONE" "${S3_OPTS[@]}"
for f in project new-a.bin media/texture.bin; do
    same_file "$S3_CLONE/$f" "$f" "S3 clone has the same $f"
done

section "S3 ROUND TRIP : a small edit moves only what changed"
s3_bytes() { find "$S3_DATA/duh-bucket" -type f -printf '%s\n' | awk '{s += $1} END {print s + 0}'; }
before=$(s3_bytes)
(cd "$S3_CLONE" && python3 -c "f = open('media/texture.bin', 'r+b'); f.seek(300000); f.write(b'edit' * 1000)" &&
    "$DUH" save -a -m "edit texture via S3 clone" > /dev/null && run send)
growth=$(( $(s3_bytes) - before ))
size=$(stat -c %s media/texture.bin)
(( growth * 20 < size )) || fail "a 4 KB edit to a ${size}-byte file added $growth bytes to the bucket"
ok "a 4 KB edit to a ${size}-byte file added only $growth bytes to the bucket"
run receive warehouse
same_file media/texture.bin "$S3_CLONE/media/texture.bin" "the edit came back from S3"
record

# ===========================================================================
part "Part 8: big files and long histories"
# Each section works in a space of its own.
# ===========================================================================

section "UNIFORM RUN : a 1-byte edit inside 8 MB of zeros stores only what's near it" \
        "(content-defined chunks are cut at 8 x chunk_size, even with no boundary)"
UNIFORM="${td%/}-uniform"
mkdir -p "$UNIFORM"
(
    cd "$UNIFORM"
    "$DUH" start > /dev/null
    head -c 8388608 /dev/zero > canvas.raw
    "$DUH" pick canvas.raw > /dev/null
    "$DUH" save -m "blank canvas" > /dev/null
    before=$(store_bytes)
    python3 -c "f = open('canvas.raw', 'r+b'); f.seek(4194304); f.write(b'X')"
    run save -a -m "one dot"
    growth=$(( $(store_bytes) - before ))
    (( growth < 262144 )) || fail "a 1-byte edit to 8 MB of zeros added $growth bytes"
    ok "a 1-byte edit to 8 MB of zeros added only $growth bytes"
    run bring-back --from HEAD^ --as "$VERIFICATION_DIR/blank.raw" canvas.raw
    cmp -s "$VERIFICATION_DIR/blank.raw" <(head -c 8388608 /dev/zero) || fail "blank canvas didn't come back"
    ok "the blank canvas comes back exactly"
    rm -f "$VERIFICATION_DIR/blank.raw"
)

section "SOLID COPIES : after 100 saves since the last solid copy, the middle one is made solid" \
        "(a solid save reads without replaying the diffs before it, so reading" \
        " any save replays at most ~100 diffs, however long the history gets)"
LONG="${td%/}-long"
LONG_COPY="${td%/}-long-copy"
mkdir -p "$LONG"
(
    cd "$LONG"
    "$DUH" start > /dev/null
    gen L 0 200000 > sketch.psd
    "$DUH" pick sketch.psd > /dev/null
    "$DUH" save -m "save 1" > /dev/null
    shas=("$(sha sketch.psd)")
    for i in $(seq 2 110); do
        python3 -c "import random; r = random.Random($i); f = open('sketch.psd', 'r+b'); f.seek(r.randrange(199000)); f.write(r.randbytes(64))"
        "$DUH" save -a -m "save $i" > /dev/null
        shas+=("$(sha sketch.psd)")
    done
    ok "110 saves of sketch.psd"

    # Save 1 diffs against nothing; saves 2-101 are the first 100 since then,
    # and the 50th of those is save 51.
    solids=$(ls .duh/solid)
    [[ "$solids" == "${shas[50]}" ]] || fail "expected only save 51 to be solid, found: $solids"
    ok "save 51 (the middle of saves 2-101) is solid, and nothing else yet"

    mapfile -t versions < <("$DUH" history | plain | awk '/^version /{print $2}' | tac)
    for i in "${!versions[@]}"; do
        "$DUH" bring-back --from "${versions[$i]}" --as "$VERIFICATION_DIR/long.bin" sketch.psd > /dev/null
        [[ "$(sha "$VERIFICATION_DIR/long.bin")" == "${shas[$i]}" ]] || fail "save $((i + 1)) differs"
    done
    ok "all 110 saves come back exactly"

    run get "$LONG" "$LONG_COPY"
    [[ -f "$LONG_COPY/.duh/solid/${shas[50]}" ]] || fail "the copy didn't get the solid copy"
    ok "get brings the solid copy along"
    cd "$LONG_COPY"
    for i in "${!versions[@]}"; do
        "$DUH" bring-back --from "${versions[$i]}" --as "$VERIFICATION_DIR/long.bin" sketch.psd > /dev/null
        [[ "$(sha "$VERIFICATION_DIR/long.bin")" == "${shas[$i]}" ]] || fail "save $((i + 1)) differs in the copy"
    done
    ok "all 110 saves come back exactly in the copy"
    rm -f "$VERIFICATION_DIR/long.bin"
)

# ===========================================================================
part "VERIFICATION: every file at every save, byte for byte"
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
            (cd "$dir" && "$DUH" bring-back --from "$c" --as "$out" "$path" > /dev/null) ||
                fail "checkout of $path at ${c:0:12} failed"
            [[ "$(sha "$out")" == "$want" ]] || fail "$path at ${c:0:12} differs"
            files=$((files + 1))
        done < <(grep "^$c|" "$EXPECT")
        commits=$((commits + 1))
    done
    ok "$commits saves, $files file versions brought back exactly"
}

mapfile -t all_commits < <(cut -d'|' -f1 "$EXPECT" | awk '!seen[$0]++')
echo "In this space (every draft, including saves rewound away):"
verify_commits "$td" "${all_commits[@]}"

mapfile -t clone_commits < <(cd "$CLONE" && "$DUH" history | plain | awk '/^version /{print $2}')
echo "In the copy made with get (its history):"
verify_commits "$CLONE" "${clone_commits[@]}"

mapfile -t s3_commits < <(cd "$S3_CLONE" && "$DUH" history | plain | awk '/^version /{print $2}')
echo "In the copy fetched from S3 (its history):"
verify_commits "$S3_CLONE" "${s3_commits[@]}"

rm -f "$out"
echo ""
echo "  object store size: $(du -sh .duh/objects | cut -f1)"
echo ""
green "SUCCESS: every outcome above was checked and held."
