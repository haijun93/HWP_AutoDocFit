import 'package:flutter/material.dart';

import '../../core/theme/app_theme.dart';

/// 표그림판 지역 색(DARK1, 2026-11-13).
///
/// 두 묶음으로 나뉜다.
/// - 캔버스(종이) 위 색: 격자선·선택·손잡이·안내선·인식 겹침. 표그림판 캔버스는 한/글 표가 놓일
///   '흰 종이'를 흉내 내므로(결과 문서와 같게 보여야 함) 테마와 상관없이 흰 바탕을 유지한다.
///   그 위에 그리는 색도 흰 종이 기준이어야 대비가 맞으므로 라이트·다크·고대비 모두 같은 값이다.
///   [TableSketchPainter]에는 생성자 `colors`로 넘긴다.
/// - 주변 도구 막대·패널 색: 테마에 따라 바뀐다. 라이트 값은 원래 리터럴과 같다.
@immutable
class TableSketchColors {
  const TableSketchColors({
    this.grid = _grid,
    this.paperBorder = _paperBorder,
    this.regionMark = _regionMark,
    this.imageTargetFill = _imageTargetFill,
    this.imageTarget = _imageTarget,
    this.selection = _selection,
    this.selectedLine = _selectedLine,
    this.selectionGlow = _selectionGlow,
    this.lassoFill = _lassoFill,
    this.guide = _guide,
    this.handleFill = _handleFill,
    required this.paperShadow,
    required this.snackOk,
    required this.toolSelected,
    required this.infoSoft,
    required this.infoLine,
    required this.errorText,
    required this.warnText,
    required this.warnSoft,
    required this.warnSoftText,
  });

  // 캔버스(흰 종이) 위 색의 값: 테마 무관.
  static const _grid = Color(0xFFE2E8F0);
  static const _paperBorder = Color(0xFFCBD5E1);
  static const _regionMark = Color(0x3316A34A);
  static const _imageTargetFill = Color(0x2216A34A);
  static const _imageTarget = Color(0xFF16A34A);
  static const _selection = Color(0xFF2563EB);
  static const _selectedLine = Color(0xFF1D4ED8);
  static const _selectionGlow = Color(0x553B82F6);
  static const _lassoFill = Color(0x1A2563EB);
  static const _guide = Color(0xFFDC2626);
  static const _handleFill = Color(0xFFFFFFFF);

  /// 종이 바탕(흰색). 결과 한/글 표가 놓이는 종이라 테마 무관.
  static const paper = Color(0xFFFFFFFF); // 문서 색(테마 무관)

  /// 적용 전 인식 결과를 겹칠 때 지금 그림을 흐리게 덮는 흰 막(종이 위).
  static const paperVeil = Color(0xC8FFFFFF);

  /// 적용 전 인식한 선 색('#RRGGBB', 그림판 선 색 문자열).
  static const recognizedLineHex = '#2563EB';

  // --- 캔버스(흰 종이) 위 색 ---
  /// 바둑판 점선 격자.
  final Color grid;
  /// 종이 가장자리 선.
  final Color paperBorder;
  /// '칸 색'으로 고른 칸 표시(연한 초록).
  final Color regionMark;
  /// '칸 그림'으로 고른 칸 바탕·테두리.
  final Color imageTargetFill;
  final Color imageTarget;
  /// 선택 강조(점선 상자·손잡이 테두리·끝점·미리보기 선).
  final Color selection;
  /// 고른 선·화살표 색.
  final Color selectedLine;
  /// 고른 선 둘레 빛·고른 글 칸 바탕.
  final Color selectionGlow;
  /// 올가미·선택 상자 안쪽.
  final Color lassoFill;
  /// 맞춤 안내선(빨간 점선).
  final Color guide;
  /// 크기 조절 손잡이 바탕.
  final Color handleFill;

  // --- 주변 도구 막대·패널 색: 테마에 따라 ---
  /// 종이 그림자.
  final Color paperShadow;
  /// 복사 성공 알림 바탕.
  final Color snackOk;
  /// 고른 도구·레이어 바탕.
  final Color toolSelected;
  /// 선택 줄·인식 띠 바탕(연한 파랑)과 선택 줄 아래 선.
  final Color infoSoft;
  final Color infoLine;
  /// 인식 실패 글.
  final Color errorText;
  /// 인식 경고 글.
  final Color warnText;
  /// 미리보기 경고 상자 바탕과 글.
  final Color warnSoft;
  final Color warnSoftText;

  static const light = TableSketchColors(
    paperShadow: Color(0x1E0F172A),
    snackOk: Color(0xFF16A34A),
    toolSelected: Color(0xFFDBEAFE),
    infoSoft: Color(0xFFEFF6FF),
    infoLine: Color(0xFFBFDBFE),
    errorText: Color(0xFF991B1B),
    warnText: Color(0xFFB45309),
    warnSoft: Color(0xFFFEF3C7),
    warnSoftText: Color(0xFF92400E),
  );

  /// 다크: 바탕은 AppColors.dark 계열, 글자는 그 바탕 위 4.5:1 이상.
  static const dark = TableSketchColors(
    paperShadow: Color(0x66000000),
    // 다크 테마의 알림 글자(onInverseSurface)는 어두운 색이라 밝은 초록 바탕.
    snackOk: Color(0xFF4ADE80),
    toolSelected: Color(0xFF1D2D4A),
    infoSoft: Color(0xFF1C2A44),
    infoLine: Color(0xFF2E4A7A),
    errorText: Color(0xFFFF9C9C),
    warnText: Color(0xFFF0B94A),
    warnSoft: Color(0xFF3A2E14),
    warnSoftText: Color(0xFFF0B94A),
  );

  static TableSketchColors of(BuildContext context) {
    final c = AppColors.of(context);
    if (c.highContrast) {
      return TableSketchColors(
        paperShadow: c.shadow,
        snackOk: c.success,
        toolSelected: c.accentSoft,
        infoSoft: c.surface,
        infoLine: c.line,
        errorText: c.danger,
        warnText: c.warning,
        warnSoft: c.warningSoft,
        warnSoftText: c.warning,
      );
    }
    return c.isDark ? dark : light;
  }

  @override
  bool operator ==(Object other) =>
      other is TableSketchColors &&
      other.grid == grid &&
      other.paperBorder == paperBorder &&
      other.regionMark == regionMark &&
      other.imageTargetFill == imageTargetFill &&
      other.imageTarget == imageTarget &&
      other.selection == selection &&
      other.selectedLine == selectedLine &&
      other.selectionGlow == selectionGlow &&
      other.lassoFill == lassoFill &&
      other.guide == guide &&
      other.handleFill == handleFill &&
      other.paperShadow == paperShadow &&
      other.snackOk == snackOk &&
      other.toolSelected == toolSelected &&
      other.infoSoft == infoSoft &&
      other.infoLine == infoLine &&
      other.errorText == errorText &&
      other.warnText == warnText &&
      other.warnSoft == warnSoft &&
      other.warnSoftText == warnSoftText;

  @override
  int get hashCode => Object.hashAll([
    grid,
    paperBorder,
    regionMark,
    imageTargetFill,
    imageTarget,
    selection,
    selectedLine,
    selectionGlow,
    lassoFill,
    guide,
    handleFill,
    paperShadow,
    snackOk,
    toolSelected,
    infoSoft,
    infoLine,
    errorText,
    warnText,
    warnSoft,
    warnSoftText,
  ]);
}
