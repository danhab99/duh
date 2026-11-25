import 'src/rust/api/engine.dart' as rust;
import 'src/rust/api/types.dart';

export 'src/rust/api/types.dart';

/// Everything the app asks of duh. [BridgeEngine] is the real thing (duh's
/// engine through flutter_rust_bridge); tests use a fake.
abstract class Engine {
  String get version;

  Future<SpaceInfo> openSpace(String folder);
  Future<SpaceInfo> startSpace(String folder);
  Future<void> startBareSpace(String folder);
  Future<SpaceInfo> getSpace(String url, String dest, List<String> options);

  Future<LookResult> look(String folder);
  Future<List<Segment>> compareFile(String folder, String path);
  Future<List<Picked>> pick(String folder, List<String> paths);
  Future<void> unpick(String folder, List<String> paths);
  Future<String> suggestMessage(String folder);
  Future<Saved> save(String folder, String message);
  Future<void> bringBack(String folder, String path, {String? from, String? asPath});

  Future<List<SaveInfo>> history(String folder, {int? max});
  Future<SaveDetail> inspect(String folder, String version);
  Future<List<JournalItem>> journal(String folder, String draft);

  Future<List<DraftInfo>> drafts(String folder);
  Future<void> startDraft(String folder, String name, {String? at});
  Future<void> openDraft(String folder, String name);
  Future<void> deleteDraft(String folder, String name);
  Future<void> renameDraft(String folder, String newName);
  Future<List<LabelInfo>> labels(String folder);
  Future<void> addLabel(String folder, String name, String version);
  Future<void> removeLabel(String folder, String name);
  Future<void> rewind(String folder, String version, RewindMode mode);

  Future<List<PlaceInfo>> places(String folder);
  Future<void> addPlace(String folder, String name, String url, List<String> options);
  Future<void> removePlace(String folder, String name);
  Future<SyncInfo> syncStatus(String folder, String place);
  Future<TransferResult> send(String folder, String place, {String? draft, bool force = false});
  Future<TransferResult> receive(String folder, String place, {String? draft, String? into});

  Future<CheckResult> check(String folder, {String? place});
}

/// The error text duh gave, without the bridge's wrapping.
String explain(Object error) {
  var s = error.toString();
  for (final prefix in ['AnyhowException(', 'Exception: ']) {
    if (s.startsWith(prefix)) s = s.substring(prefix.length);
  }
  if (s.endsWith(')') && error.toString().startsWith('AnyhowException(')) s = s.substring(0, s.length - 1);
  // anyhow appends a backtrace after a blank line when RUST_BACKTRACE is set.
  final cut = s.indexOf('\n\nStack backtrace');
  return (cut >= 0 ? s.substring(0, cut) : s).trim();
}

class BridgeEngine implements Engine {
  @override
  String get version => rust.engineVersion();

  @override
  Future<SpaceInfo> openSpace(String folder) => rust.openSpace(folder: folder);
  @override
  Future<SpaceInfo> startSpace(String folder) => rust.startSpace(folder: folder);
  @override
  Future<void> startBareSpace(String folder) => rust.startBareSpace(folder: folder);
  @override
  Future<SpaceInfo> getSpace(String url, String dest, List<String> options) =>
      rust.getSpace(url: url, dest: dest, options: options);

  @override
  Future<LookResult> look(String folder) => rust.look(folder: folder);
  @override
  Future<List<Segment>> compareFile(String folder, String path) => rust.compareFile(folder: folder, path: path);
  @override
  Future<List<Picked>> pick(String folder, List<String> paths) => rust.pick(folder: folder, paths: paths);
  @override
  Future<void> unpick(String folder, List<String> paths) => rust.unpick(folder: folder, paths: paths);
  @override
  Future<String> suggestMessage(String folder) => rust.suggestMessage(folder: folder);
  @override
  Future<Saved> save(String folder, String message) => rust.save(folder: folder, message: message);
  @override
  Future<void> bringBack(String folder, String path, {String? from, String? asPath}) =>
      rust.bringBack(folder: folder, path: path, from: from, asPath: asPath);

  @override
  Future<List<SaveInfo>> history(String folder, {int? max}) => rust.history(folder: folder, max: max);
  @override
  Future<SaveDetail> inspect(String folder, String version) => rust.inspect(folder: folder, version: version);
  @override
  Future<List<JournalItem>> journal(String folder, String draft) => rust.journal(folder: folder, draft: draft);

  @override
  Future<List<DraftInfo>> drafts(String folder) => rust.drafts(folder: folder);
  @override
  Future<void> startDraft(String folder, String name, {String? at}) => rust.startDraft(folder: folder, name: name, at: at);
  @override
  Future<void> openDraft(String folder, String name) => rust.openDraft(folder: folder, name: name);
  @override
  Future<void> deleteDraft(String folder, String name) => rust.deleteDraft(folder: folder, name: name);
  @override
  Future<void> renameDraft(String folder, String newName) => rust.renameDraft(folder: folder, newName: newName);
  @override
  Future<List<LabelInfo>> labels(String folder) => rust.labels(folder: folder);
  @override
  Future<void> addLabel(String folder, String name, String version) =>
      rust.addLabel(folder: folder, name: name, version: version);
  @override
  Future<void> removeLabel(String folder, String name) => rust.removeLabel(folder: folder, name: name);
  @override
  Future<void> rewind(String folder, String version, RewindMode mode) =>
      rust.rewind(folder: folder, version: version, mode: mode);

  @override
  Future<List<PlaceInfo>> places(String folder) => rust.places(folder: folder);
  @override
  Future<void> addPlace(String folder, String name, String url, List<String> options) =>
      rust.addPlace(folder: folder, name: name, url: url, options: options);
  @override
  Future<void> removePlace(String folder, String name) => rust.removePlace(folder: folder, name: name);
  @override
  Future<SyncInfo> syncStatus(String folder, String place) => rust.syncStatus(folder: folder, place: place);
  @override
  Future<TransferResult> send(String folder, String place, {String? draft, bool force = false}) =>
      rust.send(folder: folder, place: place, draft: draft, force: force);
  @override
  Future<TransferResult> receive(String folder, String place, {String? draft, String? into}) =>
      rust.receive(folder: folder, place: place, draft: draft, into: into);

  @override
  Future<CheckResult> check(String folder, {String? place}) => rust.check(folder: folder, place: place);
}
