import 'package:flutter/material.dart';

import '../../core/theme/app_theme.dart';

/// 몽땅클립보드 지역 색(DARK1). 라이트 값 = 기존 리터럴 그대로.
/// 다크·고대비는 AppColors 토큰으로 대체.
@immutable
class KclipColors {
  const KclipColors({required this.previewBorder, required this.recording});

  /// 그림 미리보기 테두리(Colors.grey.shade400).
  final Color previewBorder;
  /// 기록 중 표시 점.
  final Color recording;

  static const light = KclipColors(
    previewBorder: Color(0xFFBDBDBD), // Colors.grey.shade400
    recording: Colors.red,
  );

  static KclipColors of(BuildContext context) {
    final c = AppColors.of(context);
    if (!c.isDark && !c.highContrast) return light;
    return KclipColors(previewBorder: c.lineStrong, recording: c.danger);
  }
}
