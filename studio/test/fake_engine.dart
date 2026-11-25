import 'package:duh_studio/engine.dart';

/// An engine that keeps one pretend space in memory, for widget tests.
class FakeEngine implements Engine {
  FakeEngine({this.files = const {}, this.saves = const []});

  /// Path -> state of each file in the folder.
  Map<String, WorkState> files;
  final Map<String, PickedKind> picked = {};
  List<SaveInfo> saves;
  List<PlaceInfo> placeList = [];
  SyncInfo syncInfo = const SyncInfo(toSend: 0, toReceive: 0, placeHasDraft: true, diverged: false);
  final List<String> calls = [];
  String draft = 'main';

  static SaveInfo aSave(String version, String message, {List<String> labels = const [], int time = 1795600000}) => SaveInfo(
        version: version.padRight(64, '0'),
        previous: '',
        message: message,
        author: 'Dan',
        time: time,
        labels: labels,
        drafts: const [],
        added: 1200,
        removed: 40,
        filesChanged: 1,
      );

  @override
  String get version => 'fake';

  SpaceInfo get _info => SpaceInfo(root: '/tmp/fake-space', name: 'fake-space', draft: draft);

  @override
  Future<SpaceInfo> openSpace(String folder) async => _info;
  @override
  Future<SpaceInfo> startSpace(String folder) async => _info;
  @override
  Future<void> startBareSpace(String folder) async {}
  @override
  Future<SpaceInfo> getSpace(String url, String dest, List<String> options) async => _info;

  @override
  Future<LookResult> look(String folder) async => LookResult(
        draft: draft,
        head: saves.isEmpty ? '' : saves.first.version,
        files: [
          for (final e in files.entries)
            WorkFile(
              path: e.key,
              state: picked.containsKey(e.key) && e.value != WorkState.unchanged ? WorkState.picked : e.value,
              picked: picked[e.key] ?? PickedKind.none,
              size: 1000,
            ),
        ],
      );

  @override
  Future<List<Segment>> compareFile(String folder, String path) async =>
      const [Segment(kind: SegmentKind.kept, len: 800), Segment(kind: SegmentKind.added, len: 200)];

  @override
  Future<List<Picked>> pick(String folder, List<String> paths) async {
    calls.add('pick ${paths.join(',')}');
    for (final p in paths) {
      picked[p] = files[p] == WorkState.new_ ? PickedKind.new_ : PickedKind.modified;
    }
    return [for (final p in paths) Picked(path: p, changed: true, deleted: false, strip: const [])];
  }

  @override
  Future<void> unpick(String folder, List<String> paths) async {
    calls.add('unpick ${paths.join(',')}');
    for (final p in paths) {
      picked.remove(p);
    }
  }

  @override
  Future<String> suggestMessage(String folder) async => 'Update ${picked.length} file${picked.length == 1 ? '' : 's'}';

  @override
  Future<Saved> save(String folder, String message) async {
    calls.add('save $message');
    final v = 'f${saves.length}'.padRight(64, '0');
    saves = [FakeEngine.aSave(v, message), ...saves];
    for (final p in picked.keys) {
      files[p] = WorkState.unchanged;
    }
    picked.clear();
    return Saved(version: v, draft: draft);
  }

  @override
  Future<void> bringBack(String folder, String path, {String? from, String? asPath}) async => calls.add('bring back $path');

  @override
  Future<List<SaveInfo>> history(String folder, {int? max}) async => saves;
  @override
  Future<SaveDetail> inspect(String folder, String version) async => SaveDetail(
        info: saves.firstWhere((s) => s.version == version),
        files: const [
          FileInSave(path: 'body.FCStd', size: 24000000, changed: true, strip: [Segment(kind: SegmentKind.kept, len: 900), Segment(kind: SegmentKind.added, len: 100)]),
        ],
      );
  @override
  Future<List<JournalItem>> journal(String folder, String draft) async => [];

  @override
  Future<List<DraftInfo>> drafts(String folder) async => [DraftInfo(name: draft, version: saves.isEmpty ? '' : saves.first.version, open: true)];
  @override
  Future<void> startDraft(String folder, String name, {String? at}) async => calls.add('start draft $name');
  @override
  Future<void> openDraft(String folder, String name) async => draft = name;
  @override
  Future<void> deleteDraft(String folder, String name) async {}
  @override
  Future<void> renameDraft(String folder, String newName) async => draft = newName;
  @override
  Future<List<LabelInfo>> labels(String folder) async => [
        for (final s in saves)
          for (final l in s.labels) LabelInfo(name: l, version: s.version),
      ];
  @override
  Future<void> addLabel(String folder, String name, String version) async => calls.add('label $name');
  @override
  Future<void> removeLabel(String folder, String name) async {}
  @override
  Future<void> rewind(String folder, String version, RewindMode mode) async => calls.add('rewind ${mode.name}');

  @override
  Future<List<PlaceInfo>> places(String folder) async => placeList;
  @override
  Future<void> addPlace(String folder, String name, String url, List<String> options) async => placeList = [...placeList, PlaceInfo(name: name, url: url)];
  @override
  Future<void> removePlace(String folder, String name) async {}
  @override
  Future<SyncInfo> syncStatus(String folder, String place) async => syncInfo;
  @override
  Future<TransferResult> send(String folder, String place, {String? draft, bool force = false}) async {
    calls.add('send${force ? ' --force' : ''}${draft != null ? ' as $draft' : ''}');
    return TransferResult(draft: draft ?? this.draft, saves: syncInfo.toSend, bytes: 1000);
  }

  @override
  Future<TransferResult> receive(String folder, String place, {String? draft, String? into}) async {
    calls.add('receive${into != null ? ' into $into' : ''}');
    return TransferResult(draft: into ?? this.draft, saves: syncInfo.toReceive, bytes: 1000);
  }

  @override
  Future<CheckResult> check(String folder, {String? place}) async =>
      const CheckResult(objects: 10, saves: 2, fileVersions: 3, solids: 0, problems: []);
}
