import 'package:flutter/material.dart';

import '../engine.dart';
import '../state.dart';
import 'dialogs.dart';
import 'inspector.dart';
import 'theme.dart';
import 'welcome.dart';
import 'widgets.dart';

/// The window for an open space: top bar, sidebar, timeline, inspector.
class SpaceScreen extends StatelessWidget {
  const SpaceScreen({super.key, required this.app, required this.onPalette});
  final AppState app;
  final VoidCallback onPalette;

  @override
  Widget build(BuildContext context) {
    final p = Palette.of(context);
    return Scaffold(
      body: Column(children: [
        TopBar(app: app, onPalette: onPalette),
        if (app.busy != null) LinearProgressIndicator(minHeight: 2, color: p.accent, backgroundColor: p.ground),
        Expanded(
          child: LayoutBuilder(builder: (context, c) {
            final narrow = c.maxWidth < 1000;
            return Row(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
              if (!narrow) SizedBox(width: 236, child: Sidebar(app: app)),
              Expanded(child: Timeline(app: app)),
              SizedBox(width: narrow ? 360 : 420, child: Inspector(app: app)),
            ]);
          }),
        ),
      ]),
    );
  }
}

class TopBar extends StatelessWidget {
  const TopBar({super.key, required this.app, required this.onPalette});
  final AppState app;
  final VoidCallback onPalette;

  @override
  Widget build(BuildContext context) {
    final p = Palette.of(context);
    return Container(
      height: 60,
      padding: const EdgeInsets.symmetric(horizontal: 16),
      decoration: BoxDecoration(color: p.ground, border: Border(bottom: BorderSide(color: p.line))),
      child: Row(children: [
        Tooltip(
          message: 'All spaces',
          child: InkWell(
            borderRadius: BorderRadius.circular(8),
            onTap: app.closeSpace,
            child: const Padding(padding: EdgeInsets.all(2), child: StudioMark(size: 28)),
          ),
        ),
        const SizedBox(width: 10),
        Flexible(
          child: Text(app.space?.name ?? '', overflow: TextOverflow.ellipsis, style: const TextStyle(fontWeight: FontWeight.w700, fontSize: 16, letterSpacing: -0.2)),
        ),
        const SizedBox(width: 14),
        _DraftSwitcher(app: app),
        const Spacer(),
        _SyncPill(app: app),
        const SizedBox(width: 10),
        _HealthBadge(app: app),
        const SizedBox(width: 4),
        AppearanceButton(app: app),
        const SizedBox(width: 4),
        _PaletteButton(onTap: onPalette),
      ]),
    );
  }
}

class _DraftSwitcher extends StatelessWidget {
  const _DraftSwitcher({required this.app});
  final AppState app;

  @override
  Widget build(BuildContext context) {
    final p = Palette.of(context);
    return PopupMenuButton<String>(
      tooltip: 'Drafts',
      position: PopupMenuPosition.under,
      onSelected: (v) async {
        if (v == '+new') {
          final name = await prompt(context, title: 'Start a new draft', label: 'Name', action: 'Start and open', help: 'It starts from your last save. Your files stay as they are.');
          if (name != null && name.isNotEmpty) await app.startDraft(name);
        } else {
          await _openDraft(context, app, v);
        }
      },
      itemBuilder: (context) => [
        for (final d in app.drafts)
          PopupMenuItem(
            value: d.name,
            enabled: !d.open,
            child: Row(children: [
              Icon(d.open ? Icons.radio_button_checked : Icons.radio_button_unchecked, size: 16, color: d.open ? p.accent : p.faint),
              const SizedBox(width: 10),
              Text(d.name),
            ]),
          ),
        const PopupMenuDivider(),
        const PopupMenuItem(value: '+new', child: Row(children: [Icon(Icons.add_rounded, size: 16), SizedBox(width: 10), Text('New draft…')])),
      ],
      child: Container(
        height: 36,
        padding: const EdgeInsets.symmetric(horizontal: 14),
        decoration: BoxDecoration(color: p.raised, borderRadius: BorderRadius.circular(18), border: Border.all(color: p.line)),
        child: Row(mainAxisSize: MainAxisSize.min, children: [
          Icon(Icons.call_split_rounded, size: 16, color: p.muted),
          const SizedBox(width: 8),
          Text('Draft: ${app.space?.draft ?? 'none'}', style: const TextStyle(fontSize: 13, fontWeight: FontWeight.w500)),
          const SizedBox(width: 4),
          Icon(Icons.expand_more_rounded, size: 18, color: p.muted),
        ]),
      ),
    );
  }
}

Future<void> _openDraft(BuildContext context, AppState app, String name) async {
  if ((app.look?.files ?? []).any((f) => f.state != WorkState.unchanged && f.state != WorkState.new_ || f.picked != PickedKind.none)) {
    await confirm(
      context,
      title: 'Save or bring back your files first',
      body: 'You have unsaved changes. Opening “$name” would replace your files, so duh won\'t do it until they\'re saved (or brought back as they were).',
      action: 'OK',
    );
    return;
  }
  await app.openDraft(name);
}

class _SyncPill extends StatelessWidget {
  const _SyncPill({required this.app});
  final AppState app;

  @override
  Widget build(BuildContext context) {
    final p = Palette.of(context);
    final place = app.primaryPlace;
    if (place == null) {
      return QuietButton(label: 'Add a place to share', icon: Icons.cloud_outlined, onPressed: () => addPlaceDialog(context, app));
    }
    final s = app.sync;
    final diverged = s?.diverged ?? false;
    final String text;
    final Color color;
    if (app.syncError != null) {
      text = 'can\'t reach ${place.name}';
      color = p.danger;
    } else if (s == null) {
      text = 'checking ${place.name}…';
      color = p.muted;
    } else if (diverged) {
      text = '↑${s.toSend} ↓${s.toReceive} · diverged from ${place.name}';
      color = p.warn;
    } else if (s.toSend > 0 || s.toReceive > 0) {
      text = [if (s.toSend > 0) '↑ ${s.toSend} to send', if (s.toReceive > 0) '↓ ${s.toReceive} to receive'].join(' · ');
      color = p.sync;
    } else {
      text = 'up to date with ${place.name}';
      color = p.muted;
    }
    return Tooltip(
      message: app.syncError ?? place.url,
      child: Container(
        height: 36,
        padding: const EdgeInsets.only(left: 14, right: 4),
        decoration: BoxDecoration(
          color: diverged ? p.warnSoft : p.raised,
          borderRadius: BorderRadius.circular(18),
          border: Border.all(color: diverged ? p.warn.withValues(alpha: 0.6) : p.line),
        ),
        child: Row(mainAxisSize: MainAxisSize.min, children: [
          Text(text, style: TextStyle(fontSize: 13, fontWeight: FontWeight.w600, color: color)),
          const SizedBox(width: 8),
          if (diverged)
            _pillButton(context, 'Choose…', p.warn, () => divergedSheet(context, app))
          else ...[
            _pillButton(context, 'Send', (s?.toSend ?? 0) > 0 ? p.sync : null, app.busy == null ? () => app.send() : null),
            _pillButton(context, 'Receive', (s?.toReceive ?? 0) > 0 ? p.sync : null, app.busy == null ? () => _receive(context) : null),
          ],
          IconButton(
            tooltip: 'Check again',
            visualDensity: VisualDensity.compact,
            iconSize: 16,
            onPressed: app.refreshSync,
            icon: Icon(Icons.refresh_rounded, color: p.muted),
          ),
        ]),
      ),
    );
  }

  Future<void> _receive(BuildContext context) async {
    final dirty = app.workFiles.any((f) => f.state != WorkState.new_);
    if (dirty) {
      await confirm(context,
          title: 'Save first',
          body: 'Receiving updates your files to match ${app.primaryPlace?.name}. Save (or bring back) your changes first so nothing is lost.',
          action: 'OK');
      return;
    }
    await app.receive();
  }

  Widget _pillButton(BuildContext context, String label, Color? strong, VoidCallback? onTap) {
    final p = Palette.of(context);
    return Padding(
      padding: const EdgeInsets.only(right: 2),
      child: TextButton(
        onPressed: onTap,
        style: TextButton.styleFrom(
          minimumSize: const Size(0, 28),
          padding: const EdgeInsets.symmetric(horizontal: 12),
          backgroundColor: strong?.withValues(alpha: 0.18),
          foregroundColor: strong ?? p.text,
          shape: const StadiumBorder(),
          textStyle: const TextStyle(fontSize: 13, fontWeight: FontWeight.w600, fontFamily: 'Inter'),
        ),
        child: Text(label),
      ),
    );
  }
}

class _HealthBadge extends StatelessWidget {
  const _HealthBadge({required this.app});
  final AppState app;

  @override
  Widget build(BuildContext context) {
    final p = Palette.of(context);
    final h = app.health;
    final (dot, text) = switch (h) {
      null => (p.faint, 'Check'),
      _ when h.problems.isEmpty => (p.ok, 'Intact · checked ${_ago(app.checkedAt!)}'),
      _ => (p.danger, count(h.problems.length, 'problem')),
    };
    return Tooltip(
      message: 'Check that every save reads back exactly',
      child: OutlinedButton(
        onPressed: app.busy == null
            ? () async {
                final r = await app.check();
                if (r != null && context.mounted) await showCheck(context, r);
              }
            : null,
        style: OutlinedButton.styleFrom(
          minimumSize: const Size(0, 36),
          side: BorderSide(color: p.line),
          backgroundColor: p.raised,
          foregroundColor: p.text,
          shape: const StadiumBorder(),
          padding: const EdgeInsets.symmetric(horizontal: 14),
        ),
        child: Row(mainAxisSize: MainAxisSize.min, children: [
          Container(width: 8, height: 8, decoration: BoxDecoration(color: dot, shape: BoxShape.circle)),
          const SizedBox(width: 8),
          Text(text, style: const TextStyle(fontSize: 13)),
        ]),
      ),
    );
  }

  static String _ago(DateTime t) {
    final d = DateTime.now().difference(t);
    if (d.inMinutes < 1) return 'just now';
    if (d.inMinutes < 60) return '${d.inMinutes} min ago';
    return '${d.inHours} h ago';
  }
}

class _PaletteButton extends StatelessWidget {
  const _PaletteButton({required this.onTap});
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final p = Palette.of(context);
    return InkWell(
      borderRadius: BorderRadius.circular(10),
      onTap: onTap,
      child: Container(
        height: 36,
        width: 210,
        padding: const EdgeInsets.symmetric(horizontal: 12),
        decoration: BoxDecoration(color: p.raised, borderRadius: BorderRadius.circular(10), border: Border.all(color: p.line)),
        child: Row(children: [
          Icon(Icons.search_rounded, size: 16, color: p.faint),
          const SizedBox(width: 8),
          Expanded(child: Text('Search or run…', style: TextStyle(color: p.faint, fontSize: 13))),
          const KeyHint('Ctrl K'),
        ]),
      ),
    );
  }
}

class Sidebar extends StatelessWidget {
  const Sidebar({super.key, required this.app});
  final AppState app;

  @override
  Widget build(BuildContext context) {
    final p = Palette.of(context);
    return Container(
      decoration: BoxDecoration(color: p.sidebar, border: Border(right: BorderSide(color: p.line))),
      child: ListView(padding: const EdgeInsets.fromLTRB(12, 18, 12, 18), children: [
        _Section(title: 'Spaces', children: [
          for (final r in app.recent.take(6))
            _Item(
              name: r.split('/').where((s) => s.isNotEmpty).lastOrNull ?? r,
              active: r == app.folder,
              dot: r == app.folder ? p.accent : p.kept,
              onTap: r == app.folder ? null : () => app.openSpace(r),
            ),
          _Item(name: 'All spaces…', icon: Icons.grid_view_rounded, onTap: app.closeSpace),
        ]),
        _Section(
          title: 'Drafts',
          onAdd: () async {
            final name = await prompt(context, title: 'Start a new draft', label: 'Name', action: 'Start and open');
            if (name != null && name.isNotEmpty) await app.startDraft(name);
          },
          children: [
            for (final d in app.drafts)
              _Item(
                name: d.name,
                active: d.open,
                dot: d.open ? p.added : p.kept,
                onTap: d.open ? null : () => _openDraft(context, app, d.name),
                menu: [
                  if (d.open)
                    ('Rename…', () async {
                      final n = await prompt(context, title: 'Rename this draft', label: 'New name', initial: d.name, action: 'Rename');
                      if (n != null && n.isNotEmpty) await app.renameDraft(n);
                    }),
                  if (!d.open)
                    ('Delete…', () async {
                      final ok = await confirm(context,
                          title: 'Delete the draft “${d.name}”?',
                          body: 'Its saves stay in the journal for a while, but the draft itself will be gone.',
                          action: 'Delete',
                          danger: true);
                      if (ok) await app.deleteDraft(d.name);
                    }),
                ],
              ),
          ],
        ),
        _Section(title: 'Labels', children: [
          if (app.labels.isEmpty) _Empty('Name a save from its menu'),
          for (final l in app.labels)
            _Item(
              name: l.name,
              dot: p.label,
              onTap: () => app.select(SaveSelected(l.version)),
              menu: [
                ('Remove label', () => app.removeLabel(l.name)),
              ],
            ),
        ]),
        _Section(
          title: 'Places',
          onAdd: () => addPlaceDialog(context, app),
          children: [
            if (app.places.isEmpty) _Empty('Share or back up your saves'),
            for (final pl in app.places)
              _Item(
                name: pl.name,
                meta: _kind(pl.url),
                dot: p.sync,
                menu: [
                  ('Check this place', () async {
                    final r = await app.check(place: pl.name);
                    if (r != null && context.mounted) await showCheck(context, r, place: pl.name);
                  }),
                  ('Remove…', () async {
                    final ok = await confirm(context,
                        title: 'Remove the place “${pl.name}”?',
                        body: 'This space will stop sending to it. Nothing stored there is deleted.',
                        action: 'Remove');
                    if (ok) await app.removePlace(pl.name);
                  }),
                ],
              ),
          ],
        ),
        const SizedBox(height: 8),
        _Item(name: 'Recover lost work…', icon: Icons.history_rounded, onTap: () => showJournal(context, app)),
      ]),
    );
  }

  static String _kind(String url) {
    final i = url.indexOf('://');
    if (i > 0 && !url.startsWith('file://')) return url.substring(0, i).toUpperCase();
    return 'folder';
  }
}

class _Section extends StatelessWidget {
  const _Section({required this.title, required this.children, this.onAdd});
  final String title;
  final List<Widget> children;
  final VoidCallback? onAdd;

  @override
  Widget build(BuildContext context) {
    final p = Palette.of(context);
    return Padding(
      padding: const EdgeInsets.only(bottom: 20),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(10, 0, 0, 6),
          child: Row(children: [
            Expanded(child: Eyebrow(title)),
            if (onAdd != null)
              IconButton(
                tooltip: 'Add',
                onPressed: onAdd,
                visualDensity: VisualDensity.compact,
                iconSize: 16,
                icon: Icon(Icons.add_rounded, color: p.faint),
              ),
          ]),
        ),
        ...children,
      ]),
    );
  }
}

class _Empty extends StatelessWidget {
  const _Empty(this.text);
  final String text;
  @override
  Widget build(BuildContext context) => Padding(
        padding: const EdgeInsets.fromLTRB(10, 2, 10, 4),
        child: Text(text, style: TextStyle(color: Palette.of(context).faint, fontSize: 12)),
      );
}

class _Item extends StatelessWidget {
  const _Item({required this.name, this.meta, this.dot, this.icon, this.active = false, this.onTap, this.menu = const []});
  final String name;
  final String? meta;
  final Color? dot;
  final IconData? icon;
  final bool active;
  final VoidCallback? onTap;
  final List<(String, VoidCallback)> menu;

  @override
  Widget build(BuildContext context) {
    final p = Palette.of(context);
    final row = Container(
      height: 34,
      padding: const EdgeInsets.symmetric(horizontal: 10),
      decoration: BoxDecoration(color: active ? p.raised : null, borderRadius: BorderRadius.circular(8)),
      child: Row(children: [
        if (icon != null) Icon(icon, size: 15, color: p.muted) else Container(width: 8, height: 8, decoration: BoxDecoration(color: dot ?? p.kept, shape: BoxShape.circle)),
        const SizedBox(width: 10),
        Expanded(
          child: Text(name,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(fontSize: 13, fontWeight: active ? FontWeight.w600 : FontWeight.w400, color: active ? p.text : (icon != null ? p.muted : p.text))),
        ),
        if (meta != null) Text(meta!, style: TextStyle(fontSize: 11, color: p.faint)),
        if (menu.isNotEmpty)
          SizedBox(
            width: 26,
            child: PopupMenuButton<int>(
              tooltip: 'More',
              padding: EdgeInsets.zero,
              iconSize: 16,
              icon: Icon(Icons.more_horiz_rounded, color: p.faint),
              onSelected: (i) => menu[i].$2(),
              itemBuilder: (_) => [for (var i = 0; i < menu.length; i++) PopupMenuItem(value: i, child: Text(menu[i].$1))],
            ),
          ),
      ]),
    );
    return InkWell(borderRadius: BorderRadius.circular(8), onTap: onTap, child: row);
  }
}

class Timeline extends StatelessWidget {
  const Timeline({super.key, required this.app});
  final AppState app;

  @override
  Widget build(BuildContext context) {
    final p = Palette.of(context);
    final work = app.workFiles;
    final saves = app.history;
    return ListView(
      padding: const EdgeInsets.fromLTRB(28, 22, 28, 28),
      children: [
        Row(crossAxisAlignment: CrossAxisAlignment.baseline, textBaseline: TextBaseline.alphabetic, children: [
          const Text('Timeline', style: TextStyle(fontSize: 20, fontWeight: FontWeight.w700, letterSpacing: -0.2)),
          const SizedBox(width: 10),
          Text('${app.space?.draft ?? 'no draft'} · ${saves.length} save${saves.length == 1 ? '' : 's'}', style: TextStyle(color: p.faint, fontSize: 13)),
        ]),
        const SizedBox(height: 14),
        _WorkCard(app: app, files: work),
        const SizedBox(height: 10),
        if (saves.isEmpty)
          Padding(
            padding: const EdgeInsets.symmetric(vertical: 36),
            child: Column(children: [
              Icon(Icons.auto_awesome_rounded, color: p.faint, size: 28),
              const SizedBox(height: 10),
              Text('No saves yet', style: TextStyle(color: p.text, fontWeight: FontWeight.w600)),
              const SizedBox(height: 4),
              Text('Pick your files on the right and save the first version.', style: TextStyle(color: p.muted)),
            ]),
          )
        else
          Stack(children: [
            Positioned(left: 23, top: 22, bottom: 22, child: Container(width: 2, color: p.line)),
            Column(children: [for (final s in saves) _SaveRow(app: app, save: s)]),
          ]),
      ],
    );
  }
}

class _WorkCard extends StatelessWidget {
  const _WorkCard({required this.app, required this.files});
  final AppState app;
  final List<WorkFile> files;

  @override
  Widget build(BuildContext context) {
    final p = Palette.of(context);
    final selected = app.selection is WorkSelected;
    final segments = <Segment>[];
    for (final f in files) {
      final s = app.stripFor(f);
      if (s != null) segments.addAll(s);
    }
    final summary = files.isEmpty
        ? 'nothing changed'
        : '${files.length} changed · ${app.pickedCount > 0 ? '${app.pickedCount} picked' : 'not saved'}';
    return AnimatedContainer(
      duration: motion(context, 180),
      curve: Curves.easeOut,
      decoration: BoxDecoration(
        color: selected ? p.accentSoft : p.panel,
        borderRadius: BorderRadius.circular(14),
        border: Border.all(color: selected ? p.accent : p.line, width: selected ? 1.5 : 1),
        boxShadow: selected ? [BoxShadow(color: p.accent.withValues(alpha: 0.15), spreadRadius: 4)] : null,
      ),
      child: InkWell(
        borderRadius: BorderRadius.circular(14),
        onTap: () => app.select(const WorkSelected()),
        child: Padding(
          padding: const EdgeInsets.all(16),
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Row(children: [
              Container(width: 10, height: 10, decoration: BoxDecoration(color: files.isEmpty ? p.kept : p.warn, shape: BoxShape.circle)),
              const SizedBox(width: 10),
              const Text('Your files', style: TextStyle(fontWeight: FontWeight.w600, fontSize: 15)),
              const SizedBox(width: 10),
              Text(summary, style: TextStyle(color: files.isEmpty ? p.muted : p.accent, fontSize: 13)),
            ]),
            if (files.isNotEmpty && segments.isNotEmpty) ...[
              const SizedBox(height: 12),
              ChangeStrip(segments, height: 8),
              const SizedBox(height: 8),
              StripTotals(segments),
            ],
          ]),
        ),
      ),
    );
  }
}

class _SaveRow extends StatelessWidget {
  const _SaveRow({required this.app, required this.save});
  final AppState app;
  final SaveInfo save;

  @override
  Widget build(BuildContext context) {
    final p = Palette.of(context);
    final selected = app.selection is SaveSelected && (app.selection as SaveSelected).version == save.version;
    final unsent = app.isUnsent(save);
    final dotColor = save.labels.isNotEmpty ? p.label : (unsent ? p.sync : p.faint);
    final total = save.added + save.removed;
    return AnimatedContainer(
      duration: motion(context, 150),
      margin: const EdgeInsets.only(bottom: 2),
      decoration: BoxDecoration(color: selected ? p.accentSoft : null, borderRadius: BorderRadius.circular(12), border: Border.all(color: selected ? p.accent : Colors.transparent)),
      child: InkWell(
        borderRadius: BorderRadius.circular(12),
        onTap: () => app.select(SaveSelected(save.version)),
        child: Padding(
          padding: const EdgeInsets.fromLTRB(17, 11, 12, 11),
          child: Row(children: [
            Container(
              width: 12,
              height: 12,
              decoration: BoxDecoration(color: dotColor, shape: BoxShape.circle, border: Border.all(color: p.ground, width: 2)),
            ),
            const SizedBox(width: 14),
            Expanded(
              child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                Wrap(spacing: 8, runSpacing: 4, crossAxisAlignment: WrapCrossAlignment.center, children: [
                  Text(save.message.split('\n').first, style: const TextStyle(fontWeight: FontWeight.w600, fontSize: 14)),
                  for (final l in save.labels) Chip2(l, color: p.label, background: p.labelSoft, icon: Icons.label_rounded),
                  if (unsent) Chip2('not sent', color: p.sync, background: p.syncSoft),
                ]),
                const SizedBox(height: 6),
                Row(children: [
                  SizedBox(
                    width: 140,
                    child: ChangeStrip(
                      total == 0
                          ? const []
                          : [
                              Segment(kind: SegmentKind.added, len: save.added),
                              if (save.removed > 0) Segment(kind: SegmentKind.removed, len: save.removed),
                            ],
                      height: 4,
                    ),
                  ),
                  const SizedBox(width: 10),
                  Text(
                    '${save.filesChanged} file${save.filesChanged == 1 ? '' : 's'} · +${fmtBytes(save.added)}${save.removed > 0 ? ' −${fmtBytes(save.removed)}' : ''}',
                    style: TextStyle(fontSize: 12, color: p.muted),
                  ),
                ]),
              ]),
            ),
            Column(crossAxisAlignment: CrossAxisAlignment.end, children: [
              Text(fmtWhen(save.time), style: TextStyle(fontSize: 12, color: p.muted)),
              const SizedBox(height: 4),
              Avatar(save.author),
            ]),
          ]),
        ),
      ),
    );
  }
}
