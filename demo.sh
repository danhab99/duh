#!/usr/bin/env bash
set -euo pipefail

td="${1:?Usage: demo.sh <test-dir>}"

# Wrapper: runs duh under GNU time and prints peak RSS after each command.
# time stats go to a temp file (-o) so duh's own stdout/stderr are unaffected.
run() {
    local timefile
    timefile=$(mktemp)
    command time -v -o "$timefile" duh "$@"
    local kb
    kb=$(grep "Maximum resident" "$timefile" | awk '{print $NF}')
    printf "  \033[2m↳ peak RAM: %d MiB\033[0m\n" "$((kb / 1024))"
    rm -f "$timefile"
}

echo "TEST PATH $td"

cd "$td"
duh init

cat abd > file
run status
run stage file
run status
run commit -m "commit 1"

run show

cat acd > file
run status
run stage file
run status
run commit -m "commit 2"

run show
run log
