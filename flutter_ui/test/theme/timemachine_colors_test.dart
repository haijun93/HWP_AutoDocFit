import 'package:docfit_ui/core/theme/app_theme.dart';
import 'package:docfit_ui/core/theme/system_colors.dart';
import 'package:docfit_ui/features/timemachine/timemachine_colors.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

double _contrast(Color a, Color b) {
  final la = a.computeLuminance(), lb = b.computeLuminance();
  final hi = la > lb ? la : lb, lo = la > lb ? lb : la;
  return (hi + 0.05) / (lo + 0.05);
}

Future<TimemachineColors> _colorsIn(WidgetTester tester, ThemeData theme) async {
  late TimemachineColors out;
  await tester.pumpWidget(MaterialApp(
    theme: theme,
    home: Builder(builder: (context) {
      out = TimemachineColors.of(context);
      return const SizedBox();
    }),
  ));
  return out;
}

void main() {
  const sys = SystemColors(
    window: Color(0xff000000),
    windowText: Color(0xffffffff),
    highlight: Color(0xff1aebff),
    highlightText: Color(0xff000000),
    buttonFace: Color(0xff000000),
    grayText: Color(0xff3ff23f),
    hotlight: Color(0xffffff00),
  );

  testWidgets('고대비: 지금 일치는 일반 일치와 색·모양이 다르다', (tester) async {
    final c = AppColors.highContrast(sys);
    final t = await _colorsIn(tester, buildAppTheme(c, KioskColors.highContrast(sys)));
    // 일반 일치 = (highlight 바탕, onMark 글자), 지금 일치 = 뒤집은 쌍.
    expect(t.activeMark, isNot(c.highlight));
    expect(t.activeMark, sys.highlightText);
    expect(t.onActiveMark, sys.highlight);
    expect(t.onMark, sys.highlightText);
    expect(_contrast(t.activeMark, t.onActiveMark), greaterThanOrEqualTo(4.5));
    final style = t.activeMarkStyle;
    expect(style.fontWeight, FontWeight.w700);
    expect(style.decoration, TextDecoration.underline);
  });

  testWidgets('라이트: 예전 색 그대로, 장식 없음', (tester) async {
    final t = await _colorsIn(tester, buildAppTheme(AppColors.light, KioskColors.light));
    expect(t.activeMark, Colors.orangeAccent);
    expect(t.onActiveMark, Colors.black);
    expect(t.activeMarkStyle.decoration, isNull);
    expect(t.activeMarkStyle.fontWeight, isNull);
  });

  testWidgets('다크: 지금 일치와 일반 일치 바탕이 다르고 글자 대비 4.5:1 이상', (tester) async {
    final t = await _colorsIn(tester, buildAppTheme(AppColors.dark, KioskColors.dark));
    expect(t.activeMark, isNot(AppColors.dark.highlight));
    expect(_contrast(t.activeMark, t.onActiveMark), greaterThanOrEqualTo(4.5));
  });
}
