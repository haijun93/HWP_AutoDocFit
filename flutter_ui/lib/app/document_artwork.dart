import 'package:flutter/material.dart';

import '../core/theme/app_theme.dart';

/// 첫 화면 문서 그림 색(DARK1). 라이트 값은 예전 하드코딩 값 그대로,
/// 고대비는 AppColors 토큰(시스템 색)으로 대체한다. 장식 그림이라 글자 대비 대상은 아니다.
@immutable
class _ArtworkColors {
  const _ArtworkColors({
    required this.halo,
    required this.backSheet,
    required this.paper,
    required this.paperShadow,
    required this.iconBox,
    required this.icon,
    required this.textLine,
    required this.badge,
    required this.badgeBorder,
    required this.onBadge,
  });

  final Color halo;
  final Color backSheet;
  final Color paper;
  final Color paperShadow;
  final Color iconBox;
  final Color icon;
  final Color textLine;
  final Color badge;
  final Color badgeBorder;
  final Color onBadge;

  static const light = _ArtworkColors(
    halo: Color(0x55ffffff),
    backSheet: Color(0xffb9cdef),
    paper: Color(0xffffffff),
    paperShadow: Color(0x152b527e),
    iconBox: Color(0xffe7efff),
    icon: Color(0xff2563eb),
    textLine: Color(0xffe8edf5),
    badge: Color(0xff287c72),
    badgeBorder: Color(0xffecf6f2),
    onBadge: Color(0xffffffff),
  );

  static const dark = _ArtworkColors(
    halo: Color(0x14ffffff),
    backSheet: Color(0xff2b3d5e),
    paper: Color(0xff2a3442),
    paperShadow: Color(0x66000000),
    iconBox: Color(0xff1c2a44),
    icon: Color(0xff6ea2ff),
    textLine: Color(0xff435064),
    badge: Color(0xff287c72),
    badgeBorder: Color(0xff16302c),
    onBadge: Color(0xffffffff),
  );

  factory _ArtworkColors.highContrast(AppColors c) => _ArtworkColors(
    halo: Colors.transparent,
    backSheet: c.line,
    paper: c.surface,
    paperShadow: c.shadow,
    iconBox: c.surface,
    icon: c.accent,
    textLine: c.faint,
    badge: c.accentFill,
    badgeBorder: c.surface,
    onBadge: c.onAccent,
  );

  static _ArtworkColors of(BuildContext context) {
    final c = context.colors;
    if (c.highContrast) return _ArtworkColors.highContrast(c);
    return c.isDark ? dark : light;
  }
}

class DocumentArtwork extends StatelessWidget {
  const DocumentArtwork({super.key});
  @override
  Widget build(BuildContext context) {
    final a = _ArtworkColors.of(context);
    return ExcludeSemantics(
      child: Stack(
        alignment: Alignment.center,
        children: [
          Container(
            width: 185,
            height: 185,
            decoration: BoxDecoration(shape: BoxShape.circle, color: a.halo),
          ),
          Transform.rotate(
            angle: -.14,
            child: Container(
              width: 121,
              height: 158,
              decoration: BoxDecoration(
                color: a.backSheet,
                borderRadius: BorderRadius.circular(18),
              ),
            ),
          ),
          Transform.translate(
            offset: const Offset(13, -5),
            child: Transform.rotate(
              angle: .08,
              child: Container(
                width: 120,
                height: 158,
                padding: const EdgeInsets.all(19),
                decoration: BoxDecoration(
                  color: a.paper,
                  borderRadius: BorderRadius.circular(18),
                  boxShadow: [
                    BoxShadow(
                      color: a.paperShadow,
                      blurRadius: 24,
                      offset: const Offset(0, 12),
                    ),
                  ],
                ),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Container(
                      width: 28,
                      height: 28,
                      decoration: BoxDecoration(
                        color: a.iconBox,
                        borderRadius: BorderRadius.circular(9),
                      ),
                      child: Icon(
                        Icons.format_align_left_rounded,
                        color: a.icon,
                        size: 17,
                      ),
                    ),
                    const SizedBox(height: 20),
                    for (final w in [78.0, 65.0, 78.0, 43.0])
                      Padding(
                        padding: const EdgeInsets.only(bottom: 9),
                        child: Container(
                          width: w,
                          height: 5,
                          decoration: BoxDecoration(
                            color: a.textLine,
                            borderRadius: BorderRadius.circular(3),
                          ),
                        ),
                      ),
                  ],
                ),
              ),
            ),
          ),
          Positioned(
            right: 17,
            bottom: 18,
            child: Container(
              width: 49,
              height: 49,
              decoration: BoxDecoration(
                color: a.badge,
                borderRadius: BorderRadius.circular(17),
                border: Border.all(color: a.badgeBorder, width: 4),
              ),
              child: Icon(Icons.check_rounded, color: a.onBadge, size: 28),
            ),
          ),
        ],
      ),
    );
  }
}
