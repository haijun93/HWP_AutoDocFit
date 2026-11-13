import 'package:flutter/material.dart';

import '../../core/theme/app_theme.dart';

/// 준말 화면 지역 색(DARK1). 라이트 값 = 기존 리터럴 그대로.
/// 다크·고대비는 AppColors 토큰으로 대체.
@immutable
class AbbreviationColors {
  const AbbreviationColors({required this.error, required this.border});

  final Color error;
  /// 문서 준말 칸 테두리(Colors.black12).
  final Color border;

  static const light = AbbreviationColors(error: Colors.red, border: Colors.black12);

  static AbbreviationColors of(BuildContext context) {
    final c = AppColors.of(context);
    if (!c.isDark && !c.highContrast) return light;
    return AbbreviationColors(error: c.danger, border: c.line);
  }
}
