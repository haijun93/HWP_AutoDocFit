import 'package:flutter/material.dart';

import '../../core/theme/app_theme.dart';

/// 표 단위(표 템플릿) 화면 지역 색(DARK1). AppColors에 같은 라이트 값이 없는 색만 둔다.
/// 라이트 값 = 기존 리터럴. 다크 글자 색은 AppColors.dark 바탕 위에서 4.5:1 이상.
/// 고대비이면 AppColors 토큰으로 대체한다.
@immutable
class TableUnitsColors {
  const TableUnitsColors({
    required this.errorText,
    required this.emptyText,
    required this.note,
    required this.star,
    required this.favoriteStar,
    required this.badgeSurface,
    required this.selectedSurface,
    required this.dangerText,
    required this.dangerBorder,
    required this.cardShadow,
    required this.paperShadow,
  });

  /// 오류 글자(#f04452).
  final Color errorText;
  /// 빈 목록 안내 글자(#8b95a1).
  final Color emptyText;
  /// 보조 안내 글자(Colors.black54).
  final Color note;
  /// 즐겨찾기 분류 별(#eab308).
  final Color star;
  /// 즐겨찾기 켜짐 별(#f59e0b).
  final Color favoriteStar;
  /// 가변 표 배지 바탕(#eff6ff).
  final Color badgeSurface;
  /// 화살표 고르기 선택 바탕(#dbeafe).
  final Color selectedSurface;
  /// 지우기 단추 글자(#ef4444)와 테두리(#fecaca).
  final Color dangerText;
  final Color dangerBorder;
  /// 서식 카드 그림자(0x0a000000)·표 미리보기 종이 그림자(0x0d000000).
  final Color cardShadow;
  final Color paperShadow;

  static const light = TableUnitsColors(
    errorText: Color(0xfff04452),
    emptyText: Color(0xff8b95a1),
    note: Colors.black54,
    star: Color(0xffeab308),
    favoriteStar: Color(0xfff59e0b),
    badgeSurface: Color(0xffeff6ff),
    selectedSurface: Color(0xffdbeafe),
    dangerText: Color(0xffef4444),
    dangerBorder: Color(0xfffecaca),
    cardShadow: Color(0x0a000000),
    paperShadow: Color(0x0d000000),
  );

  static const dark = TableUnitsColors(
    errorText: Color(0xffff7b7b),
    emptyText: Color(0xff9eabbb),
    note: Color(0xff9eabbb),
    star: Color(0xffeab308),
    favoriteStar: Color(0xfff59e0b),
    badgeSurface: Color(0xff1c2a44),
    selectedSurface: Color(0xff1c2a44),
    dangerText: Color(0xffff7b7b),
    dangerBorder: Color(0xff7f2d2d),
    cardShadow: Color(0x66000000),
    paperShadow: Color(0x66000000),
  );

  static TableUnitsColors of(BuildContext context) {
    final c = AppColors.of(context);
    if (c.highContrast) {
      return TableUnitsColors(
        errorText: c.danger,
        emptyText: c.muted,
        note: c.muted,
        star: c.warning,
        favoriteStar: c.warning,
        badgeSurface: c.accentSoft,
        selectedSurface: c.accentSoft,
        dangerText: c.danger,
        dangerBorder: c.danger,
        cardShadow: c.shadow,
        paperShadow: c.shadow,
      );
    }
    return c.isDark ? dark : light;
  }
}
