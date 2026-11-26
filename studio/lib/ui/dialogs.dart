import 'dart:io';

import 'package:flutter/material.dart';

import '../engine.dart';
import '../state.dart';
import 'theme.dart';
import 'widgets.dart';

/// Ask before something that can't be undone. The sentence says what will
/// happen; the button names the action.
Future<bool> confirm(
  BuildContext context, {
  required String title,
  required String body,
  required String action,
  bool danger = false,
}) async {
  final p = Palette.of(context);
  final ok = await showDialog<bool>(
    context: context,
    builder: (context) => AlertDialog(
      title: Text(title, style: const TextStyle(fontSize: 19, fontWeight: FontWeight.w700)),
      content: ConstrainedBox(constraints: const BoxConstraints(maxWidth: 460), child: Text(body, style: TextStyle(color: p.muted, height: 1.5))),
      actions: [
        TextButton(onPressed: () => Navigator.pop(context, false), child: Text('Cancel', style: TextStyle(color: p.muted))),
        FilledButton(
          onPressed: () => Navigator.pop(context, true),
          style: FilledButton.styleFrom(backgroundColor: danger ? p.danger : p.accent, foregroundColor: p.onAccent),
          child: Text(action),
        ),
      ],
    ),
  );
  return ok ?? false;
}

/// Ask for one line of text.
Future<String?> prompt(
  BuildContext context, {
  required String title,
  required String label,
  String initial = '',
  String action = 'OK',
  String? help,
}) async {
  final controller = TextEditingController(text: initial);
  final p = Palette.of(context);
  return showDialog<String>(
    context: context,
    builder: (context) => AlertDialog(
      title: Text(title, style: const TextStyle(fontSize: 19, fontWeight: FontWeight.w700)),
      content: SizedBox(
        width: 420,
        child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
          TextField(
            controller: controller,
            autofocus: true,
            decoration: InputDecoration(labelText: label),
            onSubmitted: (v) => Navigator.pop(context, v.trim()),
          ),
          if (help != null) ...[const SizedBox(height: 10), Text(help, style: TextStyle(color: p.muted, fontSize: 12))],
        ]),
      ),
      actions: [
        TextButton(onPressed: () => Navigator.pop(context), child: Text('Cancel', style: TextStyle(color: p.muted))),
        FilledButton(onPressed: () => Navigator.pop(context, controller.text.trim()), child: Text(action)),
      ],
    ),
  );
}

/// Choose a folder, in the app (no system dialog needed).
Future<String?> pickFolder(BuildContext context, {required String title, String action = 'Choose this folder', bool allowNew = false}) {
  return showDialog<String>(
    context: context,
    builder: (_) => _FolderPicker(title: title, action: action, allowNew: allowNew),
  );
}

class _FolderPicker extends StatefulWidget {
  const _FolderPicker({required this.title, required this.action, required this.allowNew});
  final String title, action;
  final bool allowNew;

  @override
  State<_FolderPicker> createState() => _FolderPickerState();
}

class _FolderPickerState extends State<_FolderPicker> {
  late Directory dir = Directory(Platform.environment['HOME'] ?? '/');
  List<Directory> children = [];
  final path = TextEditingController();

  @override
  void initState() {
    super.initState();
    _load();
  }

  void _load() {
    path.text = dir.path;
    try {
      children = dir.listSync(followLinks: false).whereType<Directory>().where((d) => !d.uri.pathSegments.where((s) => s.isNotEmpty).last.startsWith('.')).toList()
        ..sort((a, b) => a.path.toLowerCase().compareTo(b.path.toLowerCase()));
    } catch (_) {
      children = [];
    }
    setState(() {});
  }

  void _go(Directory d) {
    dir = d;
    _load();
  }

  @override
  Widget build(BuildContext context) {
    final p = Palette.of(context);
    final isSpace = Directory('${dir.path}/.duh').existsSync();
    return AlertDialog(
      title: Text(widget.title, style: const TextStyle(fontSize: 19, fontWeight: FontWeight.w700)),
      content: SizedBox(
        width: 560,
        height: 440,
        child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          Row(children: [
            IconButton(
              tooltip: 'Up one folder',
              onPressed: dir.parent.path == dir.path ? null : () => _go(dir.parent),
              icon: const Icon(Icons.arrow_upward_rounded),
            ),
            IconButton(
              tooltip: 'Home',
              onPressed: () => _go(Directory(Platform.environment['HOME'] ?? '/')),
              icon: const Icon(Icons.home_outlined),
            ),
            const SizedBox(width: 6),
            Expanded(
              child: TextField(
                controller: path,
                style: mono.copyWith(fontSize: 13),
                decoration: const InputDecoration(isDense: true),
                onSubmitted: (v) {
                  final d = Directory(v.trim());
                  if (d.existsSync()) _go(d);
                },
              ),
            ),
          ]),
          const SizedBox(height: 10),
          if (isSpace)
            Padding(
              padding: const EdgeInsets.only(bottom: 8),
              child: Chip2('This folder is a duh space', color: p.accent, background: p.accentSoft, icon: Icons.check_rounded),
            ),
          Expanded(
            child: Container(
              decoration: BoxDecoration(border: Border.all(color: p.line), borderRadius: BorderRadius.circular(12)),
              child: children.isEmpty
                  ? Center(child: Text('No folders inside', style: TextStyle(color: p.muted)))
                  : ListView.builder(
                      itemCount: children.length,
                      itemBuilder: (context, i) {
                        final d = children[i];
                        final name = d.uri.pathSegments.where((s) => s.isNotEmpty).last;
                        final space = Directory('${d.path}/.duh').existsSync();
                        return ListTile(
                          dense: true,
                          leading: Icon(space ? Icons.auto_awesome_motion_rounded : Icons.folder_outlined, color: space ? p.accent : p.muted, size: 20),
                          title: Text(name),
                          onTap: () => _go(d),
                        );
                      },
                    ),
            ),
          ),
        ]),
      ),
      actions: [
        if (widget.allowNew)
          TextButton.icon(
            onPressed: () async {
              final name = await prompt(context, title: 'New folder', label: 'Name', action: 'Create');
              if (name == null || name.isEmpty) return;
              final d = Directory('${dir.path}/$name');
              try {
                d.createSync();
                _go(d);
              } catch (e) {
                if (context.mounted) ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text('Couldn\'t create it: $e')));
              }
            },
            icon: const Icon(Icons.create_new_folder_outlined, size: 18),
            label: const Text('New folder'),
          ),
        TextButton(onPressed: () => Navigator.pop(context), child: Text('Cancel', style: TextStyle(color: p.muted))),
        FilledButton(onPressed: () => Navigator.pop(context, dir.path), child: Text(widget.action)),
      ],
    );
  }
}

/// Add a place: a folder, or cloud storage with its settings.
Future<void> addPlaceDialog(BuildContext context, AppState app) async {
  await showDialog<void>(context: context, builder: (_) => _AddPlace(app: app));
}

class _AddPlace extends StatefulWidget {
  const _AddPlace({required this.app});
  final AppState app;
  @override
  State<_AddPlace> createState() => _AddPlaceState();
}

class _AddPlaceState extends State<_AddPlace> {
  String kind = 'folder';
  final name = TextEditingController(text: 'origin');
  final url = TextEditingController();
  final endpoint = TextEditingController();
  final region = TextEditingController();
  final key = TextEditingController();
  final secret = TextEditingController();

  @override
  void initState() {
    super.initState();
    if (widget.app.places.any((p) => p.name == 'origin')) name.text = 'backup';
  }

  @override
  Widget build(BuildContext context) {
    final p = Palette.of(context);
    Widget field(TextEditingController c, String label, {String? hint, bool secret = false}) => Padding(
          padding: const EdgeInsets.only(bottom: 10),
          child: TextField(controller: c, obscureText: secret, decoration: InputDecoration(labelText: label, hintText: hint)),
        );
    return AlertDialog(
      title: const Text('Add a place', style: TextStyle(fontSize: 19, fontWeight: FontWeight.w700)),
      content: SizedBox(
        width: 480,
        child: SingleChildScrollView(
          child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
            Text('A place is somewhere your saves are shared or backed up: a folder (a drive, a NAS) or cloud storage.',
                style: TextStyle(color: p.muted, height: 1.45)),
            const SizedBox(height: 14),
            SegmentedButton<String>(
              segments: const [
                ButtonSegment(value: 'folder', label: Text('Folder'), icon: Icon(Icons.folder_outlined)),
                ButtonSegment(value: 's3', label: Text('S3'), icon: Icon(Icons.cloud_outlined)),
                ButtonSegment(value: 'url', label: Text('Other URL'), icon: Icon(Icons.link)),
              ],
              selected: {kind},
              onSelectionChanged: (s) => setState(() => kind = s.first),
            ),
            const SizedBox(height: 14),
            field(name, 'Name', hint: 'origin'),
            if (kind == 'folder')
              Row(children: [
                Expanded(child: field(url, 'Folder', hint: '/mnt/nas/projects/bracket')),
                const SizedBox(width: 8),
                Padding(
                  padding: const EdgeInsets.only(bottom: 10),
                  child: QuietButton(
                    label: 'Browse',
                    onPressed: () async {
                      final f = await pickFolder(context, title: 'Where should the place be?', allowNew: true);
                      if (f != null) setState(() => url.text = f);
                    },
                  ),
                ),
              ]),
            if (kind == 's3') ...[
              field(url, 'Bucket and path', hint: 's3://my-bucket/projects/bracket'),
              field(endpoint, 'Endpoint (leave empty for AWS)', hint: 'https://s3.example.com'),
              field(region, 'Region', hint: 'us-east-1'),
              field(key, 'Access key ID'),
              field(secret, 'Secret access key', secret: true),
              Text('Credentials are stored in plain text in this space\'s settings.', style: TextStyle(color: p.warn, fontSize: 12)),
            ],
            if (kind == 'url') field(url, 'URL', hint: 'gcs://bucket/path, webdav://host/path, sftp://…'),
          ]),
        ),
      ),
      actions: [
        TextButton(onPressed: () => Navigator.pop(context), child: Text('Cancel', style: TextStyle(color: p.muted))),
        FilledButton(
          onPressed: () async {
            final opts = <String>[
              if (endpoint.text.trim().isNotEmpty) 'endpoint=${endpoint.text.trim()}',
              if (region.text.trim().isNotEmpty) 'region=${region.text.trim()}',
              if (key.text.trim().isNotEmpty) 'access_key_id=${key.text.trim()}',
              if (secret.text.trim().isNotEmpty) 'secret_access_key=${secret.text.trim()}',
            ];
            if (kind == 'folder' && url.text.trim().isNotEmpty) {
              // A folder place needs a space in it: start a bare one (history
              // only) when the folder is empty or new.
              final dir = Directory(url.text.trim());
              if (!Directory('${dir.path}/.duh').existsSync()) {
                try {
                  await widget.app.engine.startBareSpace(dir.path);
                } catch (e) {
                  widget.app.say(explain(e));
                  return;
                }
              }
            }
            final ok = await widget.app.addPlace(name.text.trim(), url.text.trim(), opts);
            if (ok && context.mounted) Navigator.pop(context);
          },
          child: const Text('Add place'),
        ),
      ],
    );
  }

}

/// When a draft and a place have both moved on: explain, and offer the
/// safe ways out first.
Future<void> divergedSheet(BuildContext context, AppState app) async {
  final p = Palette.of(context);
  final place = app.primaryPlace?.name ?? 'the place';
  final draft = app.space?.draft ?? 'main';
  final s = app.sync;
  await showDialog<void>(
    context: context,
    builder: (context) {
      Widget choice(String title, String sub, VoidCallback onTap, {bool primary = false, bool danger = false}) => Padding(
            padding: const EdgeInsets.only(bottom: 8),
            child: Material(
              color: primary ? p.accentSoft : (danger ? p.danger.withValues(alpha: 0.06) : p.panel),
              shape: RoundedRectangleBorder(
                borderRadius: BorderRadius.circular(12),
                side: BorderSide(color: primary ? p.accent : (danger ? p.danger.withValues(alpha: 0.4) : p.line), width: primary ? 1.5 : 1),
              ),
              child: InkWell(
                borderRadius: BorderRadius.circular(12),
                onTap: onTap,
                child: Padding(
                  padding: const EdgeInsets.all(14),
                  child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                    Text(title, style: TextStyle(fontWeight: FontWeight.w600, color: danger ? p.danger : p.text)),
                    const SizedBox(height: 2),
                    Text(sub, style: TextStyle(fontSize: 13, color: danger ? p.danger : p.muted)),
                  ]),
                ),
              ),
            ),
          );
      return AlertDialog(
        title: Text('$place and you have both saved since you last met', style: const TextStyle(fontSize: 19, fontWeight: FontWeight.w700)),
        content: SizedBox(
          width: 520,
          child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
            Text(
              '$place has ${count(s?.toReceive ?? 0, 'save')} you don\'t have, and you have ${s?.toSend ?? 0} it doesn\'t. '
              'duh never merges, so nothing happens until you choose.',
              style: TextStyle(color: p.muted, height: 1.5),
            ),
            const SizedBox(height: 16),
            choice('Receive theirs into a new draft', 'Keeps both: $place\'s saves arrive as “$draft-from-$place”.', () async {
              Navigator.pop(context);
              await app.receive(draft: draft, into: '$draft-from-$place');
            }, primary: true),
            choice('Send yours to a new draft on $place', 'Keeps both: yours go up as “$draft-mine”.', () async {
              Navigator.pop(context);
              await app.send(draft: '$draft-mine');
            }),
            choice('Overwrite $place\'s $draft', 'Throws away $place\'s ${count(s?.toReceive ?? 0, 'save')} for everyone. You\'ll be asked again.', () async {
              final sure = await confirm(context,
                  title: 'Overwrite $place\'s $draft?',
                  body: 'The ${count(s?.toReceive ?? 0, 'save')} only $place has will be gone for everyone who uses it. This can\'t be undone from here.',
                  action: 'Overwrite',
                  danger: true);
              if (!sure || !context.mounted) return;
              Navigator.pop(context);
              await app.send(force: true);
            }, danger: true),
          ]),
        ),
        actions: [TextButton(onPressed: () => Navigator.pop(context), child: Text('Not now', style: TextStyle(color: p.muted)))],
      );
    },
  );
}

/// The result of a check, in plain words.
Future<void> showCheck(BuildContext context, CheckResult r, {String? place}) {
  final p = Palette.of(context);
  final intact = r.problems.isEmpty;
  return showDialog<void>(
    context: context,
    builder: (context) => AlertDialog(
      title: Row(children: [
        Icon(intact ? Icons.verified_rounded : Icons.error_outline_rounded, color: intact ? p.ok : p.danger),
        const SizedBox(width: 10),
        Expanded(child: Text(intact ? 'Everything is intact' : '${count(r.problems.length, 'problem')} found', style: const TextStyle(fontSize: 19, fontWeight: FontWeight.w700))),
      ]),
      content: SizedBox(
        width: 520,
        child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
          Text(
            '${place ?? 'This space'}: ${count(r.saves, 'save')}, ${count(r.fileVersions, 'file version')}, '
            '${count(r.objects, 'stored piece')}${r.solids > 0 ? ' and ${count(r.solids, 'solid copy', 'solid copies')}' : ''} checked.',
            style: TextStyle(color: p.muted, height: 1.5),
          ),
          if (intact) ...[
            const SizedBox(height: 8),
            Text('Every save reads back exactly as it was saved.', style: TextStyle(color: p.muted, height: 1.5)),
          ],
          if (!intact)
            ConstrainedBox(
              constraints: const BoxConstraints(maxHeight: 280),
              child: ListView(shrinkWrap: true, children: [
                for (final problem in r.problems)
                  Padding(
                    padding: const EdgeInsets.only(top: 8),
                    child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
                      Icon(Icons.close_rounded, size: 16, color: p.danger),
                      const SizedBox(width: 6),
                      Expanded(child: Text(problem, style: const TextStyle(fontSize: 13))),
                    ]),
                  ),
              ]),
            ),
        ]),
      ),
      actions: [FilledButton(onPressed: () => Navigator.pop(context), child: const Text('Done'))],
    ),
  );
}

/// The journal: everywhere the open draft has been, to recover lost work.
Future<void> showJournal(BuildContext context, AppState app) async {
  final entries = (await app.journal()).reversed.toList();
  if (!context.mounted) return;
  final p = Palette.of(context);
  await showDialog<void>(
    context: context,
    builder: (context) => AlertDialog(
      title: const Text('Recover', style: TextStyle(fontSize: 19, fontWeight: FontWeight.w700)),
      content: SizedBox(
        width: 560,
        height: 420,
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Text('Everywhere “${app.space?.draft}” has been, newest first. Start a draft from any of them to get that work back.',
              style: TextStyle(color: p.muted, height: 1.45)),
          const SizedBox(height: 12),
          Expanded(
            child: entries.isEmpty
                ? Center(child: Text('Nothing recorded yet', style: TextStyle(color: p.muted)))
                : ListView.separated(
                    itemCount: entries.length,
                    separatorBuilder: (_, _) => Divider(height: 1, color: p.line),
                    itemBuilder: (context, i) {
                      final e = entries[i];
                      return ListTile(
                        dense: true,
                        title: Text(e.what.isEmpty ? '(no note)' : e.what),
                        subtitle: Text(e.version.substring(0, 12), style: mono.copyWith(color: p.muted)),
                        trailing: TextButton(
                          onPressed: () async {
                            final name = await prompt(context,
                                title: 'Start a draft here', label: 'Draft name', initial: 'recovered', action: 'Start and open');
                            if (name == null || name.isEmpty || !context.mounted) return;
                            Navigator.pop(context);
                            await app.startDraft(name, at: e.version);
                          },
                          child: const Text('Start a draft here'),
                        ),
                      );
                    },
                  ),
          ),
        ]),
      ),
      actions: [TextButton(onPressed: () => Navigator.pop(context), child: const Text('Close'))],
    ),
  );
}
