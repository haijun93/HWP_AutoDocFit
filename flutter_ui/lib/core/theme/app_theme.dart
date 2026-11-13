import 'package:flutter/material.dart';

import 'system_colors.dart';

/// 앱 전체 색 토큰(DARK1, 2026-11-13).
///
/// 화면 코드는 색 값을 직접 쓰지 않고 `context.colors.<토큰>`으로 읽는다. 라이트 값은 기존 하드코딩
/// 값과 똑같이 둔다(기존 화면·시험 그대로). 다크 값은 모든 글자 토큰이 모든 바탕 토큰 위에서
/// WCAG AA 4.5:1 이상이 되게 골랐다(`test/theme/app_theme_contrast_test.dart`).
/// 고대비는 Windows 시스템 색(GetSysColor)으로 만든다([AppColors.highContrast]).
///
/// 상태 기계: 테마 = (사용자 선택 [AppThemeChoice]) × (시스템 밝기) × (시스템 고대비).
/// - 고대비가 켜져 있으면 선택과 상관없이 고대비 색.
/// - 아니면 system → 시스템 밝기, light → 라이트, dark → 다크.
/// 선택 값은 [appThemeChoice] 하나에만 있고 설정 파일 키는 `theme_mode`.
enum AppThemeChoice {
  system('system', '시스템 설정 따르기'),
  light('light', '밝게'),
  dark('dark', '어둡게');

  const AppThemeChoice(this.key, this.label);
  final String key;
  final String label;

  static AppThemeChoice parse(Object? value) => AppThemeChoice.values
      .firstWhere((c) => c.key == value, orElse: () => AppThemeChoice.system);

  ThemeMode get mode => switch (this) {
    AppThemeChoice.system => ThemeMode.system,
    AppThemeChoice.light => ThemeMode.light,
    AppThemeChoice.dark => ThemeMode.dark,
  };
}

/// 현재 테마 선택(앱 전역 하나). `DocFitApp`이 듣고 다시 그린다.
final appThemeChoice = ValueNotifier<AppThemeChoice>(AppThemeChoice.system);

/// 테마 저장 대기열(DARK1-R). 다른 명령이 처리 중이어도 저장을 버리지 않고, 저장이 겹치면
/// 차례로 보내되 그 사이 쌓인 선택은 마지막 값 하나만 보낸다(설정 파일 = 화면에 보이는 선택).
/// 상태: 쉼 → 저장 중(대기 값 없음/있음) → 대기 값이 있으면 이어서 저장 → 쉼.
class ThemeSaveQueue {
  ThemeSaveQueue(this._save, {this.onError});

  final Future<void> Function(AppThemeChoice choice) _save;
  final void Function(Object error)? onError;
  AppThemeChoice? _pending;
  Future<void>? _running;

  bool get saving => _running != null;

  /// [choice]를 저장 대기에 넣고, 대기열이 빌 때까지 기다린다.
  Future<void> save(AppThemeChoice choice) {
    _pending = choice;
    return _running ??= _drain().whenComplete(() => _running = null);
  }

  Future<void> _drain() async {
    while (_pending != null) {
      final next = _pending!;
      _pending = null;
      try {
        await _save(next);
      } catch (e) {
        onError?.call(e);
      }
    }
  }
}

/// 작업 방식 카드 색(문서편집 작업 카드).
typedef ModeColors = ({Color surface, Color selectedSurface, Color accent});

@immutable
class AppColors extends ThemeExtension<AppColors> {
  const AppColors({
    required this.brightness,
    required this.highContrast,
    required this.canvas,
    required this.surface,
    required this.surfaceMuted,
    required this.surfaceSubtle,
    required this.line,
    required this.lineStrong,
    required this.ink,
    required this.textStrong,
    required this.text,
    required this.textBody,
    required this.textSecondary,
    required this.muted,
    required this.faint,
    required this.accent,
    required this.accentDeep,
    required this.accentFill,
    required this.onAccent,
    required this.accentSoft,
    required this.mint,
    required this.mintSoft,
    required this.danger,
    required this.dangerDeep,
    required this.dangerSoft,
    required this.success,
    required this.successSoft,
    required this.warning,
    required this.warningSoft,
    required this.highlight,
    required this.onHighlight,
    required this.scrim,
    required this.shadow,
    required this.modes,
  });

  final Brightness brightness;
  final bool highContrast;

  // 바탕
  /// 창 바탕(Scaffold).
  final Color canvas;
  /// 카드·대화상자 바탕(라이트 흰색).
  final Color surface;
  /// 살짝 회색 바탕(#f8fafc).
  final Color surfaceMuted;
  /// 더 진한 회색 바탕(#f1f5f9).
  final Color surfaceSubtle;
  /// 구분선·테두리(#e2e8f0).
  final Color line;
  /// 진한 테두리(#cbd5e1).
  final Color lineStrong;

  // 글자
  /// 제목 글자(#172b4d).
  final Color ink;
  /// 가장 진한 글자(#0f172a).
  final Color textStrong;
  /// 본문 진한 글자(#1e293b).
  final Color text;
  /// 본문 글자(#334155).
  final Color textBody;
  /// 보조 글자(#475569).
  final Color textSecondary;
  /// 설명 글자(#64748b).
  final Color muted;
  /// 흐린 글자·자리표시(#94a3b8). 라이트 값은 기존 값이라 4.5:1 미만(장식·비활성용).
  final Color faint;

  // 강조
  /// 강조 글자·아이콘·테두리(#2563eb).
  final Color accent;
  /// 진한 강조(#1d4ed8).
  final Color accentDeep;
  /// 강조 단추 바탕(흰 글자와 4.5:1 이상).
  final Color accentFill;
  /// [accentFill] 위 글자.
  final Color onAccent;
  /// 강조 옅은 바탕(#edf3ff).
  final Color accentSoft;
  final Color mint;
  final Color mintSoft;

  // 상태
  final Color danger;
  final Color dangerDeep;
  final Color dangerSoft;
  final Color success;
  final Color successSoft;
  final Color warning;
  final Color warningSoft;
  /// 검색 일치 표시 바탕(#ffeb3b)과 그 위 글자.
  final Color highlight;
  final Color onHighlight;

  /// 모달 뒤 가림막·그림자.
  final Color scrim;
  final Color shadow;

  /// 작업 방식별 카드 색. 키: spacing·unify·all·file_format, 그 밖은 기본.
  final Map<String, ModeColors> modes;

  bool get isDark => brightness == Brightness.dark;

  ModeColors mode(String name) =>
      modes[name] ??
      (surface: surface, selectedSurface: accentSoft, accent: accent);

  static const light = AppColors(
    brightness: Brightness.light,
    highContrast: false,
    canvas: Color(0xfff6f8fb),
    surface: Color(0xffffffff),
    surfaceMuted: Color(0xfff8fafc),
    surfaceSubtle: Color(0xfff1f5f9),
    line: Color(0xffe2e8f0),
    lineStrong: Color(0xffcbd5e1),
    ink: Color(0xff172b4d),
    textStrong: Color(0xff0f172a),
    text: Color(0xff1e293b),
    textBody: Color(0xff334155),
    textSecondary: Color(0xff475569),
    muted: Color(0xff64748b),
    faint: Color(0xff94a3b8),
    accent: Color(0xff2563eb),
    accentDeep: Color(0xff1d4ed8),
    accentFill: Color(0xff2563eb),
    onAccent: Color(0xffffffff),
    accentSoft: Color(0xffedf3ff),
    mint: Color(0xff287c72),
    mintSoft: Color(0xfff3f8f7),
    danger: Color(0xffdc2626),
    dangerDeep: Color(0xffb91c1c),
    dangerSoft: Color(0xfffef2f2),
    success: Color(0xff15803d),
    successSoft: Color(0xffdff3ee),
    warning: Color(0xffe0a526),
    warningSoft: Color(0xfffff7eb),
    highlight: Color(0xffffeb3b),
    onHighlight: Color(0xff0f172a),
    scrim: Color(0x8a000000),
    shadow: Color(0x14000000),
    modes: {
      'spacing': (
        surface: Color(0xfff0f5ff),
        selectedSurface: Color(0xffe3edff),
        accent: Color(0xff3b6fb6),
      ),
      'unify': (
        surface: Color(0xffeff8f5),
        selectedSurface: Color(0xffdef1ea),
        accent: Color(0xff2f8273),
      ),
      'all': (
        surface: Color(0xfffff7eb),
        selectedSurface: Color(0xffffedcf),
        accent: Color(0xffa86a1f),
      ),
      'file_format': (
        surface: Color(0xfff4f1ff),
        selectedSurface: Color(0xffeae4ff),
        accent: Color(0xff7656a8),
      ),
    },
  );

  static const dark = AppColors(
    brightness: Brightness.dark,
    highContrast: false,
    canvas: Color(0xff0f141a),
    surface: Color(0xff171d26),
    surfaceMuted: Color(0xff1b222c),
    surfaceSubtle: Color(0xff212a35),
    line: Color(0xff2e3846),
    lineStrong: Color(0xff435064),
    ink: Color(0xffe8eef6),
    textStrong: Color(0xfff1f5f9),
    text: Color(0xffe2e8f0),
    textBody: Color(0xffcbd5e1),
    textSecondary: Color(0xffb4c0cf),
    muted: Color(0xff9eabbb),
    faint: Color(0xff8e9bad),
    accent: Color(0xff6ea2ff),
    accentDeep: Color(0xff93bbff),
    accentFill: Color(0xff2563eb),
    onAccent: Color(0xffffffff),
    accentSoft: Color(0xff1c2a44),
    mint: Color(0xff5cc4b5),
    mintSoft: Color(0xff16302c),
    danger: Color(0xffff7b7b),
    dangerDeep: Color(0xffff9c9c),
    dangerSoft: Color(0xff3a1d20),
    success: Color(0xff4ade80),
    successSoft: Color(0xff15302a),
    warning: Color(0xfff0b94a),
    warningSoft: Color(0xff3a2e14),
    highlight: Color(0xff8a7300),
    onHighlight: Color(0xffffffff),
    scrim: Color(0x99000000),
    shadow: Color(0x66000000),
    modes: {
      'spacing': (
        surface: Color(0xff172235),
        selectedSurface: Color(0xff1d2d4a),
        accent: Color(0xff8fb4ee),
      ),
      'unify': (
        surface: Color(0xff152a26),
        selectedSurface: Color(0xff1b3832),
        accent: Color(0xff6cc3b2),
      ),
      'all': (
        surface: Color(0xff2e2415),
        selectedSurface: Color(0xff3d2f17),
        accent: Color(0xffe6b36e),
      ),
      'file_format': (
        surface: Color(0xff241e36),
        selectedSurface: Color(0xff2f2747),
        accent: Color(0xffbca6ec),
      ),
    },
  );

  /// Windows 고대비: 시스템 색 그대로. 바탕·글자·강조 세 쌍만 쓰므로 대비는 사용자가 고른 고대비 테마가 정한다.
  factory AppColors.highContrast(SystemColors s) {
    final dark = s.window.computeLuminance() < 0.5;
    final modes = {
      for (final k in const ['spacing', 'unify', 'all', 'file_format'])
        k: (surface: s.window, selectedSurface: s.highlight, accent: s.hotlight),
    };
    return AppColors(
      brightness: dark ? Brightness.dark : Brightness.light,
      highContrast: true,
      canvas: s.window,
      surface: s.window,
      surfaceMuted: s.window,
      surfaceSubtle: s.buttonFace,
      line: s.windowText,
      lineStrong: s.windowText,
      ink: s.windowText,
      textStrong: s.windowText,
      text: s.windowText,
      textBody: s.windowText,
      textSecondary: s.windowText,
      muted: s.windowText,
      faint: s.grayText,
      accent: s.hotlight,
      accentDeep: s.hotlight,
      accentFill: s.highlight,
      onAccent: s.highlightText,
      accentSoft: s.window,
      mint: s.windowText,
      mintSoft: s.window,
      danger: s.windowText,
      dangerDeep: s.windowText,
      dangerSoft: s.window,
      success: s.windowText,
      successSoft: s.window,
      warning: s.windowText,
      warningSoft: s.window,
      highlight: s.highlight,
      onHighlight: s.highlightText,
      scrim: const Color(0x99000000),
      shadow: const Color(0x00000000),
      modes: modes,
    );
  }

  static AppColors of(BuildContext context) =>
      Theme.of(context).extension<AppColors>() ?? light;

  @override
  AppColors copyWith() => this;

  @override
  AppColors lerp(covariant AppColors? other, double t) =>
      other == null || t < 0.5 ? this : other;
}

/// 키오스크(설정 화면) 색. 라이트 값은 기존 `Kiosk` 상수와 같다.
@immutable
class KioskColors extends ThemeExtension<KioskColors> {
  const KioskColors({
    required this.blue,
    required this.blueFill,
    required this.onBlue,
    required this.blueSoft,
    required this.ink,
    required this.sub,
    required this.faint,
    required this.line,
    required this.canvas,
    required this.card,
    required this.green,
    required this.red,
    required this.disabledFill,
    required this.secondaryFill,
  });

  /// 강조 글자·선택 테두리.
  final Color blue;
  /// 기본 단추 바탕과 그 위 글자.
  final Color blueFill;
  final Color onBlue;
  final Color blueSoft;
  final Color ink;
  final Color sub;
  final Color faint;
  final Color line;
  final Color canvas;
  final Color card;
  final Color green;
  final Color red;
  final Color disabledFill;
  final Color secondaryFill;

  static const light = KioskColors(
    blue: Color(0xff3182f6),
    blueFill: Color(0xff3182f6),
    onBlue: Color(0xffffffff),
    blueSoft: Color(0xffe8f3ff),
    ink: Color(0xff191f28),
    sub: Color(0xff4e5968),
    faint: Color(0xff8b95a1),
    line: Color(0xffe5e8eb),
    canvas: Color(0xfff2f4f6),
    card: Color(0xffffffff),
    green: Color(0xff03b26c),
    red: Color(0xfff04452),
    disabledFill: Color(0xffd1d6db),
    secondaryFill: Color(0xfff2f4f6),
  );

  static const dark = KioskColors(
    blue: Color(0xff6ea8ff),
    blueFill: Color(0xff2563eb),
    onBlue: Color(0xffffffff),
    blueSoft: Color(0xff1c2a44),
    ink: Color(0xffe8eef6),
    sub: Color(0xffb4c0cf),
    faint: Color(0xff8e9bad),
    line: Color(0xff2e3846),
    canvas: Color(0xff0f141a),
    card: Color(0xff171d26),
    green: Color(0xff4ade80),
    red: Color(0xffff7b7b),
    disabledFill: Color(0xff2e3846),
    secondaryFill: Color(0xff212a35),
  );

  factory KioskColors.highContrast(SystemColors s) => KioskColors(
    blue: s.hotlight,
    blueFill: s.highlight,
    onBlue: s.highlightText,
    blueSoft: s.window,
    ink: s.windowText,
    sub: s.windowText,
    faint: s.grayText,
    line: s.windowText,
    canvas: s.window,
    card: s.window,
    green: s.windowText,
    red: s.windowText,
    disabledFill: s.buttonFace,
    secondaryFill: s.buttonFace,
  );

  static KioskColors of(BuildContext context) =>
      Theme.of(context).extension<KioskColors>() ?? light;

  @override
  KioskColors copyWith() => this;

  @override
  KioskColors lerp(covariant KioskColors? other, double t) =>
      other == null || t < 0.5 ? this : other;
}

extension AppThemeContext on BuildContext {
  AppColors get colors => AppColors.of(this);
  KioskColors get kiosk => KioskColors.of(this);
}

/// 팔레트로 ThemeData를 만든다. 라이트는 기존 main.dart ThemeData와 같은 값.
ThemeData buildAppTheme(AppColors c, KioskColors k) {
  final ColorScheme scheme;
  if (c.highContrast) {
    scheme = ColorScheme(
      brightness: c.brightness,
      primary: c.accentFill,
      onPrimary: c.onAccent,
      secondary: c.accentFill,
      onSecondary: c.onAccent,
      error: c.danger,
      onError: c.surface,
      surface: c.surface,
      onSurface: c.text,
      outline: c.line,
    );
  } else if (c.isDark) {
    scheme = ColorScheme.fromSeed(
      seedColor: AppColors.light.accent,
      brightness: Brightness.dark,
    ).copyWith(
      surface: c.surface,
      onSurface: c.text,
      onSurfaceVariant: c.muted,
      outline: c.lineStrong,
      outlineVariant: c.line,
      error: c.danger,
    );
  } else {
    scheme = ColorScheme.fromSeed(seedColor: c.accent);
  }
  return ThemeData(
    useMaterial3: true,
    brightness: c.brightness,
    fontFamily: 'NotoSansKR',
    fontFamilyFallback: const ['NotoSansSymbols2', 'STIXTwoMath'],
    colorScheme: scheme,
    scaffoldBackgroundColor: c.canvas,
    extensions: [c, k],
    filledButtonTheme: FilledButtonThemeData(
      style: FilledButton.styleFrom(
        minimumSize: const Size(48, 52),
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(16)),
      ),
    ),
    outlinedButtonTheme: OutlinedButtonThemeData(
      style: OutlinedButton.styleFrom(
        minimumSize: const Size(48, 48),
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(16)),
      ),
    ),
    inputDecorationTheme: const InputDecorationTheme(
      border: OutlineInputBorder(),
    ),
  );
}

final lightAppTheme = buildAppTheme(AppColors.light, KioskColors.light);
final darkAppTheme = buildAppTheme(AppColors.dark, KioskColors.dark);

/// 고대비 테마(시스템 색). 시스템 색을 읽을 수 없는 환경(웹·시험)에서는 null.
ThemeData? highContrastAppTheme() {
  final s = readSystemColors();
  if (s == null) return null;
  return buildAppTheme(AppColors.highContrast(s), KioskColors.highContrast(s));
}

/// 시스템 고대비·밝기가 바뀌면 1씩 늘어난다(고대비 시스템 색을 다시 읽게).
/// Windows의 WM_SETTINGCHANGE("ImmersiveColorSet")·고대비 전환은 Flutter 엔진이 받아
/// didChangePlatformBrightness·didChangeAccessibilityFeatures로 알려 준다.
final systemThemeTick = _SystemThemeWatcher();

class _SystemThemeWatcher extends ChangeNotifier with WidgetsBindingObserver {
  bool _attached = false;

  void ensureAttached() {
    if (_attached) return;
    _attached = true;
    WidgetsBinding.instance.addObserver(this);
  }

  @override
  void didChangePlatformBrightness() => notifyListeners();

  @override
  void didChangeAccessibilityFeatures() => notifyListeners();
}

/// 테마 선택·시스템 변경을 함께 듣는 Listenable.
Listenable appThemeListenable() {
  systemThemeTick.ensureAttached();
  return Listenable.merge([appThemeChoice, systemThemeTick]);
}
