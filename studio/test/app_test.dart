import 'package:duh_studio/engine.dart';
import 'package:duh_studio/main.dart';
import 'package:duh_studio/state.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'fake_engine.dart';

Future<AppState> openApp(WidgetTester tester, FakeEngine engine, {bool open = true}) async {
  tester.view.physicalSize = const Size(1440, 900);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  final app = AppState(engine, persist: false, watch: false);
  await tester.pumpWidget(StudioApp(app: app));
  if (open) {
    await app.openSpace('/tmp/fake-space');
    await tester.pumpAndSettle();
  }
  return app;
}

void main() {
  testWidgets('the appearance button cycles system, dark, light', (tester) async {
    tester.platformDispatcher.platformBrightnessTestValue = Brightness.light;
    addTearDown(tester.platformDispatcher.clearPlatformBrightnessTestValue);
    final app = await openApp(tester, FakeEngine(files: {'body.FCStd': WorkState.changed}));
    Brightness shown() => Theme.of(tester.element(find.text('body.FCStd').first)).brightness;
    expect(shown(), Brightness.light);
    await tester.tap(find.byIcon(Icons.brightness_auto_outlined));
    await tester.pumpAndSettle();
    expect(app.appearance.value, ThemeMode.dark);
    expect(shown(), Brightness.dark);
    await tester.tap(find.byIcon(Icons.dark_mode_outlined));
    await tester.pumpAndSettle();
    expect(shown(), Brightness.light);
    expect(app.appearance.value, ThemeMode.light);
  });

  testWidgets('first run offers to start, get or open a space', (tester) async {
    await openApp(tester, FakeEngine(), open: false);
    expect(find.text('duh Studio'), findsOneWidget);
    expect(find.text('Start a space'), findsOneWidget);
    expect(find.text('Get a copy'), findsOneWidget);
    expect(find.text('Open a space'), findsOneWidget);
  });

  testWidgets('a new space with no saves asks for the first one', (tester) async {
    await openApp(tester, FakeEngine(files: {'body.FCStd': WorkState.new_}));
    expect(find.text('No saves yet'), findsOneWidget);
    expect(find.text('body.FCStd'), findsOneWidget);
    expect(find.text('new · not tracked yet'), findsOneWidget);
    expect(find.text('Pick all and save'), findsOneWidget);
  });

  testWidgets('picking a file and saving goes through the engine', (tester) async {
    final engine = FakeEngine(
      files: {'body.FCStd': WorkState.changed, 'hinge.FCStd': WorkState.changed, 'lid.FCStd': WorkState.unchanged},
      saves: [FakeEngine.aSave('a1', 'First rough shape')],
    );
    await openApp(tester, engine);
    expect(find.text('First rough shape'), findsOneWidget);
    expect(find.text('1 more file unchanged'), findsOneWidget);

    await tester.tap(find.text('body.FCStd'));
    await tester.pumpAndSettle();
    expect(engine.calls, contains('pick body.FCStd'));
    expect(find.text('Save 1 file'), findsOneWidget);

    await tester.enterText(find.byType(TextField).last, 'Thicker walls');
    await tester.tap(find.text('Save 1 file'));
    await tester.pumpAndSettle();
    expect(engine.calls, contains('save Thicker walls'));
    expect(find.text('Thicker walls'), findsWidgets);
  });

  testWidgets('saving with nothing picked picks everything and uses the suggestion', (tester) async {
    final engine = FakeEngine(files: {'a.xcf': WorkState.changed, 'b.xcf': WorkState.missing}, saves: [FakeEngine.aSave('a1', 'one')]);
    await openApp(tester, engine);
    await tester.tap(find.text('Pick all and save'));
    await tester.pumpAndSettle();
    expect(engine.calls, contains('pick a.xcf,b.xcf'));
    expect(engine.calls, contains('save Update 2 files'));
  });

  testWidgets('a save shows its files and can be rewound to', (tester) async {
    final engine = FakeEngine(saves: [FakeEngine.aSave('b2', 'Fillets on every edge', labels: ['final-v2']), FakeEngine.aSave('a1', 'First')]);
    await openApp(tester, engine);
    await tester.tap(find.text('First'));
    await tester.pumpAndSettle();
    expect(find.text('body.FCStd'), findsOneWidget);
    expect(find.text('Start a draft here'), findsOneWidget);

    await tester.tap(find.text('Rewind to here…'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Keep my files as they are'));
    await tester.pumpAndSettle();
    expect(engine.calls, contains('rewind keepFiles'));
  });

  testWidgets('a diverged place explains itself and offers the safe way first', (tester) async {
    final engine = FakeEngine(saves: [FakeEngine.aSave('a1', 'mine')])
      ..placeList = [const PlaceInfo(name: 'origin', url: 's3://bucket/x')]
      ..syncInfo = const SyncInfo(toSend: 1, toReceive: 2, placeHasDraft: true, diverged: true);
    await openApp(tester, engine);
    expect(find.text('↑1 ↓2 · diverged from origin'), findsOneWidget);
    await tester.tap(find.text('Choose…'));
    await tester.pumpAndSettle();
    expect(find.textContaining('duh never merges'), findsOneWidget);
    await tester.tap(find.text('Receive theirs into a new draft'));
    await tester.pumpAndSettle();
    expect(engine.calls, contains('receive into main-from-origin'));
  });

  testWidgets('saves not sent yet are marked, and Send sends', (tester) async {
    final engine = FakeEngine(saves: [FakeEngine.aSave('b2', 'two'), FakeEngine.aSave('a1', 'one')])
      ..placeList = [const PlaceInfo(name: 'origin', url: '/mnt/nas/x')]
      ..syncInfo = const SyncInfo(toSend: 1, toReceive: 0, placeHasDraft: true, diverged: false);
    await openApp(tester, engine);
    expect(find.text('↑ 1 to send'), findsOneWidget);
    expect(find.text('not sent'), findsOneWidget);
    await tester.tap(find.text('Send'));
    await tester.pumpAndSettle();
    expect(engine.calls, contains('send'));
  });

  testWidgets('checking shows that everything is intact', (tester) async {
    await openApp(tester, FakeEngine(saves: [FakeEngine.aSave('a1', 'one')]));
    await tester.tap(find.text('Check'));
    await tester.pumpAndSettle();
    expect(find.text('Everything is intact'), findsOneWidget);
  });

  testWidgets('the command palette finds and runs commands', (tester) async {
    final engine = FakeEngine(files: {'a.bin': WorkState.changed}, saves: [FakeEngine.aSave('a1', 'one')]);
    await openApp(tester, engine);
    await tester.tap(find.text('Search or run…'));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField).last, 'pick all');
    await tester.pumpAndSettle();
    expect(find.text('Pick all changed files'), findsOneWidget);
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pumpAndSettle();
    expect(engine.calls, contains('pick a.bin'));
  });
}
