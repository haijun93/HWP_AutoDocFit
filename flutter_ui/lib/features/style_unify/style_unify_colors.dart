import 'package:flutter/material.dart';

import '../../core/theme/app_theme.dart';

/// 서식 통일 대화상자 지역 팔레트(DARK1). 라이트 값은 원래 Material 기본색
/// (Colors.red·green·orange) 그대로. 다크 글자는 대화상자 바탕에서 4.5:1 이상.
@immutable
class StyleUnifyColors {
  const StyleUnifyColors({
    required this.error,
    required this.success,
    required this.warning,
  });

  /// 오류 글자·미확정 문단 제목(Colors.red).
  final Color error;
  /// 결과 상자 성공 테두리·아이콘(Colors.green). 바탕은 alpha 25로 옅게 쓴다.
  final Color success;
  /// 결과 상자 경고 테두리·아이콘(Colors.orange).
  final Color warning;

  static const light = StyleUnifyColors(
    error: Color(0xfff44336),
    success: Color(0xff4caf50),
    warning: Color(0xffff9800),
  );

  static const dark = StyleUnifyColors(
    error: Color(0xffff7b7b),
    success: Color(0xff4ade80),
    warning: Color(0xfff0b94a),
  );

  static StyleUnifyColors of(BuildContext context) {
    final c = AppColors.of(context);
    if (c.highContrast) {
      return StyleUnifyColors(
        error: c.danger,
        success: c.success,
        warning: c.warning,
      );
    }
    return c.isDark ? dark : light;
  }
}
