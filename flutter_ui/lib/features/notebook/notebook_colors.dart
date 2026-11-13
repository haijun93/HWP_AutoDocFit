import 'package:flutter/material.dart';

import '../../core/theme/app_theme.dart';

/// 업무자료함(노트) 화면 지역 색(DARK1). 라이트 값 = 기존 리터럴 그대로.
/// 다크는 AppColors.dark 바탕 위에서 글자 4.5:1 이상, 고대비는 AppColors 토큰으로 대체.
@immutable
class NotebookColors {
  const NotebookColors({
    required this.mark,
    required this.secondaryText,
    required this.hintText,
    required this.error,
  });

  /// 검색어 표시·미리보기 현재 문단 바탕(그 위 글자는 기본 글자색).
  final Color mark;
  /// black54·black45 글자.
  final Color secondaryText;
  final Color hintText;
  final Color error;

  static const light = NotebookColors(
    mark: Color(0xFFFFF3BF),
    secondaryText: Colors.black54,
    hintText: Colors.black45,
    error: Colors.red,
  );

  static NotebookColors of(BuildContext context) {
    final c = AppColors.of(context);
    if (!c.isDark && !c.highContrast) return light;
    return NotebookColors(
      mark: c.highContrast ? c.surfaceSubtle : const Color(0xFF3D3415),
      secondaryText: c.muted,
      hintText: c.faint,
      error: c.danger,
    );
  }
}
