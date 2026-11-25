import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';

import 'src/rust/api/engine.dart';
import 'src/rust/frb_generated.dart';

Future<void> main(List<String> args) async {
  await RustLib.init(externalLibrary: _engineLibrary());

  // `--smoke`: prove the app starts and reaches duh's engine, then exit.
  if (args.contains('--smoke')) {
    stdout.writeln('duh Studio: engine ${engineVersion()}');
    exit(0);
  }

  runApp(const StudioApp());
}

/// duh's engine: in the app bundle's `lib/` (as built by Nix or `flutter
/// build`), else the bridge crate's own build output while developing.
ExternalLibrary _engineLibrary() {
  const name = 'libduh_studio_native.so';
  final exeDir = File(Platform.resolvedExecutable).parent.path;
  for (final path in [
    '$exeDir/lib/$name',
    'native/target/release/$name',
    'native/target/debug/$name',
  ]) {
    if (File(path).existsSync()) return ExternalLibrary.open(path);
  }
  return ExternalLibrary.open(name);
}

class StudioApp extends StatelessWidget {
  const StudioApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'duh Studio',
      debugShowCheckedModeBanner: false,
      home: Scaffold(
        body: Center(child: Text('duh Studio · engine ${engineVersion()}')),
      ),
    );
  }
}
