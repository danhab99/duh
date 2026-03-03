#!/usr/bin/env bash
set -euo pipefail

td="${1:?Usage: demo.sh <test-dir>}"

# Wrapper: runs duh under GNU time and prints peak RSS after each command.
# time stats go to a temp file (-o) so duh's own stdout/stderr are unaffected.
run() {
    local timefile
    timefile=$(mktemp)

    echo
    echo
    echo "=== Executing duh $@ ==="

    command time -v -o "$timefile" duh "$@"
    local kb
    kb=$(grep "Maximum resident" "$timefile" | awk '{print $NF}')
    printf "  \033[2m↳ peak RAM: %d MiB\033[0m\n" "$((kb / 1024))"
    rm -f "$timefile"
}

echo "TEST PATH $td"

cd "$td"

# ---------------------------------------------------------------------------
# Create 4 large files, each filled with a single repeated character.
# 200 000 bytes each so the progress bar has room to show proportions clearly.
# ---------------------------------------------------------------------------
SIZE=200000
python3 -c "import sys; sys.stdout.buffer.write(b'a' * $SIZE)" > a.txt
python3 -c "import sys; sys.stdout.buffer.write(b'b' * $SIZE)" > b.txt
python3 -c "import sys; sys.stdout.buffer.write(b'c' * $SIZE)" > c.txt
python3 -c "import sys; sys.stdout.buffer.write(b'd' * $SIZE)" > d.txt
echo "Created: a.txt b.txt c.txt d.txt  (${SIZE} bytes each)"

duh init

# --- commit 1: brand-new file, all a's → progress bar should be solid green (ADDED) ---
cp a.txt project
run status
run stage project
run status
run commit -m "commit 1: all a"
run show

# --- commit 2: replace with all b's → expect fully DELETED then fully ADDED ---
cp b.txt project
run status
run stage project
run status
run commit -m "commit 2: all b"
run show

# --- commit 3: first half c's, second half b's → first half ADDED/DELETED, second half UNCHANGED ---
python3 -c "import sys; sys.stdout.buffer.write(b'c' * ($SIZE // 2) + b'b' * ($SIZE // 2))" > project
run status
run stage project
run status
run commit -m "commit 3: half c, half b"
run show

# --- commit 4: replace with all d's ---
cp d.txt project
run status
run stage project
run status
run commit -m "commit 4: all d"
run show

run log
# ---------------------------------------------------------------------------
# Part 2: every ADDED / UNCHANGED / DELETED ordering
#
# State entering here: project = d * SIZE (200 000 bytes of 'd')
#
# Bar legend:
#   green  +  = ADDED    (new bytes not present in previous version)
#   grey   =  = UNCHANGED (bytes identical to previous version, kept in place)
#   red    -  = DELETED  (bytes present in previous version, now gone)
# ---------------------------------------------------------------------------
HALF=$((SIZE / 2))
QUARTER=$((SIZE / 4))

echo ""
echo "================================================================"
echo " Part 2: exhaustive fragment-combination showcase"
echo "    bar key:  + added   = unchanged   - deleted"
echo "================================================================"

# ---- 1. UNCHANGED + ADDED -----------------------------------------------
echo ""
echo "--------------------------------------------------------------------"
echo " UNCHANGED + ADDED : append new content at the end"
echo " old: d*200k   new: d*200k + a*100k"
echo " expect bar: =...= (200k d, unchanged) +...+ (100k a, added)"
echo "--------------------------------------------------------------------"
python3 -c "import sys; sys.stdout.buffer.write(b'd' * $SIZE + b'a' * $HALF)" > project
run stage project
run commit -m "unchanged+added: append a's"

# ---- 2. UNCHANGED + DELETED ---------------------------------------------
echo ""
echo "--------------------------------------------------------------------"
echo " UNCHANGED + DELETED : trim content from the end"
echo " old: d*200k + a*100k   new: d*200k"
echo " expect bar: =...= (200k d, unchanged) -...- (100k a, deleted)"
echo "--------------------------------------------------------------------"
cp d.txt project
run stage project
run commit -m "unchanged+deleted: drop appended a's"

# ---- 3. ADDED + UNCHANGED -----------------------------------------------
echo ""
echo "--------------------------------------------------------------------"
echo " ADDED + UNCHANGED : prepend new content at the start"
echo " old: d*200k   new: b*100k + d*200k"
echo " expect bar: +...+ (100k b, added) =...= (200k d, unchanged)"
echo "--------------------------------------------------------------------"
python3 -c "import sys; sys.stdout.buffer.write(b'b' * $HALF + b'd' * $SIZE)" > project
run stage project
run commit -m "added+unchanged: prepend b's"

# ---- 4. DELETED + UNCHANGED ---------------------------------------------
echo ""
echo "--------------------------------------------------------------------"
echo " DELETED + UNCHANGED : remove content from the start"
echo " old: b*100k + d*200k   new: d*200k"
echo " expect bar: -...- (100k b, deleted) =...= (200k d, unchanged)"
echo "--------------------------------------------------------------------"
cp d.txt project
run stage project
run commit -m "deleted+unchanged: drop prepended b's"

# ---- 5. UNCHANGED + ADDED + UNCHANGED -----------------------------------
echo ""
echo "--------------------------------------------------------------------"
echo " UNCHANGED + ADDED + UNCHANGED : insert content in the middle"
echo " old: d*200k   new: d*100k + b*100k + d*100k"
echo " expect bar: =...= (100k d) +...+ (100k b, inserted) =...= (100k d)"
echo "--------------------------------------------------------------------"
python3 -c "import sys; sys.stdout.buffer.write(b'd' * $HALF + b'b' * $HALF + b'd' * $HALF)" > project
run stage project
run commit -m "unchanged+added+unchanged: insert b's in middle"

# ---- 6. UNCHANGED + DELETED + UNCHANGED ---------------------------------
echo ""
echo "--------------------------------------------------------------------"
echo " UNCHANGED + DELETED + UNCHANGED : remove content from the middle"
echo " old: d*100k + b*100k + d*100k   new: d*200k"
echo " expect bar: =...= (100k d) -...- (100k b, deleted) =...= (100k d)"
echo "--------------------------------------------------------------------"
cp d.txt project
run stage project
run commit -m "unchanged+deleted+unchanged: remove inserted b's"

# ---- 7. DELETED + UNCHANGED + ADDED -------------------------------------
# Setup: transition d*200k → a*100k + d*100k.
# The diff finds the d-block at position 0 in old, so it sees ADDED(a*100k) + UNCHANGED(d*100k).
# This also demonstrates ADDED + UNCHANGED a second time with different proportions.
echo ""
echo "--------------------------------------------------------------------"
echo " Setup for DELETED+UNCHANGED+ADDED"
echo " old: d*200k   new: a*100k + d*100k"
echo " expect bar: +...+ (100k a, added) =...= (100k d, unchanged)"
echo "--------------------------------------------------------------------"
python3 -c "import sys; sys.stdout.buffer.write(b'a' * $HALF + b'd' * $HALF)" > project
run stage project
run commit -m "setup: a-prefix + d-block"

echo ""
echo "--------------------------------------------------------------------"
echo " DELETED + UNCHANGED + ADDED : remove prefix, keep core, add suffix"
echo " old: a*100k + d*100k   new: d*100k + c*50k"
echo " expect bar: -...- (100k a, deleted) =...= (100k d, unchanged) +...+ (50k c, added)"
echo "--------------------------------------------------------------------"
python3 -c "import sys; sys.stdout.buffer.write(b'd' * $HALF + b'c' * $QUARTER)" > project
run stage project
run commit -m "deleted+unchanged+added: drop a-prefix, keep d's, add c-suffix"

# ---- 8. UNCHANGED + DELETED + ADDED + UNCHANGED -------------------------
# Setup: transition d*100k + c*50k → d*50k + a*50k + c*50k.
# old has extra 50k d's that no longer exist → UNCHANGED(50k d) + DELETED(50k d) + ADDED(50k a) + UNCHANGED(50k c)
# This is also a complete UNCHANGED+DELETED+ADDED+UNCHANGED in one shot.
echo ""
echo "--------------------------------------------------------------------"
echo " Setup for clean UNCHANGED+DELETED+ADDED+UNCHANGED"
echo " old: d*100k + c*50k   new: d*50k + a*50k + c*50k"
echo " expect bar: =...= (50k d) -...- (50k d gone) +...+ (50k a added) =...= (50k c)"
echo "--------------------------------------------------------------------"
python3 -c "import sys; sys.stdout.buffer.write(b'd' * $QUARTER + b'a' * $QUARTER + b'c' * $QUARTER)" > project
run stage project
run commit -m "setup: d-a-c three sections"

echo ""
echo "--------------------------------------------------------------------"
echo " UNCHANGED + DELETED + ADDED + UNCHANGED : replace the middle section"
echo " old: d*50k + a*50k + c*50k   new: d*50k + b*50k + c*50k"
echo " expect bar: =...= (50k d) -...- (50k a, deleted) +...+ (50k b, added) =...= (50k c)"
echo "--------------------------------------------------------------------"
python3 -c "import sys; sys.stdout.buffer.write(b'd' * $QUARTER + b'b' * $QUARTER + b'c' * $QUARTER)" > project
run stage project
run commit -m "unchanged+deleted+added+unchanged: replace a-section with b-section"

run log

echo ""
echo "================================================================"
echo " All fragment orderings demonstrated."
echo "================================================================"