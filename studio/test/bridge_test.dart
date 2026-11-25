// The real engine, through the real bridge: proves every call and type
// crosses from Dart to Rust and back. Needs the bridge library built
// (`cd native && cargo build`); skipped otherwise.

import 'dart:io';

import 'package:duh_studio/engine.dart';
import 'package:duh_studio/src/rust/frb_generated.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';
import 'package:flutter_test/flutter_test.dart';

const lib = 'native/target/debug/libduh_studio_native.so';

void main() {
  final built = File(lib).existsSync();

  setUpAll(() async {
    if (built) await RustLib.init(externalLibrary: ExternalLibrary.open(lib));
  });

  test('a whole loop through the real engine', () async {
    final e = BridgeEngine();
    final dir = Directory.systemTemp.createTempSync('duh-bridge-');
    final hub = Directory.systemTemp.createTempSync('duh-hub-');
    addTearDown(() {
      dir.deleteSync(recursive: true);
      hub.deleteSync(recursive: true);
    });
    final f = dir.path;

    final info = await e.startSpace(f);
    expect(info.draft, 'main');
    File('$f/body.FCStd').writeAsBytesSync(List.generate(60000, (i) => (i * 7919) % 251));

    var look = await e.look(f);
    expect(look.files.single.state, WorkState.new_);
    expect((await e.compareFile(f, 'body.FCStd')).single.kind, SegmentKind.added);

    final picked = await e.pick(f, ['body.FCStd']);
    expect(picked.single.changed, isTrue);
    expect((await e.look(f)).files.single.picked, PickedKind.new_);
    expect(await e.suggestMessage(f), startsWith('Update 1 file'));
    final first = await e.save(f, 'First rough shape');

    File('$f/body.FCStd').writeAsBytesSync(List.generate(60000, (i) => (i * 31) % 253));
    look = await e.look(f);
    expect(look.files.single.state, WorkState.changed);
    await e.pick(f, ['body.FCStd']);
    await e.save(f, 'Thicker walls');

    final history = await e.history(f);
    expect(history.map((s) => s.message), ['Thicker walls', 'First rough shape']);
    expect(history.last.filesChanged, 1);
    expect(history.last.added, 60000);
    final detail = await e.inspect(f, first.version);
    expect(detail.files.single.size, 60000);

    await e.addLabel(f, 'final-v2', first.version);
    expect((await e.labels(f)).single.name, 'final-v2');
    await e.startDraft(f, 'alt', at: first.version);
    await e.openDraft(f, 'alt');
    expect((await e.drafts(f)).firstWhere((d) => d.open).name, 'alt');
    await e.openDraft(f, 'main');

    await e.bringBack(f, 'body.FCStd', from: 'final-v2', asPath: '$f/../copy-${first.version.substring(0, 6)}.bin');

    await e.startBareSpace('${hub.path}/hub');
    await e.addPlace(f, 'origin', '${hub.path}/hub', []);
    expect((await e.syncStatus(f, 'origin')).toSend, 2);
    expect((await e.send(f, 'origin')).saves, 2);
    expect((await e.syncStatus(f, 'origin')).toSend, 0);

    expect((await e.check(f)).problems, isEmpty);
    expect((await e.check(f, place: 'origin')).problems, isEmpty);
    expect(await e.journal(f, 'main'), isNotEmpty);

    // Errors arrive as duh's own sentences.
    try {
      await e.deleteDraft(f, 'main');
      fail('deleting the open draft should fail');
    } catch (err) {
      expect(explain(err), contains('the draft you have open'));
    }

    await e.rewind(f, first.version, RewindMode.discard);
    expect((await e.history(f)).length, 1);
    File('${dir.parent.path}/copy-${first.version.substring(0, 6)}.bin').deleteSync();
  }, skip: built ? false : 'build the bridge first: cd native && cargo build');
}
