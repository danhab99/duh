# Build the release binary through the flake (deps come from cli/Cargo.lock via Nix).
build:
    nix build .#duh

# Creates 4 large single-character files in outdir: a.txt b.txt c.txt d.txt
generate-test-files outdir size="200000":
    python3 -c "import sys; sys.stdout.buffer.write(b'a' * {{size}})" > {{ outdir }}/a.txt
    python3 -c "import sys; sys.stdout.buffer.write(b'b' * {{size}})" > {{ outdir }}/b.txt
    python3 -c "import sys; sys.stdout.buffer.write(b'c' * {{size}})" > {{ outdir }}/c.txt
    python3 -c "import sys; sys.stdout.buffer.write(b'd' * {{size}})" > {{ outdir }}/d.txt

# Run demo.sh against the Nix-built binary (in a shell that also provides
# rclone for the S3 part), in td or else a fresh temporary folder.
demo_script td="":
    ./demo.sh {{ td }}

# Run demo.sh in the Nix sandbox (the standard duh must meet).
demo:
    nix build .#checks.x86_64-linux.demo --no-link -L

# Build, test and run the demo — everything the flake checks.
check:
    nix flake check -L

v := `nix eval --raw .#version.x86_64-linux`

version:
    git add ./cli/Cargo.toml 
    git commit -m "{{v}}"
    git tag {{v}}

