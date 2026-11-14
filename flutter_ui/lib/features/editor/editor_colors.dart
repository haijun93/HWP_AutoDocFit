import 'package:flutter/material.dart';

import '../../core/theme/app_theme.dart';

/// 메모 편집기 지역 색(DARK1). 라이트 값 = 기존 리터럴 그대로.
/// 다크는 AppColors.dark 바탕 위에서 글자 4.5:1 이상, 고대비는 AppColors 토큰으로 대체.
@immutable
class EditorColors {
  const EditorColors({required this.findMark, required this.secondaryText, required this.error});

  /// 찾은 구간 바탕(그 위 글자는 기본 글자색).
  final Color findMark;
  /// black54 안내 글자.
  final Color secondaryText;
  final Color error;

  static const light = EditorColors(
    findMark: Color(0x66FFC107),
    secondaryText: Colors.black54,
    error: Colors.red,
  );

  static EditorColors of(BuildContext context) {
    final c = AppColors.of(context);
    if (!c.isDark && !c.highContrast) return light;
    return EditorColors(
      findMark: c.highContrast ? c.highlight : const Color(0xFF5C4A12),
      secondaryText: c.muted,
      error: c.danger,
    );
  }
}
