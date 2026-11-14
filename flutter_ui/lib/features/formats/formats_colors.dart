import 'package:flutter/material.dart';

import '../../core/theme/app_theme.dart';

/// 서식 화면에서 [AppColors] 토큰에 없는 색(DARK1).
/// 라이트 값은 원래 하드코딩 값과 같다. 다크 값은 [AppColors.dark] 바탕 계열이고,
/// 글자/바탕 쌍은 4.5:1 이상이다. 고대비는 [AppColors] 시스템 색으로 대체한다.
@immutable
class FormatsColors {
  const FormatsColors({
    required this.noticeFill,
    required this.noticeLine,
    required this.noticeText,
    required this.errorLine,
  });

  /// 안내 상자 바탕(#eff6ff)·테두리(#bfdbfe)·글자(#1e3a8a).
  final Color noticeFill;
  final Color noticeLine;
  final Color noticeText;

  /// 오류 상자 테두리(#fca5a5).
  final Color errorLine;

  static const light = FormatsColors(
    noticeFill: Color(0xffeff6ff),
    noticeLine: Color(0xffbfdbfe),
    noticeText: Color(0xff1e3a8a),
    errorLine: Color(0xfffca5a5),
  );

  static const dark = FormatsColors(
    noticeFill: Color(0xff1c2a44),
    noticeLine: Color(0xff2c4a7a),
    noticeText: Color(0xffbcd4ff),
    errorLine: Color(0xff7a3a3f),
  );

  static FormatsColors of(BuildContext context) {
    final c = AppColors.of(context);
    if (c.highContrast) {
      return FormatsColors(
        noticeFill: c.surface,
        noticeLine: c.line,
        noticeText: c.text,
        errorLine: c.line,
      );
    }
    return c.isDark ? dark : light;
  }
}
