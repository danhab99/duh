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
          
          # build from the workspace root and select the `cli/` package
          src = ./.;
          setSourceRoot = "sourceRoot=$(echo */cli)";
          cargoLock.lockFile = ./cli/Cargo.lock;
          cargoSha256 = cargoVendorHash;

          nativeBuildInputs = with pkgs; [
            pkg-config
          ];
          
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
              pkg-config
              rustToolchain
              pkgs.rust-analyzer
              rustup
              gdb
            ];

            RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
          };

          duh = pkgs.mkShell {
            buildInputs = [ duh ];
          };
        };

        version = cliToml.package.version;
      }
    );
}
