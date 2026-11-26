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


# Launch duh Studio, the desktop app (built by Nix).
studio:
    nix run .#studio

# Regenerate the Dart <-> Rust bindings after changing studio/native/src/api.
studio-codegen:
    cd studio && nix develop ..#studio --command flutter_rust_bridge_codegen generate

# duh Studio's tests: widget tests against a fake engine, and the real
# engine through the bridge.
studio-test:
    cd studio/native && nix develop ../..#studio --command cargo test
    cd studio/native && nix develop ../..#studio --command cargo build
    cd studio && nix develop ..#studio --command flutter test

# Render duh Studio's main screens to studio/test_screens/out/*.png.
studio-screens:
    cd studio && nix develop ..#studio --command bash -c 'flutter test --update-goldens --dart-define=ICONS=$(dirname $(dirname $(readlink -f $(which flutter))))/bin/cache/artifacts/material_fonts/MaterialIcons-Regular.otf test_screens/screens_test.dart'

# Render the screens with Nix and copy them into docs/images, for the
# README and the guides. Commit the result.
docs-screens:
    nix build .#screens -o result-screens
    mkdir -p docs/images
    install -m644 result-screens/*.png docs/images/
    rm result-screens
