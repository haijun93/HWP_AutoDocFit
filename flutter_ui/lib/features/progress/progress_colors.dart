import 'package:flutter/material.dart';

import '../../core/theme/app_theme.dart';

/// 진행 화면(LiveProgress) 색(DARK1). 앱 토큰과 같은 값은 AppColors에서 가져오고,
/// 토큰에 없는 값(흐린 회색 8b98ad·하늘·밝은 민트·검은 알약·링 트랙·빛 막대 바탕)만
/// 라이트(원래 리터럴)·다크로 고른다. 다크 글자는 바탕에서 4.5:1 이상.
@immutable
class ProgressColors {
  const ProgressColors({
    required this.ink,
    required this.muted,
    required this.faint,
    required this.line,
    required this.blue,
    required this.blueDeep,
    required this.blueSoft,
    required this.blueFill,
    required this.onBlue,
    required this.sky,
    required this.mint,
    required this.mintBright,
    required this.mintSoft,
    required this.red,
    required this.island,
    required this.onIsland,
    required this.panel,
    required this.doneText,
    required this.waitText,
    required this.track,
    required this.shimmerTrack,
  });

  final Color ink;
  final Color muted;
  /// 흐린 아이콘·대기 글자(라이트 8b98ad).
  final Color faint;
  final Color line;
  final Color blue;
  final Color blueDeep;
  final Color blueSoft;
  /// 지금 단계 점 바탕(라이트 2563eb)과 그 위 아이콘(흰색).
  final Color blueFill;
  final Color onBlue;
  /// 링 그라데이션·지금 파일 테두리(라이트 60a5fa).
  final Color sky;
  final Color mint;
  /// 완료 점·아이콘(라이트 34c79a).
  final Color mintBright;
  final Color mintSoft;
  final Color red;
  /// 다이내믹 아일랜드 알약 바탕(라이트 0f172a)과 그 위 글자.
  final Color island;
  final Color onIsland;
  /// 안내 카드·파일 줄 바탕(f8fafc).
  final Color panel;
  /// 끝난 단계·대기 단계 글자(475569·94a3b8).
  final Color doneText;
  final Color waitText;
  /// 활동 링 회색 트랙(라이트 edf2f7)·빛 막대 바탕(라이트 dbe7fb).
  final Color track;
  final Color shimmerTrack;

  static ProgressColors of(BuildContext context) {
    final c = AppColors.of(context);
    final hc = c.highContrast;
    final dark = c.isDark;
    return ProgressColors(
      ink: c.ink,
      muted: c.muted,
      faint: hc
          ? c.faint
          : dark
          ? const Color(0xff8e9bad)
          : const Color(0xff8b98ad),
      line: c.line,
      blue: c.accent,
      blueDeep: c.accentDeep,
      blueSoft: c.accentSoft,
      blueFill: c.accentFill,
      onBlue: c.onAccent,
      sky: hc ? c.accent : const Color(0xff60a5fa),
      mint: c.mint,
      mintBright: hc ? c.success : const Color(0xff34c79a),
      mintSoft: c.successSoft,
      red: c.danger,
      island: hc
          ? c.textStrong
          : dark
          ? const Color(0xff263041)
          : const Color(0xff0f172a),
      onIsland: hc ? c.surface : const Color(0xffffffff),
      panel: c.surfaceMuted,
      doneText: c.textSecondary,
      waitText: c.faint,
      track: hc
          ? c.line
          : dark
          ? const Color(0xff2a3442)
          : const Color(0xffedf2f7),
      shimmerTrack: hc
          ? c.surfaceSubtle
          : dark
          ? const Color(0xff1c2a44)
          : const Color(0xffdbe7fb),
    );
  }

  List<Color> get _all => [
    ink, muted, faint, line, blue, blueDeep, blueSoft, blueFill, onBlue, sky, mint, mintBright, //
    mintSoft, red, island, onIsland, panel, doneText, waitText, track,
    shimmerTrack,
  ];

  @override
  bool operator ==(Object other) {
    if (other is! ProgressColors) return false;
    final a = _all;
    final b = other._all;
    for (var i = 0; i < a.length; i++) {
      if (a[i] != b[i]) return false;
    }
    return true;
  }

  @override
  int get hashCode => Object.hashAll(_all);
}
