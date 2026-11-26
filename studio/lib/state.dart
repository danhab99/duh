import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart' show ThemeMode;
import 'package:flutter/widgets.dart';

import 'engine.dart';

/// "1 save", "3 saves": a number with its noun in the right form.
String count(int n, String one, [String? many]) => '$n ${n == 1 ? one : many ?? '${one}s'}';

/// What the inspector shows.
sealed class Selection {
  const Selection();
}

/// Your files (the unsaved work at the top of the timeline).
class WorkSelected extends Selection {
  const WorkSelected();
}

/// One save.
class SaveSelected extends Selection {
  const SaveSelected(this.version);
  final String version;
}

/// The app's state: the open space and everything shown about it. Every
/// change goes through the [Engine], so the app and the CLI always agree.
class AppState extends ChangeNotifier {
  /// `persist`: remember recent spaces on disk. `watch`: notice changes to
  /// the folder. Tests turn both off (they run without real file access).
  AppState(this.engine, {String? configDir, this.persist = true, this.watch = true})
      : _configDir = configDir ?? _defaultConfigDir();

  final Engine engine;
  final String _configDir;
  final bool persist;
  final bool watch;

  /// Light, dark, or whatever the system says. Kept apart from the rest so
  /// changing it rebuilds the theme without rebuilding on every refresh.
  final appearance = ValueNotifier<ThemeMode>(ThemeMode.system);

  // Spaces
  List<String> recent = [];
  SpaceInfo? space;
  String? get folder => space?.root;

  // What's shown
  LookResult? look;
  List<SaveInfo> history = [];
  List<DraftInfo> drafts = [];
  List<LabelInfo> labels = [];
  List<PlaceInfo> places = [];
  Selection selection = const WorkSelected();
  SaveDetail? detail;
  final Map<String, List<Segment>> strips = {};

  // Sharing and health
  SyncInfo? sync;
  String? syncError;
  CheckResult? health;
  DateTime? checkedAt;

  // Activity
  String? busy;
  final Set<String> picking = {};
  final _messages = StreamController<String>.broadcast();
  Stream<String> get messages => _messages.stream;

  Timer? _poll;
  String _signature = '';

  /// The place sync shows: origin if there is one, else the first.
  PlaceInfo? get primaryPlace {
    if (places.isEmpty) return null;
    return places.firstWhere((p) => p.name == 'origin', orElse: () => places.first);
  }

  List<WorkFile> get workFiles => (look?.files ?? []).where((f) => f.state != WorkState.unchanged).toList();
  int get unchangedCount => (look?.files ?? []).where((f) => f.state == WorkState.unchanged).length;
  int get pickedCount => (look?.files ?? []).where((f) => f.picked != PickedKind.none).length;

  /// Saves on the open draft that the primary place doesn't have yet:
  /// history is linear, so they're the newest `toSend`.
  bool isUnsent(SaveInfo s) {
    final n = sync?.toSend ?? 0;
    final i = history.indexWhere((h) => h.version == s.version);
    return i >= 0 && i < n;
  }

  void say(String message) => _messages.add(message);

  // ---------------------------------------------------------------- spaces

  /// `%APPDATA%\duh-studio` on Windows, else `$XDG_CONFIG_HOME/duh-studio`
  /// or `~/.config/duh-studio`.
  static String _defaultConfigDir() => configDirFor(Platform.environment, windows: Platform.isWindows);

  @visibleForTesting
  static String configDirFor(Map<String, String> env, {required bool windows}) {
    if (windows) {
      final appData = env['APPDATA'] ?? '${env['USERPROFILE'] ?? '.'}\\AppData\\Roaming';
      return '$appData\\duh-studio';
    }
    final home = env['HOME'] ?? '.';
    return '${env['XDG_CONFIG_HOME'] ?? '$home/.config'}/duh-studio';
  }

  // -------------------------------------------------------------- settings

  Future<void> loadSettings() async {
    try {
      final f = File('$_configDir/settings.json');
      if (!await f.exists()) return;
      final json = jsonDecode(await f.readAsString()) as Map<String, dynamic>;
      appearance.value = ThemeMode.values.firstWhere((m) => m.name == json['appearance'], orElse: () => ThemeMode.system);
    } catch (_) {
      // A broken settings file means defaults, not a broken app.
    }
  }

  Future<void> setAppearance(ThemeMode mode) async {
    appearance.value = mode;
    if (!persist) return;
    try {
      await Directory(_configDir).create(recursive: true);
      await File('$_configDir/settings.json').writeAsString(jsonEncode({'appearance': mode.name}));
    } catch (_) {}
  }

  /// System → dark → light → system.
  Future<void> nextAppearance() => setAppearance(switch (appearance.value) {
        ThemeMode.system => ThemeMode.dark,
        ThemeMode.dark => ThemeMode.light,
        ThemeMode.light => ThemeMode.system,
      });

  Future<void> loadRecent() async {
    try {
      final f = File('$_configDir/recent.json');
      if (await f.exists()) {
        recent = (jsonDecode(await f.readAsString()) as List).cast<String>();
        notifyListeners();
      }
    } catch (_) {
      recent = [];
    }
  }

  Future<void> _remember(String root) async {
    recent = [root, ...recent.where((r) => r != root)].take(12).toList();
    if (!persist) return;
    try {
      await Directory(_configDir).create(recursive: true);
      await File('$_configDir/recent.json').writeAsString(jsonEncode(recent));
    } catch (_) {
      // Remembering is a convenience; never fail an open over it.
    }
  }

  Future<void> forgetRecent(String root) async {
    recent = recent.where((r) => r != root).toList();
    notifyListeners();
    if (!persist) return;
    try {
      await File('$_configDir/recent.json').writeAsString(jsonEncode(recent));
    } catch (_) {}
  }

  Future<bool> openSpace(String folder) => _enter(() => engine.openSpace(folder), 'Opening');
  Future<bool> startSpace(String folder) => _enter(() => engine.startSpace(folder), 'Starting a space');
  Future<bool> getSpace(String url, String dest, List<String> options) =>
      _enter(() => engine.getSpace(url, dest, options), 'Getting a copy');

  Future<bool> _enter(Future<SpaceInfo> Function() how, String what) async {
    busy = what;
    notifyListeners();
    try {
      final s = await how();
      _close();
      space = s;
      selection = const WorkSelected();
      await _remember(s.root);
      await refresh();
      unawaited(refreshSync());
      if (watch) _poll = Timer.periodic(const Duration(milliseconds: 1500), (_) => _checkForChanges());
      return true;
    } catch (e) {
      say(explain(e));
      return false;
    } finally {
      busy = null;
      notifyListeners();
    }
  }

  void _close() {
    _poll?.cancel();
    _poll = null;
    space = null;
    look = null;
    history = [];
    drafts = [];
    labels = [];
    places = [];
    detail = null;
    strips.clear();
    sync = null;
    syncError = null;
    health = null;
    checkedAt = null;
    _signature = '';
  }

  void closeSpace() {
    _close();
    notifyListeners();
  }

  @override
  void dispose() {
    message.dispose();
    _poll?.cancel();
    _messages.close();
    super.dispose();
  }

  // --------------------------------------------------------------- refresh

  /// Reload everything shown about the open space.
  Future<void> refresh() async {
    final f = folder;
    if (f == null) return;
    try {
      final results = await Future.wait([
        engine.look(f),
        engine.history(f, max: 500),
        engine.drafts(f),
        engine.labels(f),
        engine.places(f),
      ]);
      if (folder != f) return;
      look = results[0] as LookResult;
      history = results[1] as List<SaveInfo>;
      drafts = results[2] as List<DraftInfo>;
      labels = results[3] as List<LabelInfo>;
      places = results[4] as List<PlaceInfo>;
      space = SpaceInfo(root: space!.root, name: space!.name, draft: look!.draft);
      if (watch) _signature = await _stat(f);
      _pruneStrips();
      final sel = selection;
      if (sel is SaveSelected) {
        if (history.any((h) => h.version == sel.version)) {
          detail ??= await engine.inspect(f, sel.version);
        } else {
          selection = const WorkSelected();
          detail = null;
        }
      }
    } catch (e) {
      say(explain(e));
    }
    notifyListeners();
  }

  void _pruneStrips() {
    final live = {for (final w in workFiles) _stripKey(w)};
    strips.removeWhere((k, _) => !live.contains(k));
  }

  String _stripKey(WorkFile w) => '${w.path}|${w.size}|${w.state.name}|${w.picked.name}';

  /// The change strip for a file in "your files", computed once per state.
  List<Segment>? stripFor(WorkFile w) {
    final key = _stripKey(w);
    final cached = strips[key];
    if (cached != null) return cached;
    if (w.state == WorkState.missing || w.picked == PickedKind.deleted) return const [];
    final f = folder;
    if (f == null) return null;
    strips[key] = const [];
    engine.compareFile(f, w.path).then((s) {
      strips[key] = s;
      notifyListeners();
    }).catchError((Object _) {});
    return null;
  }

  /// A cheap fingerprint of the folder (names, sizes, times) and of duh's
  /// own records, to notice edits and saves made elsewhere without reading
  /// any file's contents.
  static Future<String> _stat(String root) async {
    final b = StringBuffer();
    try {
      await for (final e in Directory(root).list(recursive: true, followLinks: false)) {
        final rel = e.path.substring(root.length);
        final inDuh = rel.startsWith('/.duh/');
        if (inDuh && !(rel.startsWith('/.duh/refs') || rel == '/.duh/index')) continue;
        if (e is File) {
          final s = await e.stat();
          b.write('$rel:${s.size}:${s.modified.microsecondsSinceEpoch};');
        }
      }
    } catch (_) {}
    return b.toString();
  }

  bool _checking = false;
  Future<void> _checkForChanges() async {
    final f = folder;
    if (f == null || _checking || busy != null || picking.isNotEmpty) return;
    _checking = true;
    try {
      final sig = await _stat(f);
      if (sig != _signature) {
        _signature = sig;
        await refresh();
      }
    } finally {
      _checking = false;
    }
  }

  // ------------------------------------------------------------- selection

  Future<void> select(Selection s) async {
    selection = s;
    detail = null;
    notifyListeners();
    final f = folder;
    if (s is SaveSelected && f != null) {
      try {
        final d = await engine.inspect(f, s.version);
        if (selection is SaveSelected && (selection as SaveSelected).version == s.version) {
          detail = d;
          notifyListeners();
        }
      } catch (e) {
        say(explain(e));
      }
    }
  }

  /// Move the selection up (-1) or down (+1) the timeline.
  void step(int delta) {
    final order = <Selection>[const WorkSelected(), ...history.map((h) => SaveSelected(h.version))];
    final sel = selection;
    final i = sel is WorkSelected ? 0 : order.indexWhere((o) => o is SaveSelected && o.version == (sel as SaveSelected).version);
    final j = (i + delta).clamp(0, order.length - 1);
    if (j != i) select(order[j]);
  }

  // --------------------------------------------------------------- actions

  /// Run a change, then refresh. Errors become messages; returns success.
  Future<bool> _do(String what, Future<void> Function(String folder) action, {String? done}) async {
    final f = folder;
    if (f == null) return false;
    busy = what;
    notifyListeners();
    try {
      await action(f);
      if (done != null) say(done);
      return true;
    } catch (e) {
      say(explain(e));
      return false;
    } finally {
      busy = null;
      await refresh();
    }
  }

  Future<void> togglePick(WorkFile w) async {
    final f = folder;
    if (f == null || picking.contains(w.path)) return;
    picking.add(w.path);
    notifyListeners();
    try {
      if (w.picked != PickedKind.none && w.state != WorkState.changed) {
        await engine.unpick(f, [w.path]);
      } else {
        await engine.pick(f, [w.path]);
      }
    } catch (e) {
      say(explain(e));
    } finally {
      picking.remove(w.path);
      await refresh();
    }
  }

  Future<void> pickAll() async {
    final f = folder;
    if (f == null) return;
    final paths = workFiles
        .where((w) => w.picked == PickedKind.none || w.state == WorkState.changed)
        .map((w) => w.path)
        .toList();
    if (paths.isEmpty) return;
    picking.addAll(paths);
    notifyListeners();
    try {
      await engine.pick(f, paths);
    } catch (e) {
      say(explain(e));
    } finally {
      picking.removeAll(paths);
      await refresh();
    }
  }

  Future<void> unpickAll() async {
    final paths = workFiles.where((w) => w.picked != PickedKind.none).map((w) => w.path).toList();
    if (paths.isEmpty) return;
    await _do('Un-picking', (f) => engine.unpick(f, paths));
  }

  Future<String> suggestMessage() async {
    final f = folder;
    if (f == null) return '';
    try {
      return await engine.suggestMessage(f);
    } catch (_) {
      return '';
    }
  }

  /// The message being written for the next save.
  final message = TextEditingController();

  /// Save your work: what's picked, or everything changed if nothing is,
  /// with the message written (or the suggested one).
  Future<bool> saveWork() async {
    if (workFiles.isEmpty) {
      say('Nothing to save: no files have changed.');
      return false;
    }
    if (pickedCount == 0) await pickAll();
    final text = message.text.trim().isEmpty ? await suggestMessage() : message.text.trim();
    final ok = await save(text);
    if (ok) message.clear();
    return ok;
  }

  Future<bool> save(String message) async {
    final ok = await _do('Saving', (f) => engine.save(f, message));
    if (ok) {
      final s = history.isNotEmpty ? history.first : null;
      say(s == null ? 'Saved' : 'Saved “${s.message.split('\n').first}”');
      unawaited(refreshSync());
    }
    return ok;
  }

  Future<bool> bringBack(String path, {String? from, String? asPath}) => _do(
        'Bringing back $path',
        (f) => engine.bringBack(f, path, from: from, asPath: asPath),
        done: asPath == null ? 'Brought back $path' : 'Saved a copy of $path as $asPath',
      );

  Future<bool> startDraft(String name, {String? at, bool open = true}) => _do('Starting draft $name', (f) async {
        await engine.startDraft(f, name, at: at);
        if (open) await engine.openDraft(f, name);
      }, done: open ? 'Opened the new draft $name' : 'Started draft $name');

  Future<bool> openDraft(String name) async {
    final ok = await _do('Opening $name', (f) => engine.openDraft(f, name), done: 'Opened draft $name');
    if (ok) {
      selection = const WorkSelected();
      unawaited(refreshSync());
    }
    return ok;
  }

  Future<bool> deleteDraft(String name) => _do('Deleting $name', (f) => engine.deleteDraft(f, name), done: 'Deleted draft $name');
  Future<bool> renameDraft(String name) => _do('Renaming', (f) => engine.renameDraft(f, name), done: 'Renamed the draft to $name');
  Future<bool> addLabel(String name, String version) =>
      _do('Labelling', (f) => engine.addLabel(f, name, version), done: 'Labelled it $name');
  Future<bool> removeLabel(String name) => _do('Removing label', (f) => engine.removeLabel(f, name), done: 'Removed label $name');

  Future<bool> rewind(String version, RewindMode mode) async {
    final ok = await _do('Rewinding', (f) => engine.rewind(f, version, mode), done: 'Rewound');
    if (ok) {
      selection = const WorkSelected();
      unawaited(refreshSync());
    }
    return ok;
  }

  Future<bool> addPlace(String name, String url, List<String> options) async {
    final ok = await _do('Adding $name', (f) => engine.addPlace(f, name, url, options), done: 'Added the place $name');
    if (ok) unawaited(refreshSync());
    return ok;
  }

  Future<bool> removePlace(String name) => _do('Removing $name', (f) => engine.removePlace(f, name), done: 'Removed the place $name');

  Future<void> refreshSync() async {
    final f = folder;
    final p = primaryPlace;
    if (f == null || p == null) {
      sync = null;
      notifyListeners();
      return;
    }
    try {
      sync = await engine.syncStatus(f, p.name);
      syncError = null;
    } catch (e) {
      sync = null;
      syncError = explain(e);
    }
    notifyListeners();
  }

  Future<bool> send({String? place, String? draft, bool force = false}) async {
    final p = place ?? primaryPlace?.name;
    if (p == null) return false;
    final ok = await _do('Sending to $p', (f) async {
      final t = await engine.send(f, p, draft: draft, force: force);
      say(t.saves == 0 ? '$p already had everything' : 'Sent ${count(t.saves, 'save')} to $p/${t.draft} (${_bytes(t.bytes)})');
    });
    await refreshSync();
    return ok;
  }

  Future<bool> receive({String? place, String? draft, String? into}) async {
    final p = place ?? primaryPlace?.name;
    if (p == null) return false;
    final ok = await _do('Receiving from $p', (f) async {
      final t = await engine.receive(f, p, draft: draft, into: into);
      say(t.saves == 0 ? 'Nothing new from $p' : 'Received ${count(t.saves, 'save')} into ${t.draft} (${_bytes(t.bytes)})');
    });
    await refreshSync();
    return ok;
  }

  Future<CheckResult?> check({String? place}) async {
    final f = folder;
    if (f == null) return null;
    busy = place == null ? 'Checking every save' : 'Checking $place';
    notifyListeners();
    try {
      final r = await engine.check(f, place: place);
      if (place == null) {
        health = r;
        checkedAt = DateTime.now();
      }
      return r;
    } catch (e) {
      say(explain(e));
      return null;
    } finally {
      busy = null;
      notifyListeners();
    }
  }

  Future<List<JournalItem>> journal() async {
    final f = folder;
    final d = space?.draft;
    if (f == null || d == null) return [];
    try {
      return await engine.journal(f, d);
    } catch (e) {
      say(explain(e));
      return [];
    }
  }

  static String _bytes(int n) {
    if (n >= 1000000) return '${(n / 1e6).toStringAsFixed(1)} MB';
    if (n >= 1000) return '${(n / 1e3).toStringAsFixed(1)} KB';
    return '$n B';
  }
}
