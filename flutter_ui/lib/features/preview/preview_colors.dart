import 'package:flutter/material.dart';

import '../../core/theme/app_theme.dart';

/// 옵션 예시 그림 둘레 지역 팔레트(DARK1). 라이트 값은 원래 Colors.black54 그대로.
/// 다크 글자는 카드·대화상자 바탕(171d26·212a35)에서 6.2:1 이상.
@immutable
class PreviewColors {
  const PreviewColors({required this.caption});

  /// 그림 설명·'예시 이미지 준비 중' 글자.
  final Color caption;

  static const light = PreviewColors(caption: Color(0x8a000000));
  static const dark = PreviewColors(caption: Color(0xff9eabbb));

  static PreviewColors of(BuildContext context) {
    final c = AppColors.of(context);
    if (c.highContrast) return PreviewColors(caption: c.muted);
    return c.isDark ? dark : light;
  }
}
