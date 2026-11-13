import 'package:flutter/material.dart';

import '../../core/theme/app_theme.dart';

/// 설정·서식 키오스크 화면에서 [KioskColors] 토큰에 없는 색(DARK1).
/// 라이트 값은 원래 하드코딩 값과 같다. 다크 값은 [KioskColors.dark] 바탕 계열이고,
/// 그 위 글자(ink·sub·faint)와 4.5:1 이상이다. 고대비는 [AppColors] 시스템 색으로 대체한다.
@immutable
class SettingsColors {
  const SettingsColors({
    required this.softFill,
    required this.unchangedFill,
    required this.infoFill,
    required this.infoLine,
    required this.badgeLine,
    required this.boxLine,
    required this.errorText,
  });

  /// 옅은 회색 바탕(꺼진 칸·안 고른 견본 받침, #f9fafb).
  final Color softFill;

  /// 안 바뀐 항목 칸 바탕(#fafbfc).
  final Color unchangedFill;

  /// 읽기 전용 안내 상자 바탕(#f8f9fa)과 테두리(#e9ecef).
  final Color infoFill;
  final Color infoLine;

  /// 안 고른 방향 표시 테두리(#d5d9de).
  final Color badgeLine;

  /// 안 바뀐 여백 칸 테두리(#d0d5dc).
  final Color boxLine;

  /// 오류 글자(Colors.red).
  final Color errorText;

  static const light = SettingsColors(
    softFill: Color(0xfff9fafb),
    unchangedFill: Color(0xfffafbfc),
    infoFill: Color(0xfff8f9fa),
    infoLine: Color(0xffe9ecef),
    badgeLine: Color(0xffd5d9de),
    boxLine: Color(0xffd0d5dc),
    errorText: Color(0xfff44336),
  );

  static const dark = SettingsColors(
    softFill: Color(0xff1b222c),
    unchangedFill: Color(0xff1b222c),
    infoFill: Color(0xff1b222c),
    infoLine: Color(0xff2e3846),
    badgeLine: Color(0xff435064),
    boxLine: Color(0xff435064),
    errorText: Color(0xffff7b7b),
  );

  static SettingsColors of(BuildContext context) {
    final c = AppColors.of(context);
    if (c.highContrast) {
      return SettingsColors(
        softFill: c.surface,
        unchangedFill: c.surface,
        infoFill: c.surface,
        infoLine: c.line,
        badgeLine: c.line,
        boxLine: c.line,
        errorText: c.danger,
      );
    }
    return c.isDark ? dark : light;
  }
}
