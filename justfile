a := `mktemp`
b := `mktemp`
c := `mktemp`
d := `mktemp`

generate-test-files outdir:
    dd if=/dev/urandom count=1 bs=50M of={{ a }}
    dd if=/dev/urandom count=2 bs=50M of={{ b }} 
    dd if=/dev/urandom count=3 bs=50M of={{ c }} 
    dd if=/dev/urandom count=4 bs=50M of={{ d }} 

    rm -rf test-data
    mkdir test-data

    cat {{ a }} {{ b }} {{ d }} > {{ outdir }}/abd
    cat {{ a }} {{ c }} {{ d }} > {{ outdir }}/acd

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

tmpdir := `mktemp -d`

demo_script td:
    bash demo.sh {{ td }}

demo: (generate-test-files tmpdir)
    nix develop .#duh --command bash -c "bash demo.sh {{ tmpdir }}"

v := `nix eval --raw .#version.x86_64-linux`

version:
    git tag {{v}}
