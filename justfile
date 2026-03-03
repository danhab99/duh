build:
    cd cli && cargo build

# Creates 4 large single-character files in outdir: a.txt b.txt c.txt d.txt
generate-test-files outdir size="200000":
    python3 -c "import sys; sys.stdout.buffer.write(b'a' * {{size}})" > {{ outdir }}/a.txt
    python3 -c "import sys; sys.stdout.buffer.write(b'b' * {{size}})" > {{ outdir }}/b.txt
    python3 -c "import sys; sys.stdout.buffer.write(b'c' * {{size}})" > {{ outdir }}/c.txt
    python3 -c "import sys; sys.stdout.buffer.write(b'd' * {{size}})" > {{ outdir }}/d.txt

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

demo:
    nix develop .#duh --command bash -c "bash demo.sh {{ tmpdir }}"

v := `nix eval --raw .#version.x86_64-linux`

version:
    git add ./cli/Cargo.toml 
    git commit -m "{{v}}"
    git tag {{v}}

