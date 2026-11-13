import 'package:docfit_ui/core/theme/app_theme.dart';
import 'package:docfit_ui/core/theme/system_colors.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

/// WCAG 2.x 대비(상대 휘도 비).
double contrast(Color a, Color b) {
  final la = a.computeLuminance(), lb = b.computeLuminance();
  final hi = la > lb ? la : lb, lo = la > lb ? lb : la;
  return (hi + 0.05) / (lo + 0.05);
}

void main() {
  group('다크 팔레트 WCAG AA(4.5:1)', () {
    const c = AppColors.dark;
    final backgrounds = {
      'canvas': c.canvas,
      'surface': c.surface,
      'surfaceMuted': c.surfaceMuted,
      'surfaceSubtle': c.surfaceSubtle,
      'accentSoft': c.accentSoft,
    };
    final texts = {
      'ink': c.ink,
      'textStrong': c.textStrong,
      'text': c.text,
      'textBody': c.textBody,
      'textSecondary': c.textSecondary,
      'muted': c.muted,
      'faint': c.faint,
      'accent': c.accent,
      'accentDeep': c.accentDeep,
      'mint': c.mint,
      'danger': c.danger,
      'dangerDeep': c.dangerDeep,
      'success': c.success,
      'warning': c.warning,
    };
    for (final t in texts.entries) {
      for (final b in backgrounds.entries) {
        test('${t.key} / ${b.key}', () {
          expect(contrast(t.value, b.value), greaterThanOrEqualTo(4.5));
        });
      }
    }
    test('상태 옅은 바탕 위 상태 글자', () {
      expect(contrast(c.danger, c.dangerSoft), greaterThanOrEqualTo(4.5));
      expect(contrast(c.success, c.successSoft), greaterThanOrEqualTo(4.5));
      expect(contrast(c.warning, c.warningSoft), greaterThanOrEqualTo(4.5));
      expect(contrast(c.mint, c.mintSoft), greaterThanOrEqualTo(4.5));
      expect(contrast(c.onAccent, c.accentFill), greaterThanOrEqualTo(4.5));
      expect(contrast(c.onHighlight, c.highlight), greaterThanOrEqualTo(4.5));
    });
    test('작업 방식 카드 강조색', () {
      for (final m in c.modes.values) {
        expect(contrast(m.accent, m.surface), greaterThanOrEqualTo(4.5));
        expect(contrast(m.accent, m.selectedSurface), greaterThanOrEqualTo(4.5));
        expect(contrast(c.text, m.selectedSurface), greaterThanOrEqualTo(4.5));
      }
    });
  });

  group('키오스크 다크 WCAG AA', () {
    const k = KioskColors.dark;
    test('글자 / 카드·바탕', () {
      for (final fg in [k.ink, k.sub, k.faint, k.blue, k.green, k.red]) {
        expect(contrast(fg, k.card), greaterThanOrEqualTo(4.5));
        expect(contrast(fg, k.canvas), greaterThanOrEqualTo(4.5));
      }
      expect(contrast(k.blue, k.blueSoft), greaterThanOrEqualTo(4.5));
      expect(contrast(k.onBlue, k.blueFill), greaterThanOrEqualTo(4.5));
      expect(contrast(k.sub, k.secondaryFill), greaterThanOrEqualTo(4.5));
    });
  });

  group('라이트 팔레트는 기존 값 그대로', () {
    test('주요 토큰', () {
      const c = AppColors.light;
      expect(c.ink, const Color(0xff172b4d));
      expect(c.muted, const Color(0xff64748b));
      expect(c.line, const Color(0xffe2e8f0));
      expect(c.accent, const Color(0xff2563eb));
      expect(c.canvas, const Color(0xfff6f8fb));
      expect(c.mode('spacing').selectedSurface, const Color(0xffe3edff));
      expect(c.mode('unknown').surface, const Color(0xffffffff));
      expect(KioskColors.light.blue, const Color(0xff3182f6));
    });
    test('본문 글자는 흰 바탕에서 4.5:1 이상(faint는 기존 장식용 값이라 제외)', () {
      const c = AppColors.light;
      for (final fg in [c.ink, c.text, c.textBody, c.textSecondary, c.muted, c.accent, c.danger, c.success]) {
        expect(contrast(fg, c.surface), greaterThanOrEqualTo(4.5));
      }
    });
  });

  group('고대비는 시스템 색을 그대로 쓴다', () {
    const sys = SystemColors(
      window: Color(0xff000000),
      windowText: Color(0xffffffff),
      highlight: Color(0xff1aebff),
      highlightText: Color(0xff000000),
      buttonFace: Color(0xff000000),
      grayText: Color(0xff3ff23f),
      hotlight: Color(0xffffff00),
    );
    test('색 배치', () {
      final c = AppColors.highContrast(sys);
      expect(c.highContrast, isTrue);
      expect(c.brightness, Brightness.dark);
      expect(c.surface, sys.window);
      expect(c.text, sys.windowText);
      expect(c.accentFill, sys.highlight);
      expect(c.onAccent, sys.highlightText);
      final theme = buildAppTheme(c, KioskColors.highContrast(sys));
      expect(theme.colorScheme.primary, sys.highlight);
      expect(theme.scaffoldBackgroundColor, sys.window);
    });
    test('COLORREF 변환(0x00BBGGRR)', () {
      expect(SystemColors.fromColorRef(0x00ff8000), const Color(0xff0080ff));
    });
  });

  group('테마 선택', () {
    test('parse', () {
      expect(AppThemeChoice.parse('dark'), AppThemeChoice.dark);
      expect(AppThemeChoice.parse('light').mode, ThemeMode.light);
      expect(AppThemeChoice.parse(null), AppThemeChoice.system);
      expect(AppThemeChoice.parse('bogus').mode, ThemeMode.system);
    });
  });
}
