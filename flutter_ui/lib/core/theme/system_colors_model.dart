import 'package:flutter/painting.dart';

/// 고대비 테마에 쓰는 Windows 시스템 색 묶음.
class SystemColors {
  const SystemColors({
    required this.window,
    required this.windowText,
    required this.highlight,
    required this.highlightText,
    required this.buttonFace,
    required this.grayText,
    required this.hotlight,
  });
  final Color window;
  final Color windowText;
  final Color highlight;
  final Color highlightText;
  final Color buttonFace;
  final Color grayText;
  final Color hotlight;

  /// COLORREF(0x00BBGGRR) → Color.
  static Color fromColorRef(int ref) => Color.fromARGB(
    0xff,
    ref & 0xff,
    (ref >> 8) & 0xff,
    (ref >> 16) & 0xff,
  );
}
