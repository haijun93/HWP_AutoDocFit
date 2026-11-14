import 'package:flutter/material.dart';

import '../../core/theme/app_theme.dart';

/// 서재(자료함) 화면 지역 색(DARK1). 라이트 값 = 기존 리터럴 그대로.
/// 다크는 AppColors.dark 바탕(0f141a·171d26·1b222c·212a35) 위에서 글자 4.5:1 이상,
/// 고대비는 AppColors(시스템 색) 토큰으로 대체한다.
@immutable
class LibraryColors {
  const LibraryColors({
    required this.selectedFill,
    required this.selectedBorder,
    required this.badgeFill,
    required this.hoverFill,
    required this.rowSelectedFill,
    required this.rowSelectedBorder,
    required this.focusBorder,
    required this.missingIcon,
    required this.secondaryText,
    required this.hintText,
    required this.disabledText,
    required this.bodyText,
    required this.border,
    required this.borderStrong,
    required this.error,
    required this.relationLine,
    required this.headerFill,
    required this.spaceCore,
    required this.spaceBasis,
    required this.spaceReference,
    required this.spaceTemp,
  });

  /// 선택된 카드·선택 막대·끌어 놓기 대상 바탕.
  final Color selectedFill;
  /// 선택된 카드 테두리·선택 아이콘. 라이트는 `Colors.blue`(MaterialColor) 그대로(시험이 비교).
  final Color selectedBorder;
  /// 여러 개 끌기 개수 표시 바탕(그 위 글자는 AppColors.onAccent).
  final Color badgeFill;
  /// 끌기·마우스 올림 바탕.
  final Color hoverFill;
  /// 탐색기 선택 줄 바탕·테두리.
  final Color rowSelectedFill;
  final Color rowSelectedBorder;
  /// 탐색기 초점 줄 테두리.
  final Color focusBorder;
  /// 원본 없음 경고 아이콘.
  final Color missingIcon;
  /// black54·black45·black38·black87 글자.
  final Color secondaryText;
  final Color hintText;
  final Color disabledText;
  final Color bodyText;
  /// black12·black26 테두리.
  final Color border;
  final Color borderStrong;
  final Color error;
  /// 관계선(CustomPainter).
  final Color relationLine;
  /// 탐색기 머리줄·상태 줄 바탕.
  final Color headerFill;
  /// 가상 폴더별 바탕.
  final Color spaceCore;
  final Color spaceBasis;
  final Color spaceReference;
  final Color spaceTemp;

  static const light = LibraryColors(
    selectedFill: Color(0xFFE3F2FD),
    selectedBorder: Colors.blue,
    badgeFill: Colors.blue,
    hoverFill: Color(0xFFBBDEFB),
    rowSelectedFill: Color(0xFFCCE8FF),
    rowSelectedBorder: Color(0xFF99D1FF),
    focusBorder: Color(0xFF90CAF9), // Colors.blue.shade200
    missingIcon: Colors.orange,
    secondaryText: Colors.black54,
    hintText: Colors.black45,
    disabledText: Colors.black38,
    bodyText: Colors.black87,
    border: Colors.black12,
    borderStrong: Colors.black26,
    error: Colors.red,
    relationLine: Colors.indigo,
    headerFill: Color(0xFFF7F7F7),
    spaceCore: Color(0xFFF0F5FF),
    spaceBasis: Color(0xFFF4F7F0),
    spaceReference: Color(0xFFFFF8EA),
    spaceTemp: Color(0xFFF5F1F8),
  );

  static LibraryColors of(BuildContext context) {
    final c = AppColors.of(context);
    if (!c.isDark && !c.highContrast) return light;
    final hc = c.highContrast;
    return LibraryColors(
      selectedFill: c.accentSoft,
      selectedBorder: c.accent,
      badgeFill: c.accentFill,
      hoverFill: hc ? c.surfaceSubtle : const Color(0xFF24365A),
      rowSelectedFill: hc ? c.surfaceSubtle : const Color(0xFF1F3A5F),
      rowSelectedBorder: hc ? c.accent : const Color(0xFF4A7CC4),
      focusBorder: c.accent,
      missingIcon: c.warning,
      secondaryText: c.muted,
      hintText: c.faint,
      disabledText: c.faint,
      bodyText: c.text,
      border: c.line,
      borderStrong: c.lineStrong,
      error: c.danger,
      relationLine: hc ? c.accent : const Color(0xFF8C9EFF),
      headerFill: c.surfaceMuted,
      spaceCore: hc ? c.surface : const Color(0xFF172235),
      spaceBasis: hc ? c.surface : const Color(0xFF1A2419),
      spaceReference: hc ? c.surface : const Color(0xFF2E2415),
      spaceTemp: hc ? c.surface : const Color(0xFF241E36),
    );
  }
}
