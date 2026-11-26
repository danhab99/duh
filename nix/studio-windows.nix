# duh Studio for Windows, built on Linux.
#
# Flutter's own tool only builds Windows apps on Windows, but a Flutter app
# is just four pieces, and each can be made here:
#
#   1. The Dart code compiled to a kernel file, by Flutter's compiler run on
#      Linux and told the target is Windows.
#   2. That kernel turned into machine code (app.so) by Flutter's Windows
#      gen_snapshot.exe, run under Wine. The engine ships it for exactly this
#      job; the output is the same file a Windows machine would make.
#   3. The small C++ program that opens the window, compiled with MinGW.
#      It talks to Flutter's prebuilt flutter_windows.dll through its C API.
#   4. duh's engine, cross-compiled from Rust as a DLL.
#
# The launcher then packs the whole folder into one .exe.
{ pkgs, studioSrc, rustSrc }:
let
  inherit (pkgs) lib;
  flutter = pkgs.flutter;
  sdk = flutter.sdk;
  win = pkgs.pkgsCross.mingwW64;
  # The bridge needs a newer Rust than nixpkgs' own, so cross builds use the
  # pinned nightly (as the Linux build does) with Windows' standard library.
  winRust = win.makeRustPlatform {
    cargo = toolchain;
    rustc = toolchain;
  };
  toolchain = pkgs.rust-bin.nightly.latest.minimal.override {
    targets = [ "x86_64-pc-windows-gnu" ];
  };

  # Flutter's Windows engine, matching the Flutter that builds the Dart code.
  engineVersion = flutter.engineVersion;
  # Each zip is unpacked without the parts the build doesn't use (the
  # engine's 250 MB of debug symbols, the test runner).
  engineZip = name: path: hash: only: pkgs.runCommand "flutter-${name}-${engineVersion}" {
    src = pkgs.fetchurl {
      url = "https://storage.googleapis.com/flutter_infra_release/flutter/${engineVersion}/${path}";
      inherit hash;
    };
    nativeBuildInputs = [ pkgs.unzip ];
  } "mkdir $out && unzip -q $src ${only} -d $out";
  engine = engineZip "windows-engine" "windows-x64-release/windows-x64-flutter.zip"
    "sha256-Njhk4xBtNZsXAucwS578xFZcYGjMU3y+WOUR5rEpM7Q=" "-x '*.pdb'";
  wrapper = engineZip "windows-wrapper" "windows-x64/flutter-cpp-client-wrapper.zip"
    "sha256-ITAZLAR787/vCu/Rw4WQlmFKr4rN9pOdXRDLFqScR50=" "";
  artifacts = engineZip "windows-artifacts" "windows-x64/artifacts.zip"
    "sha256-EZRSwmFNhw5uQZdmFh7d8mn5vDPmZPa08ZH5vLYpn84=" "icudtl.dat";

  # 1. Dart → kernel, and the app's assets (fonts, shaders, manifest).
  kernel = flutter.buildFlutterApplication {
    pname = "duh-studio-windows-kernel";
    version = "0.1.0";
    src = studioSrc;
    pubspecLock = lib.importJSON ../studio/pubspec.lock.json;
    buildPhase = ''
      runHook preBuild
      flutter build bundle --release --target-platform windows-x64 \
        --asset-dir build/flutter_assets
      ${sdk}/bin/cache/dart-sdk/bin/dartaotruntime \
        ${sdk}/bin/cache/dart-sdk/bin/snapshots/frontend_server_aot.dart.snapshot \
        --sdk-root ${sdk}/bin/cache/artifacts/engine/common/flutter_patched_sdk_product/ \
        --target=flutter \
        --no-print-incremental-dependencies \
        -Ddart.vm.profile=false -Ddart.vm.product=true \
        --delete-tostring-package-uri=dart:ui \
        --delete-tostring-package-uri=package:flutter \
        --aot --tfa --target-os windows \
        --packages .dart_tool/package_config.json \
        --output-dill build/app.dill \
        --verbosity=error \
        package:duh_studio/main.dart
      runHook postBuild
    '';
    installPhase = ''
      runHook preInstall
      mkdir -p $out $debug
      cp build/app.dill $out/
      cp -r build/flutter_assets $out/
      runHook postInstall
    '';
  };

  # 2. kernel → Windows machine code, with Flutter's Windows compiler.
  aot = pkgs.runCommand "duh-studio-windows-aot" {
    nativeBuildInputs = [ pkgs.wine64 ];
  } ''
    export HOME=$TMPDIR WINEPREFIX=$TMPDIR/wine WINEDLLOVERRIDES="mscoree,mshtml="
    cp ${kernel}/app.dill app.dill
    wine ${engine}/gen_snapshot.exe \
      --deterministic --snapshot_kind=app-aot-elf --elf=app.so --strip app.dill
    mkdir $out
    cp app.so $out/
  '';

  # 3. The window: Flutter's runner template, compiled with MinGW.
  icon = pkgs.runCommand "duh-studio-ico" {
    nativeBuildInputs = [ pkgs.librsvg pkgs.imagemagick ];
  } ''
    for s in 16 32 48 256; do
      rsvg-convert -w $s -h $s ${../studio/assets/duh-studio.svg} -o $s.png
    done
    magick 16.png 32.png 48.png 256.png ico:$out
  '';
  runner = win.stdenv.mkDerivation {
    pname = "duh-studio-windows-runner";
    version = "0.1.0";
    src = ../studio/windows;
    buildPhase = ''
      mkdir -p runner/resources
      cp ${icon} runner/resources/app_icon.ico
      cp ${engine}/flutter_windows.dll .
      W=${wrapper}/cpp_client_wrapper
      $CXX -std=c++17 -O2 -municode -mwindows -DNOMINMAX -DUNICODE -D_UNICODE \
        -I. -I${engine} -I$W/include -I$W -Iflutter \
        runner/*.cpp flutter/generated_plugin_registrant.cc \
        $W/core_implementations.cc $W/standard_codec.cc \
        $W/flutter_engine.cc $W/flutter_view_controller.cc \
        -c
      (cd runner && $WINDRES -I. -DFLUTTER_VERSION='\"0.1.0\"' \
        -DFLUTTER_VERSION_MAJOR=0 -DFLUTTER_VERSION_MINOR=1 \
        -DFLUTTER_VERSION_PATCH=0 -DFLUTTER_VERSION_BUILD=1 \
        Runner.rc -O coff -o ../runner.res.o)
      $CXX -municode -mwindows -static -static-libgcc -static-libstdc++ \
        *.o -o duh_studio.exe \
        ${engine}/flutter_windows.dll.lib -ldwmapi -lole32
    '';
    WINDRES = "${win.stdenv.cc.targetPrefix}windres";
    installPhase = ''
      mkdir -p $out
      cp duh_studio.exe $out/
    '';
  };

  # 4. duh's engine as a DLL.
  native = winRust.buildRustPackage {
    pname = "duh-studio-native-windows";
    version = "0.1.0";
    src = rustSrc;
    setSourceRoot = "sourceRoot=$(echo */studio/native)";
    cargoLock.lockFile = ../studio/native/Cargo.lock;
    doCheck = false;
    env.CARGO_PROFILE_RELEASE_LTO = "thin";
    env.CARGO_PROFILE_RELEASE_STRIP = "symbols";
  };

  # The app's folder, laid out as `flutter build windows` lays it out.
  bundle = pkgs.runCommand "duh-studio-windows-bundle" { } ''
    mkdir -p $out/data
    cp ${runner}/duh_studio.exe ${engine}/flutter_windows.dll $out/
    cp ${native}/bin/duh_studio_native.dll $out/
    cp ${aot}/app.so ${artifacts}/icudtl.dat $out/data/
    cp -r ${kernel}/flutter_assets $out/data/
  '';

  # Every file of the folder, in the launcher's payload format.
  payload = pkgs.runCommand "duh-studio-windows-payload" {
    nativeBuildInputs = [ pkgs.python3 ];
  } ''
    python3 - ${bundle} $out <<'EOF'
    import os, struct, sys
    root, out = sys.argv[1], sys.argv[2]
    with open(out, "wb") as f:
        for d, _, files in sorted(os.walk(root)):
            for name in sorted(files):
                path = os.path.join(d, name)
                rel = os.path.relpath(path, root).replace(os.sep, "/").encode()
                data = open(path, "rb").read()
                f.write(struct.pack("<I", len(rel)) + rel)
                f.write(struct.pack("<Q", len(data)) + data)
    EOF
  '';

  launcher = winRust.buildRustPackage {
    pname = "duh-studio-windows";
    version = "0.1.0";
    src = ../studio/windows/launcher;
    cargoLock.lockFile = ../studio/windows/launcher/Cargo.lock;
    doCheck = false;
    DUH_STUDIO_PAYLOAD = payload;
    # The payload's store hash names the folder it unpacks to, so a new
    # version never runs an old one's files.
    DUH_STUDIO_PAYLOAD_ID = builtins.substring 11 12 (builtins.unsafeDiscardStringContext "${payload}");
  };
in
{
  inherit kernel aot runner native bundle launcher;
  exe = "${launcher}/bin/duh-studio.exe";
}
