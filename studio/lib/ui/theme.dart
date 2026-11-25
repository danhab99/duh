import 'package:flutter/material.dart';

/// duh Studio's palette. One set per brightness; widgets read it with
/// `Palette.of(context)`.
@immutable
class Palette extends ThemeExtension<Palette> {
  const Palette({
    required this.ground,
    required this.sidebar,
    required this.panel,
    required this.raised,
    required this.line,
    required this.text,
    required this.muted,
    required this.faint,
    required this.accent,
    required this.onAccent,
    required this.accentSoft,
    required this.sync,
    required this.syncSoft,
    required this.added,
    required this.removed,
    required this.kept,
    required this.warn,
    required this.warnSoft,
    required this.danger,
    required this.label,
    required this.labelSoft,
    required this.ok,
  });

  final Color ground, sidebar, panel, raised, line;
  final Color text, muted, faint;
  final Color accent, onAccent, accentSoft;
  final Color sync, syncSoft;
  final Color added, removed, kept;
  final Color warn, warnSoft, danger;
  final Color label, labelSoft, ok;

  static const dark = Palette(
    ground: Color(0xFF0E1116),
    sidebar: Color(0xFF0B0E13),
    panel: Color(0xFF11151C),
    raised: Color(0xFF161B22),
    line: Color(0xFF222937),
    text: Color(0xFFE6EAF2),
    muted: Color(0xFF9AA4B5),
    faint: Color(0xFF8A94A6),
    accent: Color(0xFF7C3AED),
    onAccent: Color(0xFFFFFFFF),
    accentSoft: Color(0xFF1A1530),
    sync: Color(0xFF67E8F9),
    syncSoft: Color(0xFF0B2E36),
    added: Color(0xFF34D399),
    removed: Color(0xFFE5484D),
    kept: Color(0xFF3A4252),
    warn: Color(0xFFFBBF24),
    warnSoft: Color(0xFF3A2E0B),
    danger: Color(0xFFF87171),
    label: Color(0xFFFCD34D),
    labelSoft: Color(0xFF3A2E0B),
    ok: Color(0xFF34D399),
  );

  static const light = Palette(
    ground: Color(0xFFF6F7FB),
    sidebar: Color(0xFFEEF0F6),
    panel: Color(0xFFFFFFFF),
    raised: Color(0xFFF6F7FB),
    line: Color(0xFFE3E6EE),
    text: Color(0xFF161A22),
    muted: Color(0xFF5B6475),
    faint: Color(0xFF5B6475),
    accent: Color(0xFF7C3AED),
    onAccent: Color(0xFFFFFFFF),
    accentSoft: Color(0xFFF5F3FF),
    sync: Color(0xFF0E7490),
    syncSoft: Color(0xFFECFEFF),
    added: Color(0xFF059669),
    removed: Color(0xFFDC2626),
    kept: Color(0xFFD5DAE3),
    warn: Color(0xFF8A4B00),
    warnSoft: Color(0xFFFFF4E5),
    danger: Color(0xFFB42318),
    label: Color(0xFF8A5A00),
    labelSoft: Color(0xFFFEF3C7),
    ok: Color(0xFF059669),
  );

  static Palette of(BuildContext context) => Theme.of(context).extension<Palette>()!;

  @override
  Palette copyWith() => this;

  @override
  Palette lerp(Palette? other, double t) => t < 0.5 ? this : (other ?? this);
}

ThemeData studioTheme(Brightness brightness) {
  final p = brightness == Brightness.dark ? Palette.dark : Palette.light;
  final base = ThemeData(
    brightness: brightness,
    useMaterial3: true,
    fontFamily: 'Inter',
    colorScheme: ColorScheme.fromSeed(
      seedColor: p.accent,
      brightness: brightness,
      primary: p.accent,
      onPrimary: p.onAccent,
      surface: p.panel,
      onSurface: p.text,
      error: p.danger,
    ),
    scaffoldBackgroundColor: p.ground,
    dividerColor: p.line,
    extensions: [p],
  );
  return base.copyWith(
    textTheme: base.textTheme.apply(bodyColor: p.text, displayColor: p.text),
    tooltipTheme: TooltipThemeData(
      decoration: BoxDecoration(color: p.raised, borderRadius: BorderRadius.circular(8), border: Border.all(color: p.line)),
      textStyle: TextStyle(color: p.text, fontSize: 12),
      waitDuration: const Duration(milliseconds: 400),
    ),
    dialogTheme: DialogThemeData(
      backgroundColor: p.panel,
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(18)),
    ),
    inputDecorationTheme: InputDecorationTheme(
      filled: true,
      fillColor: p.ground,
      contentPadding: const EdgeInsets.symmetric(horizontal: 14, vertical: 12),
      border: OutlineInputBorder(borderRadius: BorderRadius.circular(12), borderSide: BorderSide(color: p.line)),
      enabledBorder: OutlineInputBorder(borderRadius: BorderRadius.circular(12), borderSide: BorderSide(color: p.line)),
      focusedBorder: OutlineInputBorder(borderRadius: BorderRadius.circular(12), borderSide: BorderSide(color: p.accent, width: 1.5)),
      hintStyle: TextStyle(color: p.faint),
    ),
    snackBarTheme: SnackBarThemeData(
      behavior: SnackBarBehavior.floating,
      backgroundColor: p.raised,
      contentTextStyle: TextStyle(color: p.text, fontFamily: 'Inter'),
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12), side: BorderSide(color: p.line)),
    ),
    checkboxTheme: CheckboxThemeData(
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(5)),
      side: BorderSide(color: p.faint, width: 1.5),
    ),
  );
}

const mono = TextStyle(fontFamily: 'JetBrains Mono', fontSize: 12);

/// "1.2 MB" for people.
String fmtBytes(int n) {
  if (n >= 1000000000) return '${(n / 1e9).toStringAsFixed(1)} GB';
  if (n >= 1000000) return '${(n / 1e6).toStringAsFixed(1)} MB';
  if (n >= 1000) return '${(n / 1e3).toStringAsFixed(1)} KB';
  return '$n B';
}

const _months = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
const _days = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'];

/// "just now", "12 min ago", "yesterday", "Mon", "Nov 12", "Nov 12, 2024".
String fmtWhen(int unixSeconds, {DateTime? now}) {
  final t = DateTime.fromMillisecondsSinceEpoch(unixSeconds * 1000);
  final n = now ?? DateTime.now();
  final d = n.difference(t);
  if (d.inSeconds < 60) return 'just now';
  if (d.inMinutes < 60) return '${d.inMinutes} min ago';
  if (d.inHours < 12) return '${d.inHours} h ago';
  final today = DateTime(n.year, n.month, n.day);
  final day = DateTime(t.year, t.month, t.day);
  final days = today.difference(day).inDays;
  if (days == 0) return 'today';
  if (days == 1) return 'yesterday';
  if (days < 7) return _days[t.weekday - 1];
  if (t.year == n.year) return '${_months[t.month - 1]} ${t.day}';
  return '${_months[t.month - 1]} ${t.day}, ${t.year}';
}

/// "Mon 14:32", the full time of a save.
String fmtStamp(int unixSeconds) {
  final t = DateTime.fromMillisecondsSinceEpoch(unixSeconds * 1000);
  String two(int v) => v.toString().padLeft(2, '0');
  return '${_days[t.weekday - 1]} ${_months[t.month - 1]} ${t.day}, ${t.year} · ${two(t.hour)}:${two(t.minute)}';
}

/// An animation's length, or none when the system asks for less motion.
Duration motion(BuildContext context, int milliseconds) =>
    MediaQuery.maybeDisableAnimationsOf(context) ?? false ? Duration.zero : Duration(milliseconds: milliseconds);
