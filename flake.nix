{
  description = "duh — deduplicative update helper";

  inputs = {
    nixpkgs.url      = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url  = "github:numtide/flake-utils";
    # Packs duh Studio and everything it needs into one AppImage file.
    nix-appimage = {
      # Pinned before 3f60392, whose AppRun mounts the bundle one level too
      # deep (/nix/store/store/…) on machines without a /nix/store, i.e.
      # every distro but NixOS. Unpin once that's fixed upstream.
      url = "github:ralismark/nix-appimage/6cdc3f8faa6e37ba2c5d745803b16125688b6b88";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.flake-utils.follows = "flake-utils";
    };
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, nix-appimage, ... }:
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
        # Packages build with the same nightly but without rust-src: when
        # it's present, rustc writes paths into it into every binary's panic
        # messages, and the whole toolchain (1.6 GB) lands in the closure.
        buildToolchain = pkgs.rust-bin.nightly.latest.minimal;
        rustPlatform = pkgs.makeRustPlatform {
          cargo = buildToolchain;
          rustc = buildToolchain;
        };

        cliToml = builtins.fromTOML (builtins.readFile ./cli/Cargo.toml);

        src = builtins.path {
          path = ./.;
          name = "duh-source";
          filter = path: type:
            let base = builtins.baseNameOf path; in
            base != "target" && base != ".git" && base != "result";
        };

        # The CLI, built by any Rust platform: the pinned toolchain for the
        # package and its tests, nixpkgs' own for static and cross builds.
        mkDuh = rustPlatform: extra: rustPlatform.buildRustPackage ({
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
        } // extra);

        # The native build also installs the manual pages, written by duh
        # itself from its command definitions: `man duh`, `man duh-save`, …
        duh = mkDuh rustPlatform {
          nativeBuildInputs = [ pkgs.pkg-config pkgs.installShellFiles ];
          postInstall = ''
            "$out/bin/duh" __man man
            installManPage man/*.1
          '';
        };

        # Release builds of the CLI that run anywhere, not only under Nix:
        # fully static for Linux (musl), and cross-compiled for Windows.
        # Their tests ran in `duh` already; these only change the target.
        duhStatic = mkDuh pkgs.pkgsStatic.rustPlatform { doCheck = false; };
        duhWindows = mkDuh pkgs.pkgsCross.mingwW64.rustPlatform { doCheck = false; };

        # duh Studio's bridge to lib: a shared library Dart loads at start.
        # Built like the CLI, every crate fetched by Nix from Cargo.lock.
        studioNative = rustPlatform.buildRustPackage {
          pname = "duh-studio-native";
          version = "0.1.0";
          inherit src;
          setSourceRoot = "sourceRoot=$(echo */studio/native)";
          cargoLock.lockFile = ./studio/native/Cargo.lock;
          nativeBuildInputs = with pkgs; [ pkg-config ];
          # The bridge catches Rust panics and reports them to Dart, so
          # unlike the CLI it must not be built with panic = abort.
          env.CARGO_PROFILE_RELEASE_LTO = "thin";
          env.CARGO_PROFILE_RELEASE_STRIP = "symbols";
        };

        studioSrc = builtins.path {
          path = ./studio;
          name = "duh-studio-source";
          filter = path: type:
            let base = builtins.baseNameOf path; in
            !(builtins.elem base [ "target" "build" ".dart_tool" "native" ])
            # Rendered screens, which `screens` below makes.
            && !(base == "out" && builtins.baseNameOf (builtins.dirOf path) == "test_screens");
        };

        # duh Studio's screens, rendered from a believable fake project in
        # light and dark: the README's and the tutorial's pictures.
        # `just docs-screens` copies them into docs/images.
        screens = pkgs.flutter.buildFlutterApplication {
          pname = "duh-studio-screens";
          version = "0.1.0";
          src = studioSrc;
          pubspecLock = pkgs.lib.importJSON ./studio/pubspec.lock.json;
          buildPhase = ''
            runHook preBuild
            flutter test --update-goldens \
              --dart-define=ICONS=${pkgs.flutter.sdk}/bin/cache/artifacts/material_fonts/MaterialIcons-Regular.otf \
              test_screens/screens_test.dart
            runHook postBuild
          '';
          installPhase = ''
            runHook preInstall
            mkdir -p $out $debug
            cp test_screens/out/*.png $out/
            runHook postInstall
          '';
        };

        # duh Studio, the desktop app: `nix run .#studio`.
        studio = pkgs.flutter.buildFlutterApplication {
          pname = "duh-studio";
          version = "0.1.0";
          src = studioSrc;
          pubspecLock = pkgs.lib.importJSON ./studio/pubspec.lock.json;
          # A launcher entry, so the app shows up in rofi, application menus
          # and anything else that reads share/applications.
          nativeBuildInputs = [ pkgs.copyDesktopItems ];
          desktopItems = [
            (pkgs.makeDesktopItem {
              name = "duh-studio";
              desktopName = "duh Studio";
              genericName = "Version history for your files";
              comment = "Keep every version of every file";
              exec = "duh-studio %f";
              icon = "duh-studio";
              categories = [ "Utility" ];
              keywords = [ "version" "history" "backup" "duh" ];
            })
          ];
          postInstall = ''
            cp ${studioNative}/lib/libduh_studio_native.so "$out/app/$pname/lib/"
            # `duh-studio` for dmenu and the shell; `duh_studio` is Flutter's name.
            ln -s duh_studio "$out/bin/duh-studio"
            install -Dm644 ${./studio/assets/duh-studio.svg} \
              "$out/share/icons/hicolor/scalable/apps/duh-studio.svg"
          '';
          meta.mainProgram = "duh-studio";
        };

        # The built app starts headless and reaches duh's engine.
        studioSmoke = pkgs.runCommand "duh-studio-smoke" {
          nativeBuildInputs = [ pkgs.xvfb-run pkgs.coreutils ];
        } ''
          export HOME=$PWD
          # No GPU in the sandbox: Mesa's software renderer stands in.
          export __EGL_VENDOR_LIBRARY_DIRS=${pkgs.mesa}/share/glvnd/egl_vendor.d
          export LIBGL_ALWAYS_SOFTWARE=1
          export LD_LIBRARY_PATH=${pkgs.libglvnd}/lib
          timeout 180 xvfb-run -a ${studio}/bin/duh_studio --smoke | tee $out
          grep -q "engine" $out
        '';

        # The Windows .exe starts under Wine and reaches duh's engine.
        studioWindowsSmoke = pkgs.runCommand "duh-studio-windows-smoke" {
          nativeBuildInputs = [ pkgs.wine64 pkgs.xvfb-run pkgs.coreutils ];
        } ''
          export HOME=$PWD WINEPREFIX=$PWD/wine WINEDLLOVERRIDES="mscoree,mshtml="
          timeout 300 xvfb-run -a wine ${studioWindows.exe} --smoke > log 2>&1 || true
          grep "duh Studio" log | tee $out
          grep -q "engine" $out || { cat log; exit 1; }
        '';

        # duh Studio as one file for any Linux. Nix builds its OpenGL against
        # Nix's own Mesa, which other distros don't have at the paths Nix
        # expects, so the launcher points the app at the Mesa bundled
        # inside the AppImage (Intel, AMD, and software rendering anywhere).
        studioPortable = pkgs.writeShellScriptBin "duh-studio" ''
          export __EGL_VENDOR_LIBRARY_DIRS=${pkgs.mesa}/share/glvnd/egl_vendor.d
          export LIBGL_DRIVERS_PATH=${pkgs.mesa}/lib/dri
          export LD_LIBRARY_PATH=${pkgs.libglvnd}/lib''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}
          exec ${studio}/bin/duh_studio "$@"
        '';
        studioAppImage = nix-appimage.lib.${system}.mkAppImage {
          program = "${studioPortable}/bin/duh-studio";
          name = "duh-studio.AppImage";
        };

        # duh Studio for Windows, as one .exe, built here (see the file).
        studioWindows = import ./nix/studio-windows.nix {
          inherit pkgs studioSrc;
          rustSrc = src;
        };

        # Everything a release ships, in one folder. CI builds this and
        # uploads its files as they are; it does no other work.
        ci = pkgs.runCommand "duh-release" { } ''
          mkdir $out
          cp ${studioAppImage} $out/duh-studio-x86_64-linux.AppImage
          cp ${studioWindows.exe} $out/duh-studio-x86_64-windows.exe
          cp ${duhWindows}/bin/duh.exe $out/duh-x86_64-windows.exe
        '';

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
          inherit duh studio screens;
          default = duh;
        } // pkgs.lib.optionalAttrs pkgs.stdenv.isLinux {
          inherit duhStatic duhWindows studioAppImage;
          # `nix build .#studioWindows` is the .exe; its pieces build alone
          # as .#studioWindows.kernel, .aot, .runner, .native, .bundle.
          studioWindows = studioWindows.launcher // studioWindows;
        } // pkgs.lib.optionalAttrs (system == "x86_64-linux") {
          inherit ci;
        };

        checks = {
          inherit duh demo studioSmoke;
        } // pkgs.lib.optionalAttrs (system == "x86_64-linux") {
          inherit studioWindowsSmoke;
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

          # Working on duh Studio: Flutter, the bridge codegen (pinned to
          # match the flutter_rust_bridge crate and Dart package) and Rust.
          studio = pkgs.mkShell {
            buildInputs = with pkgs; [
              flutter
              flutter_rust_bridge_codegen
              rustToolchain
              pkg-config
              just
            ];
          };
        };

        apps = {
          duh = flake-utils.lib.mkApp {
            drv = duh;
            exePath = "/bin/duh";
          };

          studio = flake-utils.lib.mkApp {
            drv = studio;
            exePath = "/bin/duh-studio";
          };

          default = self.apps.${system}.duh;
        };

        version = cliToml.package.version;
      }
    );
}
