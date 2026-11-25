import 'package:flutter/material.dart';

import '../engine.dart';
import '../state.dart';
import 'dialogs.dart';
import 'theme.dart';
import 'widgets.dart';

/// The right-hand panel: whatever is selected in the timeline.
class Inspector extends StatelessWidget {
  const Inspector({super.key, required this.app});
  final AppState app;

  @override
  Widget build(BuildContext context) {
    final p = Palette.of(context);
    final sel = app.selection;
    return Container(
      decoration: BoxDecoration(color: p.panel, border: Border(left: BorderSide(color: p.line))),
      child: AnimatedSwitcher(
        duration: const Duration(milliseconds: 180),
        child: switch (sel) {
          WorkSelected() => WorkInspector(key: const ValueKey('work'), app: app),
          SaveSelected(:final version) => SaveInspector(key: ValueKey(version), app: app),
        },
      ),
    );
  }
}

class WorkInspector extends StatefulWidget {
  const WorkInspector({super.key, required this.app});
  final AppState app;

  @override
  State<WorkInspector> createState() => WorkInspectorState();
}

class WorkInspectorState extends State<WorkInspector> {
  String suggestion = '';
  int _pickedSeen = -1;

  AppState get app => widget.app;

  @override
  void initState() {
    super.initState();
    app.addListener(_onApp);
    _onApp();
  }

  @override
  void dispose() {
    app.removeListener(_onApp);
    super.dispose();
  }

  void _onApp() {
    if (app.pickedCount != _pickedSeen) {
      _pickedSeen = app.pickedCount;
      app.suggestMessage().then((s) {
        if (mounted) setState(() => suggestion = s.split('\n').first);
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final p = Palette.of(context);
    final files = app.workFiles;
    final picked = app.pickedCount;
    final allPicked = files.isNotEmpty && files.every((f) => f.picked != PickedKind.none && f.state != WorkState.changed);
    return Padding(
      padding: const EdgeInsets.all(22),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        Row(children: [
          const Expanded(child: Text('Your files', style: TextStyle(fontSize: 17, fontWeight: FontWeight.w700))),
          if (files.isNotEmpty)
            TextButton(
              onPressed: app.busy != null ? null : (allPicked ? app.unpickAll : app.pickAll),
              child: Text(allPicked ? 'Un-pick all' : 'Pick all'),
            ),
        ]),
        const SizedBox(height: 10),
        Expanded(
          child: files.isEmpty
              ? _Calm(unchanged: app.unchangedCount)
              : ListView(children: [
                  for (final f in files) _FileRow(app: app, file: f),
                  if (app.unchangedCount > 0)
                    Padding(
                      padding: const EdgeInsets.all(4),
                      child: Text(
                        '${app.unchangedCount} more file${app.unchangedCount == 1 ? '' : 's'} unchanged',
                        style: TextStyle(fontSize: 12, color: p.faint),
                      ),
                    ),
                ]),
        ),
        const SizedBox(height: 12),
        Text('What changed?', style: TextStyle(fontSize: 12, fontWeight: FontWeight.w600, color: p.muted)),
        const SizedBox(height: 8),
        TextField(
          controller: app.message,
          style: const TextStyle(fontSize: 14, height: 1.4),
          minLines: 2,
          maxLines: 4,
          decoration: InputDecoration(hintText: suggestion.isEmpty ? 'Describe this version' : suggestion),
        ),
        if (suggestion.isNotEmpty && files.isNotEmpty) ...[
          const SizedBox(height: 6),
          Row(children: [
            Expanded(
              child: Text('Leave it empty to use “$suggestion”', overflow: TextOverflow.ellipsis, style: TextStyle(fontSize: 12, color: p.faint)),
            ),
          ]),
        ],
        const SizedBox(height: 12),
        PrimaryButton(
          label: files.isEmpty
              ? 'Nothing to save'
              : picked == 0
                  ? 'Pick all and save'
                  : 'Save $picked file${picked == 1 ? '' : 's'}',
          hint: 'Ctrl S',
          onPressed: files.isEmpty || app.busy != null || app.picking.isNotEmpty ? null : app.saveWork,
        ),
      ]),
    );
  }
}

class _Calm extends StatelessWidget {
  const _Calm({required this.unchanged});
  final int unchanged;

  @override
  Widget build(BuildContext context) {
    final p = Palette.of(context);
    return Center(
      child: Column(mainAxisSize: MainAxisSize.min, children: [
        Icon(Icons.check_circle_outline_rounded, color: p.ok, size: 34),
        const SizedBox(height: 10),
        const Text('Everything is saved', style: TextStyle(fontWeight: FontWeight.w600)),
        const SizedBox(height: 4),
        Text(
          unchanged == 0 ? 'Add files to this folder and they\'ll show up here.' : 'Edit any of your $unchanged file${unchanged == 1 ? '' : 's'} and the change shows up here.',
          textAlign: TextAlign.center,
          style: TextStyle(color: p.muted, height: 1.4),
        ),
      ]),
    );
  }
}

class _FileRow extends StatelessWidget {
  const _FileRow({required this.app, required this.file});
  final AppState app;
  final WorkFile file;

  @override
  Widget build(BuildContext context) {
    final p = Palette.of(context);
    final busy = app.picking.contains(file.path);
    final repick = file.picked != PickedKind.none && file.state == WorkState.changed;
    final bool? ticked = repick ? null : file.picked != PickedKind.none;
    final state = switch ((file.state, file.picked)) {
      (_, PickedKind.deleted) => 'deletion picked',
      (WorkState.changed, PickedKind.none) => 'changed',
      (WorkState.changed, _) => 'changed again since you picked it',
      (WorkState.picked, PickedKind.new_) => 'new · picked',
      (WorkState.picked, _) => 'picked',
      (WorkState.missing, _) => 'missing from the folder',
      (WorkState.new_, _) => 'new · not tracked yet',
      _ => '',
    };
    final strip = app.stripFor(file);
    return Padding(
      padding: const EdgeInsets.only(bottom: 8),
      child: Material(
        color: p.raised,
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12), side: BorderSide(color: p.line)),
        child: InkWell(
          borderRadius: BorderRadius.circular(12),
          onTap: busy ? null : () => app.togglePick(file),
          child: Padding(
            padding: const EdgeInsets.fromLTRB(8, 10, 12, 12),
            child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
              Row(children: [
                SizedBox(
                  width: 36,
                  child: busy
                      ? const Center(child: SizedBox(width: 16, height: 16, child: CircularProgressIndicator(strokeWidth: 2)))
                      : Checkbox(
                          value: ticked,
                          tristate: true,
                          activeColor: p.accent,
                          onChanged: (_) => app.togglePick(file),
                        ),
                ),
                FileBadge(file.path),
                const SizedBox(width: 10),
                Expanded(
                  child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                    Text(file.path, overflow: TextOverflow.ellipsis, style: const TextStyle(fontWeight: FontWeight.w600, fontSize: 14)),
                    const SizedBox(height: 2),
                    Text(state, style: TextStyle(fontSize: 12, color: repick ? p.warn : p.muted)),
                  ]),
                ),
                if (file.size > 0) Text(fmtBytes(file.size), style: TextStyle(fontSize: 12, color: p.muted)),
                if (file.state == WorkState.changed || file.state == WorkState.missing)
                  PopupMenuButton<String>(
                    tooltip: 'More',
                    iconSize: 18,
                    icon: Icon(Icons.more_horiz_rounded, color: p.faint),
                    onSelected: (v) async {
                      if (v == 'discard') {
                        final ok = await confirm(context,
                            title: 'Bring back ${file.path}?',
                            body: 'Your copy is replaced with ${file.picked != PickedKind.none ? 'the version you picked' : 'the last save'}. The changes you haven\'t saved will be lost.',
                            action: 'Bring back',
                            danger: true);
                        if (ok) await app.bringBack(file.path);
                      }
                    },
                    itemBuilder: (_) => const [PopupMenuItem(value: 'discard', child: Text('Bring back the saved version…'))],
                  ),
              ]),
              if (file.state != WorkState.missing && file.picked != PickedKind.deleted) ...[
                const SizedBox(height: 10),
                Padding(
                  padding: const EdgeInsets.only(left: 8),
                  child: ChangeStrip(strip ?? const [], loading: strip == null || strip.isEmpty),
                ),
              ],
            ]),
          ),
        ),
      ),
    );
  }
}

class SaveInspector extends StatelessWidget {
  const SaveInspector({super.key, required this.app});
  final AppState app;

  @override
  Widget build(BuildContext context) {
    final p = Palette.of(context);
    final d = app.detail;
    if (d == null) return const Center(child: CircularProgressIndicator(strokeWidth: 2));
    final s = d.info;
    return ListView(padding: const EdgeInsets.all(22), children: [
      Eyebrow('Save · ${fmtStamp(s.time)}'),
      const SizedBox(height: 6),
      SelectableText(s.message, style: const TextStyle(fontSize: 19, fontWeight: FontWeight.w700, height: 1.3)),
      const SizedBox(height: 10),
      Wrap(spacing: 8, runSpacing: 6, crossAxisAlignment: WrapCrossAlignment.center, children: [
        Avatar(s.author),
        Text(s.author, style: TextStyle(color: p.muted, fontSize: 13)),
        Tooltip(message: s.version, child: Text(s.version.substring(0, 12), style: mono.copyWith(color: p.muted))),
        for (final l in s.labels) Chip2(l, color: p.label, background: p.labelSoft, icon: Icons.label_rounded),
        for (final dr in s.drafts) Chip2('last save on $dr', color: p.accent, background: p.accentSoft),
      ]),
      const SizedBox(height: 18),
      Eyebrow('${d.files.length} file${d.files.length == 1 ? '' : 's'}'),
      const SizedBox(height: 8),
      for (final f in d.files) _SavedFile(app: app, version: s.version, file: f),
      const SizedBox(height: 14),
      Wrap(spacing: 8, runSpacing: 8, children: [
        QuietButton(
          label: 'Start a draft here',
          icon: Icons.call_split_rounded,
          onPressed: () async {
            final name = await prompt(context, title: 'Start a draft from this save', label: 'Draft name', action: 'Start and open',
                help: 'Your files will change to match this save. Unsaved changes have to be saved first.');
            if (name != null && name.isNotEmpty) await app.startDraft(name, at: s.version);
          },
        ),
        QuietButton(
          label: 'Label…',
          icon: Icons.label_outline_rounded,
          onPressed: () async {
            final name = await prompt(context, title: 'Label this save', label: 'Label', initial: '', action: 'Label', help: 'For example final-v2 or sent-to-printer.');
            if (name != null && name.isNotEmpty) await app.addLabel(name, s.version);
          },
        ),
        QuietButton(
          label: 'Rewind to here…',
          icon: Icons.undo_rounded,
          danger: true,
          onPressed: () => _rewind(context, s),
        ),
      ]),
    ]);
  }

  Future<void> _rewind(BuildContext context, SaveInfo s) async {
    final p = Palette.of(context);
    final mode = await showDialog<RewindMode>(
      context: context,
      builder: (context) {
        Widget opt(RewindMode m, String title, String body, {bool danger = false}) => Padding(
              padding: const EdgeInsets.only(bottom: 8),
              child: Material(
                color: p.raised,
                shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12), side: BorderSide(color: danger ? p.danger.withValues(alpha: 0.5) : p.line)),
                child: InkWell(
                  borderRadius: BorderRadius.circular(12),
                  onTap: () => Navigator.pop(context, m),
                  child: Padding(
                    padding: const EdgeInsets.all(14),
                    child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                      Text(title, style: TextStyle(fontWeight: FontWeight.w600, color: danger ? p.danger : p.text)),
                      const SizedBox(height: 2),
                      Text(body, style: TextStyle(fontSize: 13, color: p.muted)),
                    ]),
                  ),
                ),
              ),
            );
        return AlertDialog(
          title: Text('Rewind “${app.space?.draft}” to this save?', style: const TextStyle(fontSize: 19, fontWeight: FontWeight.w700)),
          content: SizedBox(
            width: 480,
            child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
              Text('The saves after it leave the draft. They stay in Recover for a while.', style: TextStyle(color: p.muted, height: 1.45)),
              const SizedBox(height: 14),
              opt(RewindMode.keepFiles, 'Keep my files as they are', 'The undone saves show up as unsaved changes.'),
              opt(RewindMode.keepPicked, 'Keep them, picked', 'The undone changes come back picked, ready to save again.'),
              opt(RewindMode.discard, 'Make my files match this save', 'Unsaved work is thrown away.', danger: true),
            ]),
          ),
          actions: [TextButton(onPressed: () => Navigator.pop(context), child: Text('Cancel', style: TextStyle(color: p.muted)))],
        );
      },
    );
    if (mode == null || !context.mounted) return;
    if (mode == RewindMode.discard) {
      final ok = await confirm(context,
          title: 'Throw away unsaved work?', body: 'Your files will be replaced with this save. Anything not saved is lost.', action: 'Rewind and discard', danger: true);
      if (!ok) return;
    }
    await app.rewind(s.version, mode);
  }
}

class _SavedFile extends StatelessWidget {
  const _SavedFile({required this.app, required this.version, required this.file});
  final AppState app;
  final String version;
  final FileInSave file;

  @override
  Widget build(BuildContext context) {
    final p = Palette.of(context);
    return Padding(
      padding: const EdgeInsets.only(bottom: 8),
      child: Container(
        padding: const EdgeInsets.all(12),
        decoration: BoxDecoration(color: p.raised, borderRadius: BorderRadius.circular(12), border: Border.all(color: p.line)),
        child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          Row(children: [
            FileBadge(file.path),
            const SizedBox(width: 10),
            Expanded(
              child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                Text(file.path, overflow: TextOverflow.ellipsis, style: const TextStyle(fontWeight: FontWeight.w600)),
                const SizedBox(height: 2),
                Text('${fmtBytes(file.size)} · ${file.changed ? 'changed in this save' : 'unchanged'}', style: TextStyle(fontSize: 12, color: p.muted)),
              ]),
            ),
            PopupMenuButton<String>(
              tooltip: 'Bring back',
              icon: Icon(Icons.more_horiz_rounded, color: p.faint),
              onSelected: (v) async {
                if (v == 'copy') {
                  final dir = await pickFolder(context, title: 'Save a copy of ${file.path} into…', action: 'Save the copy here');
                  if (dir == null) return;
                  final base = file.path.split('/').last;
                  final dot = base.lastIndexOf('.');
                  final named = dot > 0 ? '${base.substring(0, dot)}-${version.substring(0, 8)}${base.substring(dot)}' : '$base-${version.substring(0, 8)}';
                  await app.bringBack(file.path, from: version, asPath: '$dir/$named');
                } else {
                  final ok = await confirm(context,
                      title: 'Bring back ${file.path} as it was here?',
                      body: 'Your copy is replaced with this version. If it has unsaved changes they\'ll be lost; “Save a copy as…” is the safe way to look.',
                      action: 'Bring back',
                      danger: true);
                  if (ok) await app.bringBack(file.path, from: version);
                }
              },
              itemBuilder: (_) => const [
                PopupMenuItem(value: 'copy', child: Text('Save a copy as…')),
                PopupMenuItem(value: 'replace', child: Text('Bring back over my copy…')),
              ],
            ),
          ]),
          if (file.changed && file.strip.isNotEmpty) ...[
            const SizedBox(height: 10),
            ChangeStrip(file.strip),
            const SizedBox(height: 6),
            StripTotals(file.strip),
          ],
        ]),
      ),
    );
  }
}
