import 'package:flutter/material.dart';

import '../../core/theme/app_theme.dart';

/// 토스 키오스크 느낌의 설정 화면 글자·간격 규칙.
/// 큰 글자, 넉넉한 여백, 둥근 카드, 한 가지 강조색(파랑)만 쓴다.
/// 색은 테마 토큰 [KioskColors](`context.kiosk`)에서 읽는다(DARK1).
class Kiosk {
  static const radius = 20.0;

  static TextStyle title(BuildContext context) => TextStyle(
    fontSize: 22,
    fontWeight: FontWeight.w700,
    color: context.kiosk.ink,
    height: 1.35,
  );
  static TextStyle cardTitle(BuildContext context) => TextStyle(
    fontSize: 18,
    fontWeight: FontWeight.w700,
    color: context.kiosk.ink,
    height: 1.35,
  );
  static TextStyle body(BuildContext context) =>
      TextStyle(fontSize: 15, color: context.kiosk.sub, height: 1.5);
  static TextStyle caption(BuildContext context) =>
      TextStyle(fontSize: 13, color: context.kiosk.faint, height: 1.4);

  static BoxDecoration cardBox(BuildContext context, {bool selected = false}) {
    final k = context.kiosk;
    return BoxDecoration(
      color: k.card,
      borderRadius: BorderRadius.circular(radius),
      border: Border.all(
        color: selected ? k.blue : k.line,
        width: selected ? 2 : 1,
      ),
    );
  }
}

/// 큼직한 파란 기본 버튼(키오스크의 '다음' 버튼).
class KioskPrimaryButton extends StatelessWidget {
  const KioskPrimaryButton({
    super.key,
    required this.label,
    required this.onPressed,
  });
  final String label;
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    final k = context.kiosk;
    return FilledButton(
      style: FilledButton.styleFrom(
        backgroundColor: k.blueFill,
        foregroundColor: k.onBlue,
        disabledBackgroundColor: k.disabledFill,
        minimumSize: const Size(160, 52),
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(14)),
        textStyle: const TextStyle(fontSize: 16, fontWeight: FontWeight.w700),
      ),
      onPressed: onPressed,
      child: Text(label),
    );
  }
}

/// 회색 보조 버튼.
class KioskSecondaryButton extends StatelessWidget {
  const KioskSecondaryButton({
    super.key,
    required this.label,
    required this.onPressed,
  });
  final String label;
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    final k = context.kiosk;
    return TextButton(
      style: TextButton.styleFrom(
        backgroundColor: k.secondaryFill,
        foregroundColor: k.sub,
        minimumSize: const Size(110, 52),
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(14)),
        textStyle: const TextStyle(fontSize: 15, fontWeight: FontWeight.w600),
      ),
      onPressed: onPressed,
      child: Text(label),
    );
  }
}
