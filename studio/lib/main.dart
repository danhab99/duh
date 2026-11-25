import 'dart:async';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';

import 'engine.dart';
import 'src/rust/frb_generated.dart';
import 'state.dart';
import 'ui/dialogs.dart';
import 'ui/shell.dart';
import 'ui/theme.dart';
import 'ui/welcome.dart';
import 'ui/widgets.dart';

Future<void> main(List<String> args) async {
  await RustLib.init(externalLibrary: _engineLibrary());
  final engine = BridgeEngine();

  // `--smoke`: prove the app starts and reaches duh's engine, then exit.
  if (args.contains('--smoke')) {
    stdout.writeln('duh Studio: engine ${engine.version}');
    exit(0);
  }

  final app = AppState(engine);
  await app.loadSettings();
  await app.loadRecent();
  // A folder given on the command line opens straight away.
  final folder = args.where((a) => !a.startsWith('-')).firstOrNull;
  if (folder != null) unawaited(app.openSpace(Directory(folder).absolute.path));
  runApp(StudioApp(app: app));
}

/// duh's engine: in the app bundle's `lib/` (as built by Nix or `flutter
/// build`), else the bridge crate's own build output while developing.
ExternalLibrary _engineLibrary() {
  const name = 'libduh_studio_native.so';
  final exeDir = File(Platform.resolvedExecutable).parent.path;
  for (final path in ['$exeDir/lib/$name', 'native/target/release/$name', 'native/target/debug/$name']) {
    if (File(path).existsSync()) return ExternalLibrary.open(path);
  }
  return ExternalLibrary.open(name);
}

class StudioApp extends StatefulWidget {
  const StudioApp({super.key, required this.app});
  final AppState app;

  @override
  State<StudioApp> createState() => _StudioAppState();
}

class _StudioAppState extends State<StudioApp> {
  final messenger = GlobalKey<ScaffoldMessengerState>();
  final navigator = GlobalKey<NavigatorState>();
  late final StreamSubscription<String> _sub;

  AppState get app => widget.app;

  @override
  void initState() {
    super.initState();
    _sub = app.messages.listen((m) {
      messenger.currentState
        ?..hideCurrentSnackBar()
        ..showSnackBar(SnackBar(content: Text(m), duration: Duration(milliseconds: 2500 + 40 * m.length)));
    });
  }

  @override
  void dispose() {
    _sub.cancel();
    super.dispose();
  }

  void _palette() {
    final context = navigator.currentContext;
    if (context != null) showCommandPalette(context, app);
  }

  @override
  Widget build(BuildContext context) {
    return ValueListenableBuilder(
      valueListenable: app.appearance,
      builder: (context, mode, _) => _app(mode),
    );
  }

  Widget _app(ThemeMode mode) {
    return MaterialApp(
      title: 'duh Studio',
      debugShowCheckedModeBanner: false,
      theme: studioTheme(Brightness.light),
      darkTheme: studioTheme(Brightness.dark),
      themeMode: mode,
      scaffoldMessengerKey: messenger,
      navigatorKey: navigator,
      home: ListenableBuilder(
        listenable: app,
        builder: (context, _) => CallbackShortcuts(
          bindings: {
            const SingleActivator(LogicalKeyboardKey.keyK, control: true): _palette,
            const SingleActivator(LogicalKeyboardKey.keyS, control: true): () {
              if (app.space != null && app.busy == null) app.saveWork();
            },
            const SingleActivator(LogicalKeyboardKey.keyR, control: true): () => app.refresh(),
            const SingleActivator(LogicalKeyboardKey.arrowUp, alt: true): () => app.step(-1),
            const SingleActivator(LogicalKeyboardKey.arrowDown, alt: true): () => app.step(1),
          },
          child: Focus(
            autofocus: true,
            child: AnimatedSwitcher(
              duration: motion(context, 220),
              child: app.space == null
                  ? Welcome(key: const ValueKey('welcome'), app: app)
                  : SpaceScreen(key: ValueKey(app.folder), app: app, onPalette: _palette),
            ),
          ),
        ),
      ),
    );
  }
}

/// One thing the palette can do.
class Command {
  const Command(this.title, this.run, {this.hint, this.icon = Icons.chevron_right_rounded});
  final String title;
  final String? hint;
  final IconData icon;
  final Future<void> Function(BuildContext) run;
}

List<Command> commandsFor(AppState app) {
  if (app.space == null) return [];
  return [
    Command('Save your files', (_) => app.saveWork(), hint: 'Ctrl S', icon: Icons.check_rounded),
    Command('Pick all changed files', (_) => app.pickAll(), icon: Icons.done_all_rounded),
    if (app.primaryPlace != null) ...[
      Command('Send to ${app.primaryPlace!.name}', (_) => app.send(), icon: Icons.upload_rounded),
      Command('Receive from ${app.primaryPlace!.name}', (_) => app.receive(), icon: Icons.download_rounded),
    ],
    Command('Add a place…', (c) => addPlaceDialog(c, app), icon: Icons.cloud_outlined),
    Command('Check that every save is intact', (c) async {
      final r = await app.check();
      if (r != null && c.mounted) await showCheck(c, r);
    }, icon: Icons.verified_outlined),
    Command('Start a new draft…', (c) async {
      final name = await prompt(c, title: 'Start a new draft', label: 'Name', action: 'Start and open');
      if (name != null && name.isNotEmpty) await app.startDraft(name);
    }, icon: Icons.call_split_rounded),
    for (final d in app.drafts.where((d) => !d.open)) Command('Open draft ${d.name}', (_) => app.openDraft(d.name), icon: Icons.swap_horiz_rounded),
    for (final l in app.labels) Command('Go to label ${l.name}', (_) => app.select(SaveSelected(l.version)), icon: Icons.label_outline_rounded),
    Command('Recover lost work…', (c) => showJournal(c, app), icon: Icons.history_rounded),
    for (final (mode, name, icon) in [
      (ThemeMode.dark, 'dark', Icons.dark_mode_outlined),
      (ThemeMode.light, 'light', Icons.light_mode_outlined),
      (ThemeMode.system, 'the system\'s', Icons.brightness_auto_outlined),
    ])
      if (app.appearance.value != mode) Command('Use $name appearance', (_) => app.setAppearance(mode), icon: icon),
    Command('Look again', (_) => app.refresh(), hint: 'Ctrl R', icon: Icons.refresh_rounded),
    Command('All spaces', (_) async => app.closeSpace(), icon: Icons.grid_view_rounded),
  ];
}

Future<void> showCommandPalette(BuildContext context, AppState app) async {
  final all = commandsFor(app);
  if (all.isEmpty) return;
  final chosen = await showDialog<Command>(
    context: context,
    builder: (context) => _Palette(commands: all),
  );
  if (chosen != null && context.mounted) await chosen.run(context);
}

class _Palette extends StatefulWidget {
  const _Palette({required this.commands});
  final List<Command> commands;
  @override
  State<_Palette> createState() => _PaletteState();
}

class _PaletteState extends State<_Palette> {
  String query = '';
  int index = 0;

  List<Command> get shown {
    final q = query.toLowerCase().trim();
    if (q.isEmpty) return widget.commands;
    final words = q.split(RegExp(r'\s+'));
    return widget.commands.where((c) => words.every((w) => c.title.toLowerCase().contains(w))).toList();
  }

  @override
  Widget build(BuildContext context) {
    final p = Palette.of(context);
    final list = shown;
    if (index >= list.length) index = list.isEmpty ? 0 : list.length - 1;
    return Align(
      alignment: const Alignment(0, -0.55),
      child: Material(
        color: p.panel,
        elevation: 24,
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(16), side: BorderSide(color: p.line)),
        child: SizedBox(
          width: 560,
          child: Column(mainAxisSize: MainAxisSize.min, children: [
            CallbackShortcuts(
              bindings: {
                const SingleActivator(LogicalKeyboardKey.arrowDown): () => setState(() => index = (index + 1).clamp(0, list.length - 1)),
                const SingleActivator(LogicalKeyboardKey.arrowUp): () => setState(() => index = (index - 1).clamp(0, list.length - 1)),
              },
              child: TextField(
                autofocus: true,
                style: const TextStyle(fontSize: 16),
                decoration: InputDecoration(
                  hintText: 'What do you want to do?',
                  prefixIcon: Icon(Icons.search_rounded, color: p.faint),
                  border: InputBorder.none,
                  enabledBorder: InputBorder.none,
                  focusedBorder: InputBorder.none,
                  fillColor: Colors.transparent,
                  contentPadding: const EdgeInsets.symmetric(vertical: 18),
                ),
                onChanged: (v) => setState(() {
                  query = v;
                  index = 0;
                }),
                onSubmitted: (_) {
                  if (list.isNotEmpty) Navigator.pop(context, list[index]);
                },
              ),
            ),
            Divider(height: 1, color: p.line),
            ConstrainedBox(
              constraints: const BoxConstraints(maxHeight: 380),
              child: list.isEmpty
                  ? Padding(padding: const EdgeInsets.all(20), child: Text('Nothing matches', style: TextStyle(color: p.muted)))
                  : ListView.builder(
                      shrinkWrap: true,
                      padding: const EdgeInsets.all(6),
                      itemCount: list.length,
                      itemBuilder: (context, i) {
                        final c = list[i];
                        return Material(
                          color: i == index ? p.accentSoft : Colors.transparent,
                          borderRadius: BorderRadius.circular(10),
                          child: ListTile(
                            dense: true,
                            leading: Icon(c.icon, size: 18, color: i == index ? p.accent : p.muted),
                            title: Text(c.title, style: const TextStyle(fontSize: 14)),
                            trailing: c.hint == null ? null : KeyHint(c.hint!),
                            onTap: () => Navigator.pop(context, c),
                          ),
                        );
                      },
                    ),
            ),
          ]),
        ),
      ),
    );
  }
}
