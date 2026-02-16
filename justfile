a := `mktemp`
b := `mktemp`
c := `mktemp`
d := `mktemp`

generate-test-files:
    dd if=/dev/urandom count=10 bs=1M of={{ a }}
    dd if=/dev/urandom count=10 bs=1M of={{ b }} 
    dd if=/dev/urandom count=10 bs=1M of={{ c }} 
    dd if=/dev/urandom count=10 bs=1M of={{ d }} 

    rm -rf test-data
    mkdir test-data

    cat {{ a }} {{ b }} {{ d }} > cli/test-data/abd
    cat {{ a }} {{ c }} {{ d }} > cli/test-data/acd

update-vendor-hash:
    #!/usr/bin/env bash
    set -euo pipefail
    cd cli
    cargo vendor --versioned-dirs 2>&1 | head -n1
    cd ..
    hash=$(nix hash path cli/vendor)
    echo ""
    echo "Vendor hash for flake.nix:"
    echo "  cargoVendorHash = \"$hash\";"
