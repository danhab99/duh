{
  description = "A devShell example";

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
        # rustToolchain = pkgs.rust-bin.beta.latest.default.override {
        rustToolchain = pkgs.rust-bin.nightly.latest.default.override {
          extensions = [ "rust-src" "rustfmt" "clippy" ];
        };

        cliToml = builtins.fromTOML (builtins.readFile ./cli/Cargo.toml);

        # pinned vendor hash (computed locally with `cargo vendor` + `nix hash`)
        cargoVendorHash = "sha256-3tBgsSofdwz6fU31+7IohiGPJZr8IcL0DBibucwXn5Q=";

        duh = pkgs.rustPlatform.buildRustPackage {
          pname = "duh";
          version = cliToml.package.version;
          buildType = "release";

          src = builtins.path {
            path = ./.;
            name = "duh-source";
            filter = path: type:
              let base = builtins.baseNameOf path; in
              base != "target" && base != ".git" && base != "result";
          };

          setSourceRoot = "sourceRoot=$(echo */cli)";
          cargoLock.lockFile = ./cli/Cargo.lock;
          cargoSha256 = cargoVendorHash;

          nativeBuildInputs = with pkgs; [
            pkg-config
          ];

          RUSTFLAGS = [
            "-C target-cpu=native"
            "-C opt-level=3"
            "-C codegen-units=1"
            "-C lto=fat"
            "-C strip=symbols"
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
            maintainers = [{
              name = "Dan Habot";
              email = "dan.habot@gmail.com";
            }];
          };
        };
        # (the `cli` crate lives in the `cli/` subdirectory).
      in
      {
        packages = {
          inherit duh;
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
              rustup
              time
            ];

            RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
          };

          duh = pkgs.mkShell {
            buildInputs = [ duh pkgs.time ];
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
