import 'package:flutter/material.dart';

import '../../core/theme/app_theme.dart';

/// 파일변환 화면(보라 계열) 지역 팔레트(DARK1). 라이트 값은 원래 리터럴 그대로.
/// 다크 글자/바탕 쌍은 WCAG 4.5:1 이상. 고대비는 AppColors 토큰으로 대체한다.
@immutable
class ConversionColors {
  const ConversionColors({
    required this.canvas,
    required this.dropSurface,
    required this.dropBorder,
    required this.frameBorder,
    required this.dropAccent,
    required this.dropAccentFill,
    required this.accent,
    required this.accentFill,
    required this.centerCell,
    required this.cellEnabled,
    required this.keyText,
    required this.keyDisabledText,
    required this.keySelected,
    required this.keyDisabled,
    required this.keyShadow,
    required this.done,
    required this.imageFile,
    required this.notice,
  });

  /// 화면 바탕.
  final Color canvas;
  /// 끌어 놓기 칸 바탕·테두리.
  final Color dropSurface;
  final Color dropBorder;
  /// 3단 작업 칸 테두리.
  final Color frameBorder;
  /// 끌어 놓기 화면 강조(아이콘)와 단추 바탕(흰 글자).
  final Color dropAccent;
  final Color dropAccentFill;
  /// 변환 강조(아이콘·진행 막대)와 단추·선택 키 바탕(흰 글자).
  final Color accent;
  final Color accentFill;
  /// 포맷 칸 가운데 칸·고를 수 있는 칸 바탕.
  final Color centerCell;
  final Color cellEnabled;
  /// 키패드 글자(고를 수 있음/없음)와 선택·비활성 키 바탕, 키 그림자.
  final Color keyText;
  final Color keyDisabledText;
  final Color keySelected;
  final Color keyDisabled;
  final Color keyShadow;
  /// 변환 완료 표시(초록), 합치기 목록의 그림 파일 아이콘, 고를 수 없는 이유 글자.
  final Color done;
  final Color imageFile;
  final Color notice;

  static const light = ConversionColors(
    canvas: Color(0xfff6f7fc),
    dropSurface: Color(0xfff4f1ff),
    dropBorder: Color(0xffb9a8e0),
    frameBorder: Color(0xffcfc6f3),
    dropAccent: Color(0xff7656a8),
    dropAccentFill: Color(0xff7656a8),
    accent: Color(0xff7664ec),
    accentFill: Color(0xff7664ec),
    centerCell: Color(0xffeef2ff),
    cellEnabled: Color(0xfff8f7ff),
    keyText: Color(0xff25324a),
    keyDisabledText: Color(0xff8a94a6),
    keySelected: Color(0xffe9e5ff),
    keyDisabled: Color(0xffedf0f5),
    keyShadow: Color(0x500f172a),
    done: Color(0xff16a34a),
    imageFile: Color(0xff059669),
    notice: Color(0xff9a3412),
  );

  static const dark = ConversionColors(
    canvas: Color(0xff0f141a),
    dropSurface: Color(0xff241e36),
    dropBorder: Color(0xff5a4c86),
    frameBorder: Color(0xff3a3258),
    dropAccent: Color(0xffbca6ec),
    dropAccentFill: Color(0xff6d4fb0),
    accent: Color(0xffb4a8ff),
    accentFill: Color(0xff6650e0),
    centerCell: Color(0xff1d2540),
    cellEnabled: Color(0xff1f1b33),
    keyText: Color(0xffe2e8f0),
    keyDisabledText: Color(0xff98a4b5),
    keySelected: Color(0xff2f2747),
    keyDisabled: Color(0xff1b222c),
    keyShadow: Color(0x66000000),
    done: Color(0xff4ade80),
    imageFile: Color(0xff34d399),
    notice: Color(0xfffdba74),
  );

  static ConversionColors of(BuildContext context) {
    final c = AppColors.of(context);
    if (c.highContrast) {
      return ConversionColors(
        canvas: c.canvas,
        dropSurface: c.surface,
        dropBorder: c.line,
        frameBorder: c.line,
        dropAccent: c.accent,
        dropAccentFill: c.accentFill,
        accent: c.accent,
        accentFill: c.accentFill,
        centerCell: c.surfaceSubtle,
        cellEnabled: c.surface,
        keyText: c.text,
        keyDisabledText: c.faint,
        keySelected: c.surfaceSubtle,
        keyDisabled: c.surface,
        keyShadow: c.shadow,
        done: c.success,
        imageFile: c.text,
        notice: c.text,
      );
    }
    return c.isDark ? dark : light;
  }
}
