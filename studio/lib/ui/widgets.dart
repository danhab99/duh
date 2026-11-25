import 'package:flutter/material.dart';

import '../engine.dart';
import 'theme.dart';

/// A change strip: where a file's bytes were kept, added and removed, each
/// run as wide as its share of the file. Tiny runs still get a sliver, so a
/// one-byte edit in a big file stays visible.
class ChangeStrip extends StatelessWidget {
  const ChangeStrip(this.segments, {super.key, this.height = 6, this.loading = false});

  final List<Segment> segments;
  final double height;
  final bool loading;

  @override
  Widget build(BuildContext context) {
    final p = Palette.of(context);
    final total = segments.fold<int>(0, (a, s) => a + s.len);
    return Semantics(
      label: _describe(),
      child: ClipRRect(
        borderRadius: BorderRadius.circular(height / 2),
        child: SizedBox(
          height: height,
          child: LayoutBuilder(builder: (context, c) {
            if (loading || total == 0) {
              return Container(color: p.kept.withValues(alpha: loading ? 0.5 : 1));
            }
            final gap = segments.length > 1 ? 1.0 : 0.0;
            final usable = c.maxWidth - gap * (segments.length - 1);
            const minW = 2.0;
            final widths = segments.map((s) => s.kind == SegmentKind.kept ? usable * s.len / total : (usable * s.len / total).clamp(minW, usable)).toList();
            final over = widths.fold<double>(0, (a, w) => a + w) - usable;
            if (over > 0) {
              // Take the slivers' extra width from the kept runs.
              final kept = segments.where((s) => s.kind == SegmentKind.kept).fold<int>(0, (a, s) => a + s.len);
              for (var i = 0; i < widths.length; i++) {
                if (segments[i].kind == SegmentKind.kept && kept > 0) {
                  widths[i] = (widths[i] - over * segments[i].len / kept).clamp(0, usable);
                }
              }
            }
            return Row(children: [
              for (var i = 0; i < segments.length; i++) ...[
                if (i > 0) SizedBox(width: gap),
                Container(width: widths[i], color: _color(p, segments[i].kind)),
              ],
            ]);
          }),
        ),
      ),
    );
  }

  static Color _color(Palette p, SegmentKind k) => switch (k) {
        SegmentKind.kept => p.kept,
        SegmentKind.added => p.added,
        SegmentKind.removed => p.removed,
      };

  String _describe() {
    final a = segments.where((s) => s.kind == SegmentKind.added).fold<int>(0, (x, s) => x + s.len);
    final r = segments.where((s) => s.kind == SegmentKind.removed).fold<int>(0, (x, s) => x + s.len);
    return 'Change strip: ${fmtBytes(a)} added, ${fmtBytes(r)} removed';
  }
}

/// "+1.2 MB  −40 KB  38 MB kept" under a strip.
class StripTotals extends StatelessWidget {
  const StripTotals(this.segments, {super.key});
  final List<Segment> segments;

  @override
  Widget build(BuildContext context) {
    final p = Palette.of(context);
    int sum(SegmentKind k) => segments.where((s) => s.kind == k).fold<int>(0, (a, s) => a + s.len);
    final a = sum(SegmentKind.added), r = sum(SegmentKind.removed), k = sum(SegmentKind.kept);
    return Wrap(spacing: 14, children: [
      if (a > 0) Text('+${fmtBytes(a)} added', style: TextStyle(color: p.added, fontSize: 12)),
      if (r > 0) Text('−${fmtBytes(r)} removed', style: TextStyle(color: p.removed, fontSize: 12)),
      if (k > 0) Text('${fmtBytes(k)} kept', style: TextStyle(color: p.muted, fontSize: 12)),
    ]);
  }
}

/// The primary action: violet, large.
class PrimaryButton extends StatelessWidget {
  const PrimaryButton({super.key, required this.label, this.onPressed, this.hint, this.icon});
  final String label;
  final String? hint;
  final IconData? icon;
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    final p = Palette.of(context);
    return SizedBox(
      height: 46,
      child: FilledButton(
        onPressed: onPressed,
        style: FilledButton.styleFrom(
          backgroundColor: p.accent,
          foregroundColor: p.onAccent,
          disabledBackgroundColor: p.kept,
          shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
          textStyle: const TextStyle(fontSize: 15, fontWeight: FontWeight.w600, fontFamily: 'Inter'),
        ),
        child: Row(mainAxisAlignment: MainAxisAlignment.center, mainAxisSize: MainAxisSize.min, children: [
          if (icon != null) ...[Icon(icon, size: 18), const SizedBox(width: 8)],
          Flexible(child: Text(label, overflow: TextOverflow.ellipsis, maxLines: 1)),
          if (hint != null) ...[const SizedBox(width: 10), KeyHint(hint!, onAccent: true)],
        ]),
      ),
    );
  }
}

/// A quiet bordered button.
class QuietButton extends StatelessWidget {
  const QuietButton({super.key, required this.label, this.onPressed, this.icon, this.danger = false, this.dense = false});
  final String label;
  final IconData? icon;
  final VoidCallback? onPressed;
  final bool danger;
  final bool dense;

  @override
  Widget build(BuildContext context) {
    final p = Palette.of(context);
    return SizedBox(
      height: dense ? 32 : 38,
      child: OutlinedButton(
        onPressed: onPressed,
        style: OutlinedButton.styleFrom(
          foregroundColor: danger ? p.danger : p.text,
          side: BorderSide(color: p.line),
          backgroundColor: p.panel,
          padding: EdgeInsets.symmetric(horizontal: dense ? 10 : 14),
          shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(10)),
          textStyle: TextStyle(fontSize: dense ? 12 : 13, fontWeight: FontWeight.w500, fontFamily: 'Inter'),
        ),
        child: Row(mainAxisSize: MainAxisSize.min, children: [
          if (icon != null) ...[Icon(icon, size: dense ? 14 : 16), const SizedBox(width: 6)],
          Text(label),
        ]),
      ),
    );
  }
}

/// A keyboard shortcut, e.g. "Ctrl S".
class KeyHint extends StatelessWidget {
  const KeyHint(this.keys, {super.key, this.onAccent = false});
  final String keys;
  final bool onAccent;

  @override
  Widget build(BuildContext context) {
    final p = Palette.of(context);
    final c = onAccent ? p.onAccent.withValues(alpha: 0.85) : p.muted;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 5, vertical: 1),
      decoration: BoxDecoration(
        border: Border.all(color: onAccent ? p.onAccent.withValues(alpha: 0.35) : p.line),
        borderRadius: BorderRadius.circular(5),
      ),
      child: Text(keys, style: mono.copyWith(fontSize: 11, color: c)),
    );
  }
}

/// A small coloured tag: a label, "not sent", "picked".
class Chip2 extends StatelessWidget {
  const Chip2(this.text, {super.key, required this.color, required this.background, this.icon});
  final String text;
  final Color color, background;
  final IconData? icon;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 2),
      decoration: BoxDecoration(color: background, borderRadius: BorderRadius.circular(6)),
      child: Row(mainAxisSize: MainAxisSize.min, children: [
        if (icon != null) ...[Icon(icon, size: 11, color: color), const SizedBox(width: 3)],
        Text(text, style: TextStyle(color: color, fontSize: 11, fontWeight: FontWeight.w600)),
      ]),
    );
  }
}

/// A round initial for who saved something.
class Avatar extends StatelessWidget {
  const Avatar(this.name, {super.key, this.size = 22});
  final String name;
  final double size;

  @override
  Widget build(BuildContext context) {
    final p = Palette.of(context);
    final initial = name.trim().isEmpty ? '?' : name.trim()[0].toUpperCase();
    return Container(
      width: size,
      height: size,
      alignment: Alignment.center,
      decoration: BoxDecoration(color: p.accentSoft, shape: BoxShape.circle, border: Border.all(color: p.line)),
      child: Text(initial, style: TextStyle(fontSize: size * 0.48, fontWeight: FontWeight.w700, color: p.text)),
    );
  }
}

/// A file-type badge: "CAD" for FreeCAD, "XCF" for GIMP, and so on.
class FileBadge extends StatelessWidget {
  const FileBadge(this.path, {super.key});
  final String path;

  static (String, Color) kind(String path) {
    final ext = path.contains('.') ? path.split('.').last.toLowerCase() : '';
    return switch (ext) {
      'fcstd' || 'step' || 'stp' || 'stl' || 'blend' || 'obj' || '3mf' => ('CAD', const Color(0xFFA78BFA)),
      'xcf' || 'psd' || 'kra' || 'ora' => ('XCF', const Color(0xFFF9A8D4)),
      'png' || 'jpg' || 'jpeg' || 'webp' || 'gif' || 'tif' || 'tiff' => ('IMG', const Color(0xFF67E8F9)),
      'svg' || 'ai' || 'pdf' => ('VEC', const Color(0xFFFCD34D)),
      'wav' || 'flac' || 'mp3' || 'ogg' => ('AUD', const Color(0xFF6EE7B7)),
      'mp4' || 'mov' || 'mkv' || 'webm' => ('VID', const Color(0xFFFDBA74)),
      '' => ('FILE', const Color(0xFFCBD5E1)),
      _ => (ext.length <= 4 ? ext.toUpperCase() : 'FILE', const Color(0xFFCBD5E1)),
    };
  }

  @override
  Widget build(BuildContext context) {
    final (text, color) = kind(path);
    return Container(
      width: 32,
      height: 32,
      alignment: Alignment.center,
      decoration: BoxDecoration(color: color, borderRadius: BorderRadius.circular(8)),
      child: Text(text, style: const TextStyle(color: Color(0xFF0E1116), fontSize: 9.5, fontWeight: FontWeight.w700)),
    );
  }
}

/// A section heading in small caps.
class Eyebrow extends StatelessWidget {
  const Eyebrow(this.text, {super.key});
  final String text;
  @override
  Widget build(BuildContext context) => Text(
        text.toUpperCase(),
        style: TextStyle(fontSize: 11, fontWeight: FontWeight.w600, letterSpacing: 0.9, color: Palette.of(context).faint),
      );
}
