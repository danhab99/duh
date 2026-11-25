import 'dart:io';

import 'package:flutter/material.dart';

import '../state.dart';
import 'dialogs.dart';
import 'theme.dart';
import 'widgets.dart';

/// The first screen: start a space, get a copy, or open one you've used.
class Welcome extends StatelessWidget {
  const Welcome({super.key, required this.app});
  final AppState app;

  @override
  Widget build(BuildContext context) {
    final p = Palette.of(context);
    return Scaffold(
      body: Center(
        child: SingleChildScrollView(
          padding: const EdgeInsets.all(32),
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 760),
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Row(children: [
                const _Mark(size: 44),
                const SizedBox(width: 14),
                Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                  const Text('duh Studio', style: TextStyle(fontSize: 28, fontWeight: FontWeight.w700, letterSpacing: -0.5)),
                  Text('Every version of every file, kept for you.', style: TextStyle(color: p.muted, fontSize: 15)),
                ]),
                const Spacer(),
                AppearanceButton(app: app),
              ]),
              const SizedBox(height: 36),
              LayoutBuilder(builder: (context, c) {
                final cards = [
                  _BigChoice(
                    icon: Icons.add_rounded,
                    title: 'Start a space',
                    body: 'Pick a project folder. duh will keep every version of the files in it.',
                    onTap: () async {
                      final f = await pickFolder(context, title: 'Which folder should duh look after?', action: 'Start a space here', allowNew: true);
                      if (f != null) await app.startSpace(f);
                    },
                  ),
                  _BigChoice(
                    icon: Icons.download_rounded,
                    title: 'Get a copy',
                    body: 'Copy a space from a place: a shared folder, a drive, or cloud storage.',
                    onTap: () => _getDialog(context),
                  ),
                  _BigChoice(
                    icon: Icons.folder_open_rounded,
                    title: 'Open a space',
                    body: 'A folder that already has a duh space in it.',
                    onTap: () async {
                      final f = await pickFolder(context, title: 'Open a space', action: 'Open');
                      if (f != null) await app.openSpace(f);
                    },
                  ),
                ];
                if (c.maxWidth < 620) {
                  return Column(children: [for (final w in cards) Padding(padding: const EdgeInsets.only(bottom: 12), child: w)]);
                }
                return IntrinsicHeight(
                  child: Row(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                    for (var i = 0; i < cards.length; i++) ...[if (i > 0) const SizedBox(width: 14), Expanded(child: cards[i])],
                  ]),
                );
              }),
              if (app.recent.isNotEmpty) ...[
                const SizedBox(height: 36),
                const Eyebrow('Recent spaces'),
                const SizedBox(height: 10),
                Container(
                  decoration: BoxDecoration(color: p.panel, borderRadius: BorderRadius.circular(14), border: Border.all(color: p.line)),
                  child: Column(children: [
                    for (var i = 0; i < app.recent.length; i++) ...[
                      if (i > 0) Divider(height: 1, color: p.line),
                      _RecentRow(app: app, root: app.recent[i]),
                    ],
                  ]),
                ),
              ],
              if (app.busy != null) ...[
                const SizedBox(height: 24),
                Row(children: [
                  const SizedBox(width: 16, height: 16, child: CircularProgressIndicator(strokeWidth: 2)),
                  const SizedBox(width: 10),
                  Text('${app.busy}…', style: TextStyle(color: p.muted)),
                ]),
              ],
            ]),
          ),
        ),
      ),
    );
  }

  Future<void> _getDialog(BuildContext context) async {
    final url = TextEditingController();
    final dest = TextEditingController(text: '${Platform.environment['HOME'] ?? ''}/');
    final p = Palette.of(context);
    await showDialog<void>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Get a copy', style: TextStyle(fontSize: 19, fontWeight: FontWeight.w700)),
        content: SizedBox(
          width: 480,
          child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
            Text('Where is the space? A folder, or a storage URL such as s3://bucket/path.', style: TextStyle(color: p.muted, height: 1.45)),
            const SizedBox(height: 14),
            Row(children: [
              Expanded(child: TextField(controller: url, autofocus: true, decoration: const InputDecoration(labelText: 'From'))),
              const SizedBox(width: 8),
              QuietButton(
                label: 'Browse',
                onPressed: () async {
                  final f = await pickFolder(context, title: 'Where is the space?', action: 'Use this folder');
                  if (f != null) {
                    url.text = f;
                    final name = f.split('/').where((s) => s.isNotEmpty).lastOrNull ?? 'copy';
                    dest.text = '${Platform.environment['HOME'] ?? ''}/$name';
                  }
                },
              ),
            ]),
            const SizedBox(height: 10),
            TextField(controller: dest, decoration: const InputDecoration(labelText: 'Into a new folder')),
            const SizedBox(height: 8),
            Text('Cloud storage with credentials: add it as a place from inside a space instead.', style: TextStyle(color: p.faint, fontSize: 12)),
          ]),
        ),
        actions: [
          TextButton(onPressed: () => Navigator.pop(context), child: Text('Cancel', style: TextStyle(color: p.muted))),
          FilledButton(
            onPressed: () async {
              Navigator.pop(context);
              await app.getSpace(url.text.trim(), dest.text.trim(), const []);
            },
            child: const Text('Get a copy'),
          ),
        ],
      ),
    );
  }
}

class _BigChoice extends StatelessWidget {
  const _BigChoice({required this.icon, required this.title, required this.body, required this.onTap});
  final IconData icon;
  final String title, body;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final p = Palette.of(context);
    return Material(
      color: p.panel,
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(16), side: BorderSide(color: p.line)),
      child: InkWell(
        borderRadius: BorderRadius.circular(16),
        onTap: onTap,
        child: Padding(
          padding: const EdgeInsets.all(20),
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Container(
              width: 40,
              height: 40,
              decoration: BoxDecoration(color: p.accentSoft, borderRadius: BorderRadius.circular(12)),
              child: Icon(icon, color: p.accent),
            ),
            const SizedBox(height: 14),
            Text(title, style: const TextStyle(fontSize: 16, fontWeight: FontWeight.w700)),
            const SizedBox(height: 6),
            Text(body, style: TextStyle(color: p.muted, height: 1.45, fontSize: 13)),
          ]),
        ),
      ),
    );
  }
}

class _RecentRow extends StatelessWidget {
  const _RecentRow({required this.app, required this.root});
  final AppState app;
  final String root;

  @override
  Widget build(BuildContext context) {
    final p = Palette.of(context);
    final exists = Directory('$root/.duh').existsSync();
    final name = root.split('/').where((s) => s.isNotEmpty).lastOrNull ?? root;
    return ListTile(
      leading: Icon(Icons.auto_awesome_motion_rounded, color: exists ? p.accent : p.faint),
      title: Text(name, style: const TextStyle(fontWeight: FontWeight.w600)),
      subtitle: Text(exists ? root : '$root · not found', style: TextStyle(color: p.muted, fontSize: 12)),
      trailing: IconButton(tooltip: 'Remove from this list', icon: Icon(Icons.close_rounded, size: 18, color: p.faint), onPressed: () => app.forgetRecent(root)),
      onTap: exists ? () => app.openSpace(root) : null,
    );
  }
}

/// duh's mark: a violet tile with an M-ish stroke.
class _Mark extends StatelessWidget {
  const _Mark({this.size = 28});
  final double size;
  @override
  Widget build(BuildContext context) => StudioMark(size: size);
}

class StudioMark extends StatelessWidget {
  const StudioMark({super.key, this.size = 28});
  final double size;

  @override
  Widget build(BuildContext context) {
    final p = Palette.of(context);
    return Container(
      width: size,
      height: size,
      decoration: BoxDecoration(color: p.accent, borderRadius: BorderRadius.circular(size * 0.28)),
      child: CustomPaint(painter: _MarkPainter(p.onAccent)),
    );
  }
}

class _MarkPainter extends CustomPainter {
  _MarkPainter(this.color);
  final Color color;

  @override
  void paint(Canvas canvas, Size s) {
    final paint = Paint()
      ..color = color
      ..style = PaintingStyle.stroke
      ..strokeWidth = s.width * 0.11
      ..strokeCap = StrokeCap.round
      ..strokeJoin = StrokeJoin.round;
    final path = Path()
      ..moveTo(s.width * 0.22, s.height * 0.72)
      ..lineTo(s.width * 0.22, s.height * 0.28)
      ..lineTo(s.width * 0.5, s.height * 0.52)
      ..lineTo(s.width * 0.78, s.height * 0.28)
      ..lineTo(s.width * 0.78, s.height * 0.72);
    canvas.drawPath(path, paint);
  }

  @override
  bool shouldRepaint(_MarkPainter old) => old.color != color;
}
