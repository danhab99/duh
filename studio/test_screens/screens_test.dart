// Renders duh Studio's main screens to PNGs with the real fonts, for looking
// at the design without a GPU. Not part of `flutter test` (it lives outside
// test/); run it with `just studio-screens`, which writes test_screens/out/.

import 'dart:io';

import 'package:duh_studio/engine.dart';
import 'package:duh_studio/main.dart';
import 'package:duh_studio/state.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import '../test/fake_engine.dart';

const icons = String.fromEnvironment('ICONS');

Future<void> loadFonts() async {
  Future<void> load(String family, List<String> files) async {
    final loader = FontLoader(family);
    for (final f in files) {
      loader.addFont(Future.value(ByteData.sublistView(File(f).readAsBytesSync())));
    }
    await loader.load();
  }

  await load('Inter', [for (final w in [400, 500, 600, 700]) 'fonts/Inter-$w.ttf']);
  await load('JetBrains Mono', ['fonts/JetBrainsMono-400.ttf', 'fonts/JetBrainsMono-500.ttf']);
  if (icons.isNotEmpty) await load('MaterialIcons', [icons]);
}

FakeEngine bracket() {
  final now = DateTime.now().millisecondsSinceEpoch ~/ 1000;
  SaveInfo s(String v, String msg, int ago, int added, int removed, int files, {List<String> labels = const [], List<String> drafts = const []}) => SaveInfo(
        version: v.padRight(64, '7'),
        previous: '',
        message: msg,
        author: 'Dan',
        time: now - ago,
        labels: labels,
        drafts: drafts,
        added: added,
        removed: removed,
        filesChanged: files,
      );
  return _StripyEngine(
    files: {
      'body.FCStd': WorkState.changed,
      'hinge.FCStd': WorkState.changed,
      'label-art.xcf': WorkState.new_,
      'lid.FCStd': WorkState.unchanged,
      'notes.txt': WorkState.unchanged,
    },
    saves: [
      s('b9b1c81a66ee', 'Thicker walls on the body', 2 * 3600, 620000, 0, 1, drafts: ['main']),
      s('32445210e611', 'Hinge pin down to 3 mm', 26 * 3600, 180000, 12000, 2),
      s('616f79c65b49', 'Fillets on every edge', 4 * 86400, 2100000, 400000, 3, labels: ['final-v2']),
      s('57551d184977', 'Split the lid from the body', 9 * 86400, 4800000, 0, 2),
      s('796402472ffb', 'First rough shape', 15 * 86400, 2740000, 0, 3),
    ],
  )
    ..placeList = [const PlaceInfo(name: 'origin', url: 's3://studio-bucket/bracket'), const PlaceInfo(name: 'nas-backup', url: '/mnt/nas/bracket')]
    ..syncInfo = const SyncInfo(toSend: 2, toReceive: 0, placeHasDraft: true, diverged: false);
}

/// Strips that differ per file, like real ones.
class _StripyEngine extends FakeEngine {
  _StripyEngine({super.files, super.saves});

  @override
  Future<LookResult> look(String folder) async {
    final r = await super.look(folder);
    const sizes = {'body.FCStd': 24100000, 'hinge.FCStd': 3400000, 'label-art.xcf': 11200000, 'lid.FCStd': 8900000, 'notes.txt': 2100};
    return LookResult(
      draft: r.draft,
      head: r.head,
      files: [for (final f in r.files) WorkFile(path: f.path, state: f.state, picked: f.path == 'body.FCStd' ? PickedKind.modified : f.picked, size: sizes[f.path] ?? 1000)],
    );
  }

  @override
  Future<List<Segment>> compareFile(String folder, String path) async => switch (path) {
        'body.FCStd' => const [
            Segment(kind: SegmentKind.kept, len: 6800000),
            Segment(kind: SegmentKind.added, len: 2100000),
            Segment(kind: SegmentKind.removed, len: 40000),
            Segment(kind: SegmentKind.kept, len: 15200000),
          ],
        'hinge.FCStd' => const [Segment(kind: SegmentKind.added, len: 200000), Segment(kind: SegmentKind.kept, len: 3200000)],
        _ => const [Segment(kind: SegmentKind.added, len: 11200000)],
      };

  @override
  Future<SaveDetail> inspect(String folder, String version) async => SaveDetail(
        info: saves.firstWhere((s) => s.version == version),
        files: const [
          FileInSave(path: 'body.FCStd', size: 23600000, changed: true, strip: [
            Segment(kind: SegmentKind.kept, len: 4000000),
            Segment(kind: SegmentKind.added, len: 5000000),
            Segment(kind: SegmentKind.removed, len: 2000000),
            Segment(kind: SegmentKind.kept, len: 12600000),
          ]),
          FileInSave(path: 'hinge.FCStd', size: 3400000, changed: true, strip: [
            Segment(kind: SegmentKind.kept, len: 2000000),
            Segment(kind: SegmentKind.added, len: 400000),
            Segment(kind: SegmentKind.kept, len: 1000000),
          ]),
          FileInSave(path: 'lid.FCStd', size: 8900000, changed: false, strip: []),
        ],
      );
}

Future<AppState> show(WidgetTester tester, FakeEngine engine, Brightness b, {bool open = true}) async {
  tester.view.physicalSize = const Size(1440, 900);
  tester.view.devicePixelRatio = 1;
  tester.platformDispatcher.platformBrightnessTestValue = b;
  addTearDown(tester.view.reset);
  addTearDown(tester.platformDispatcher.clearPlatformBrightnessTestValue);
  final app = AppState(engine, persist: false, watch: false)..recent = ['/home/dan/Projects/bracket-v3', '/home/dan/Art/poster-series', '/home/dan/Projects/desk-lamp'];
  await tester.pumpWidget(StudioApp(app: app));
  if (open) {
    await app.openSpace('/home/dan/Projects/bracket-v3');
    await tester.pumpAndSettle();
    await tester.pumpAndSettle();
  }
  return app;
}

Future<void> snap(String name) => expectLater(find.byType(StudioApp), matchesGoldenFile('out/$name.png'));

void main() {
  setUpAll(loadFonts);

  testWidgets('main, dark', (tester) async {
    final app = await show(tester, bracket(), Brightness.dark);
    app.message.text = 'Thicker ribs and a 3 mm hinge pin';
    await tester.pumpAndSettle();
    await snap('main-dark');
  });

  testWidgets('a save, light', (tester) async {
    final app = await show(tester, bracket(), Brightness.light);
    await app.select(SaveSelected(app.history[2].version));
    await tester.pumpAndSettle();
    await snap('save-light');
  });

  testWidgets('diverged, light', (tester) async {
    final engine = bracket()..syncInfo = const SyncInfo(toSend: 1, toReceive: 2, placeHasDraft: true, diverged: true);
    await show(tester, engine, Brightness.light);
    await tester.tap(find.text('Choose…'));
    await tester.pumpAndSettle();
    await snap('diverged-light');
  });

  testWidgets('welcome, dark', (tester) async {
    await show(tester, bracket(), Brightness.dark, open: false);
    await tester.pumpAndSettle();
    await snap('welcome-dark');
  });

  testWidgets('palette, dark', (tester) async {
    await show(tester, bracket(), Brightness.dark);
    await tester.tap(find.text('Search or run…'));
    await tester.pumpAndSettle();
    await snap('palette-dark');
  });
}
