{
  description = "duh — deduplicative update helper";

  inputs = {
    nixpkgs.url      = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url  = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        # One toolchain (pinned by flake.lock) for both the package and the dev shell.
        rustToolchain = pkgs.rust-bin.nightly.latest.default.override {
          extensions = [ "rust-src" "rustfmt" "clippy" ];
        };
        rustPlatform = pkgs.makeRustPlatform {
          cargo = rustToolchain;
          rustc = rustToolchain;
        };

        cliToml = builtins.fromTOML (builtins.readFile ./cli/Cargo.toml);

        src = builtins.path {
          path = ./.;
          name = "duh-source";
          filter = path: type:
            let base = builtins.baseNameOf path; in
            base != "target" && base != ".git" && base != "result";
        };

        duh = rustPlatform.buildRustPackage {
          pname = "duh";
          version = cliToml.package.version;
          buildType = "release";

          inherit src;
          # The `cli` crate lives in `cli/` and depends on `../lib`.
          setSourceRoot = "sourceRoot=$(echo */cli)";

          # Every crate is fetched by Nix from the checksums in Cargo.lock;
          # no vendoring step or hand-maintained hash.
          cargoLock.lockFile = ./cli/Cargo.lock;

          # Run the lib's tests as well as the CLI's.
          cargoTestFlags = [ "--package" "duh" "--package" "lib" ];

          nativeBuildInputs = with pkgs; [
            pkg-config
          ];

          env = {
            CARGO_PROFILE_RELEASE_OPT_LEVEL = "3";
            CARGO_PROFILE_RELEASE_LTO = "fat";
            CARGO_PROFILE_RELEASE_CODEGEN_UNITS = "1";
            CARGO_PROFILE_RELEASE_PANIC = "abort";
            CARGO_PROFILE_RELEASE_STRIP = "symbols";
          };

          meta = with pkgs.lib; {
            description = "deduplicative update helper";
            homepage = "https://github.com/danhab99/duh";
            license = licenses.mit;
            mainProgram = "duh";
            maintainers = [{
              name = "Dan Habot";
              email = "dan.habot@gmail.com";
            }];
          };
        };

        # demo.sh sets the standard: it must pass against the Nix-built binary.
        demo = pkgs.runCommand "duh-demo" {
          nativeBuildInputs = with pkgs; [
            bash
            coreutils
            gawk
            gnugrep
            gnused
            python3
            rclone  # serves S3 for the demo's remote section
            time
          ];
        } ''
          mkdir work
          DUH=${duh}/bin/duh bash ${src}/demo.sh "$PWD/work" > $out || { cat $out; exit 1; }
          cat $out
        '';
      in
      {
        packages = {
          inherit duh;
          default = duh;
        };

        checks = {
          inherit duh demo;
        };

        devShells = {
          default = with pkgs; mkShell {
            buildInputs = [
              cargo-machete
              gdb
              just
              pkg-config
              pkgs.rust-analyzer
              rustToolchain
              time
            ];

            RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
          };

          # What demo.sh needs, with the Nix-built duh.
          duh = pkgs.mkShell {
            buildInputs = with pkgs; [ duh python3 rclone time ];
          };
        };

        apps = {
          duh = flake-utils.lib.mkApp {
            drv = duh;
            exePath = "/bin/duh";
          };

          default = self.apps.${system}.duh;
        };

        version = cliToml.package.version;
      }
    );
}
