import 'package:docfit_ui/features/settings/idiom_panel.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('앱 정보에 함께 쓰는 프로그램·코드와 라이선스를 보인다', (tester) async {
    tester.view.physicalSize = const Size(1000, 2400);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    await tester.pumpWidget(
      const MaterialApp(
        home: Scaffold(body: AppInfoPanel(version: '1.76.5')),
      ),
    );
    expect(find.text('도토리만두'), findsOneWidget);
    expect(find.text('1.76.5'), findsOneWidget);
    expect(find.text('함께 쓰는 프로그램·코드'), findsOneWidget);
    expect(find.text('kordoc 4.24.9 (chrisryugj)'), findsOneWidget);
    expect(find.text('Node.js 22.23.3'), findsOneWidget);
    expect(thirdPartyComponents.every((c) => c.$3.isNotEmpty), isTrue);
  });

  testWidgets('앱 정보에 개발자 모드 연결 프로그램 3종(포크·원본·라이선스)을 보인다', (tester) async {
    await tester.pumpWidget(const MaterialApp(home: Scaffold(body: AppInfoPanel(version: 'v2.2.40'))));
    await tester.scrollUntilVisible(find.byKey(const ValueKey('about-external-programs')), 300);
    await tester.pumpAndSettle();
    expect(find.text('개발자 모드 연결 프로그램'), findsOneWidget);
    expect(find.textContaining('사진첩 — mongdangview'), findsOneWidget);
    expect(find.textContaining('아웃라이너 — mongdangoutliner'), findsOneWidget);
    expect(find.textContaining('사진관리 — mondangpics'), findsOneWidget);
    expect(find.text('AGPL-3.0'), findsOneWidget);
    expect(find.text('GPL-2.0'), findsOneWidget);
  });
}
