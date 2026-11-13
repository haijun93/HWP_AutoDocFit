import 'package:flutter/material.dart';

import '../../core/theme/app_theme.dart';

/// 타임머신 화면 지역 색(DARK1). 라이트 값 = 기존 리터럴 그대로.
/// 검색 일치 바탕은 AppColors.highlight를 쓰고 여기서는 그 위 글자와 나머지만 정한다.
/// 다크는 AppColors.dark 바탕 위에서 글자 4.5:1 이상, 고대비는 AppColors 토큰으로 대체.
@immutable
class TimemachineColors {
  const TimemachineColors({
    required this.onMark,
    required this.activeMark,
    required this.onActiveMark,
    this.activeMarkDecorated = false,
    required this.secondaryText,
    required this.hintText,
    required this.rule,
    required this.infoIcon,
    required this.blue,
    required this.error,
  });

  /// 일치 표시(highlight·activeMark) 위 글자.
  final Color onMark;
  /// 미리보기에서 지금 가리키는 일치 바탕.
  final Color activeMark;
  /// 지금 가리키는 일치 위 글자.
  final Color onActiveMark;
  /// 고대비: 시스템 색은 둘뿐이라 색만으로는 일반 일치와 구별이 약하므로 굵게·밑줄도 붙인다.
  final bool activeMarkDecorated;
  /// black54 글자.
  final Color secondaryText;
  /// 회색(Colors.grey) 위치 글자.
  final Color hintText;
  /// 회색 세로 구분선(투명도는 쓰는 곳에서).
  final Color rule;
  /// 재색인 안내 아이콘.
  final Color infoIcon;
  /// 밀도 막대·달력·시간 줄 강조(투명도는 쓰는 곳에서).
  final Color blue;
  final Color error;

  static const light = TimemachineColors(
    onMark: Colors.black,
    activeMark: Colors.orangeAccent,
    onActiveMark: Colors.black,
    secondaryText: Colors.black54,
    hintText: Colors.grey,
    rule: Colors.grey,
    infoIcon: Colors.amber,
    blue: Colors.blue,
    error: Colors.red,
  );

  /// 지금 가리키는 일치의 글자 모양.
  TextStyle get activeMarkStyle => TextStyle(
        backgroundColor: activeMark,
        color: onActiveMark,
        fontWeight: activeMarkDecorated ? FontWeight.w700 : null,
        decoration: activeMarkDecorated ? TextDecoration.underline : null,
        decorationColor: activeMarkDecorated ? onActiveMark : null,
      );

  static TimemachineColors of(BuildContext context) {
    final c = AppColors.of(context);
    if (!c.isDark && !c.highContrast) return light;
    final hc = c.highContrast;
    return TimemachineColors(
      onMark: c.onHighlight,
      // 고대비: 일반 일치는 (Highlight 바탕, HighlightText 글자), 지금 일치는 그 둘을 뒤집어
      // (HighlightText 바탕, Highlight 글자) 구별한다. 시스템이 정한 색 쌍이라 대비가 보장된다.
      activeMark: hc ? c.onHighlight : const Color(0xFFB45309),
      onActiveMark: hc ? c.highlight : c.onHighlight,
      activeMarkDecorated: hc,
      secondaryText: c.muted,
      hintText: c.muted,
      rule: c.faint,
      infoIcon: c.warning,
      blue: c.accent,
      error: c.danger,
    );
  }
}
