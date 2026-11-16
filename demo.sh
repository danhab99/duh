#!/usr/bin/env bash
set -euo pipefail

td="${1:?Usage: demo.sh <test-dir>}"
DUH="${DUH:-duh}"

VERIFICATION_DIR="$td/.verification"
HASH_LOG="$VERIFICATION_DIR/hashes.log"

dim()   { printf '\033[2m%s\033[0m\n' "$*"; }
green() { printf '\033[32m%s\033[0m\n' "$*"; }
red()   { printf '\033[31m%s\033[0m\n' "$*"; }

# Wrapper: runs duh under GNU time and prints peak RSS after each command.
# time stats go to a temp file (-o) so duh's own stdout/stderr are unaffected.
run() {
    local timefile
    timefile=$(mktemp)

    echo
    echo
    echo "=== Executing duh $* ==="

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

# Record the sha256 of `project` for the commit that was just made.
store_hash() {
    local commit_msg="$1"
    local hash
    hash=$(sha256sum project | cut -d' ' -f1)
    echo "$commit_msg|$hash" >> "$HASH_LOG"
    dim "  ✓ stored hash: $hash for '$commit_msg'"
}

commit() {
    run stage project
    run commit -m "$1"
    store_hash "$1"
}

echo "TEST PATH $td"
echo "duh binary: $(command -v "$DUH")"

cd "$td"

mkdir -p "$VERIFICATION_DIR"
echo "# Hash verification log: commit_message|sha256_hash" > "$HASH_LOG"

SIZE=1000000
HALF=$((SIZE / 2))
QUARTER=$((SIZE / 4))

run init

# ---------------------------------------------------------------------------
# Part 1: basic workflow
# Streams A, B, C, D are unrelated random data; slices of one stream match.
# ---------------------------------------------------------------------------

# --- commit 1: brand-new file → bar should be all ADDED ---
gen a 0 $SIZE > project
run status
run stage project
run status
run commit -m "commit 1: all A"
store_hash "commit 1: all A"
run show

# --- commit 2: unrelated content → all DELETED then all ADDED ---
gen b 0 $SIZE > project
run status
commit "commit 2: all B"
run show

# --- commit 3: first half new (C), second half kept (B) ---
{ gen c 0 $HALF; gen b $HALF $HALF; } > project
echo
echo " expect bar: -...- (500k B deleted) +...+ (500k C added) =...= (500k B unchanged)"
commit "commit 3: half C, half B"
run show

# --- commit 4: unrelated content again ---
gen d 0 $SIZE > project
commit "commit 4: all D"
run show

run log

# ---------------------------------------------------------------------------
# Part 2: every ADDED / UNCHANGED / DELETED ordering
#
# State entering here: project = D[0:1M]
#   +  = ADDED     (new bytes not present in previous version)
#   =  = UNCHANGED (bytes identical to previous version)
#   -  = DELETED   (bytes present in previous version, now gone)
# Chunk boundaries are content-defined (~2-4 KiB), so a region edge can be
# off by one chunk; at 60 columns that is far below one column.
# ---------------------------------------------------------------------------

echo ""
echo "================================================================"
echo " Part 2: exhaustive fragment-combination showcase"
echo "    bar key:  + added   = unchanged   - deleted"
echo "================================================================"

section() {
    echo ""
    echo "--------------------------------------------------------------------"
    for line in "$@"; do echo " $line"; done
    echo "--------------------------------------------------------------------"
}

section "UNCHANGED + ADDED : append new content at the end" \
        "old: D[0:1M]   new: D[0:1M] + A[0:500k]" \
        "expect bar: =...= (1M D, unchanged) +...+ (500k A, added)"
{ gen d 0 $SIZE; gen a 0 $HALF; } > project
commit "unchanged+added: append A"

section "UNCHANGED + DELETED : trim content from the end" \
        "old: D[0:1M] + A[0:500k]   new: D[0:1M]" \
        "expect bar: =...= (1M D, unchanged) -...- (500k A, deleted)"
gen d 0 $SIZE > project
commit "unchanged+deleted: drop appended A"

section "ADDED + UNCHANGED : prepend new content at the start" \
        "old: D[0:1M]   new: B[0:500k] + D[0:1M]" \
        "expect bar: +...+ (500k B, added) =...= (1M D, unchanged)"
{ gen b 0 $HALF; gen d 0 $SIZE; } > project
commit "added+unchanged: prepend B"

section "DELETED + UNCHANGED : remove content from the start" \
        "old: B[0:500k] + D[0:1M]   new: D[0:1M]" \
        "expect bar: -...- (500k B, deleted) =...= (1M D, unchanged)"
gen d 0 $SIZE > project
commit "deleted+unchanged: drop prepended B"

section "UNCHANGED + ADDED + UNCHANGED : insert content in the middle" \
        "old: D[0:1M]   new: D[0:500k] + B[0:500k] + D[500k:1M]" \
        "expect bar: =...= (500k D) +...+ (500k B, inserted) =...= (500k D)"
{ gen d 0 $HALF; gen b 0 $HALF; gen d $HALF $HALF; } > project
commit "unchanged+added+unchanged: insert B in middle"

section "UNCHANGED + DELETED + UNCHANGED : remove content from the middle" \
        "old: D[0:500k] + B[0:500k] + D[500k:1M]   new: D[0:1M]" \
        "expect bar: =...= (500k D) -...- (500k B, deleted) =...= (500k D)"
gen d 0 $SIZE > project
commit "unchanged+deleted+unchanged: remove inserted B"

section "DELETED + ADDED + UNCHANGED : replace the first half" \
        "old: D[0:1M]   new: A[0:500k] + D[500k:1M]" \
        "expect bar: -...- (500k D, deleted) +...+ (500k A, added) =...= (500k D)"
{ gen a 0 $HALF; gen d $HALF $HALF; } > project
commit "deleted+added+unchanged: replace first half with A"

section "DELETED + UNCHANGED + ADDED : remove prefix, keep core, add suffix" \
        "old: A[0:500k] + D[500k:1M]   new: D[500k:1M] + C[0:250k]" \
        "expect bar: -...- (500k A, deleted) =...= (500k D) +...+ (250k C, added)"
{ gen d $HALF $HALF; gen c 0 $QUARTER; } > project
commit "deleted+unchanged+added: drop A prefix, keep D, add C suffix"

section "UNCHANGED + DELETED + ADDED + UNCHANGED : setup three sections" \
        "old: D[500k:1M] + C[0:250k]   new: D[500k:750k] + A[0:250k] + C[0:250k]" \
        "expect bar: =...= (250k D) -...- (250k D) +...+ (250k A) =...= (250k C)"
{ gen d $HALF $QUARTER; gen a 0 $QUARTER; gen c 0 $QUARTER; } > project
commit "setup: D-A-C three sections"

section "UNCHANGED + DELETED + ADDED + UNCHANGED : replace the middle section" \
        "old: D[500k:750k] + A[0:250k] + C[0:250k]   new: D[500k:750k] + B[0:250k] + C[0:250k]" \
        "expect bar: =...= (250k D) -...- (250k A, deleted) +...+ (250k B, added) =...= (250k C)"
{ gen d $HALF $QUARTER; gen b 0 $QUARTER; gen c 0 $QUARTER; } > project
commit "unchanged+deleted+added+unchanged: replace A section with B"

run log

# ---------------------------------------------------------------------------
# Verification: check out every commit from history and compare its sha256
# with the hash recorded when it was committed.
# ---------------------------------------------------------------------------

echo ""
echo "================================================================"
echo " VERIFICATION: checking out every commit and comparing sha256"
echo "================================================================"

pass=0
fail=0
out="$VERIFICATION_DIR/checkout.bin"

# `duh log` prints "commit <hash>", "Author: ...", "    <message>" per entry.
while IFS='|' read -r commit_hash commit_msg; do
    expected=$(grep -F "$commit_msg|" "$HASH_LOG" | head -n1 | cut -d'|' -f2)
    rm -f "$out"
    if ! "$DUH" checkout -c "$commit_hash" --as "$out" project > /dev/null; then
        red "  ✗ checkout failed: $commit_msg"
        fail=$((fail + 1))
        continue
    fi
    actual=$(sha256sum "$out" | cut -d' ' -f1)
    if [[ "$actual" == "$expected" ]]; then
        green "  ✓ ${commit_hash:0:12}  $commit_msg"
        pass=$((pass + 1))
    else
        red "  ✗ ${commit_hash:0:12}  $commit_msg"
        echo "      expected $expected"
        echo "      actual   $actual"
        fail=$((fail + 1))
    fi
done < <("$DUH" log | sed 's/\x1b\[[0-9;]*m//g' |
         awk '/^commit /{h=$2; getline; getline; sub(/^ +/, ""); print h "|" $0}')

rm -f "$out"

stored=$(($(wc -l < "$HASH_LOG") - 1))
echo ""
echo "  commits verified: $pass/$stored"
echo "  object store size: $(du -sh .duh/objects | cut -f1)"

echo ""
if [[ $fail -eq 0 && $pass -eq $stored ]]; then
    green "SUCCESS: every commit was reconstructed byte-for-byte from history."
else
    red "FAILURE: $fail commit(s) failed verification ($pass/$stored passed)."
    exit 1
fi
