import 'package:docfit_ui/features/settings/idiom_panel.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('앱 정보에 구성요소·라이선스·GitHub 저장소를 보인다', (tester) async {
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
    expect(find.text('연계 오픈소스 및 저장소'), findsOneWidget);
    expect(find.text('구성요소'), findsOneWidget);
    expect(find.text('라이선스'), findsOneWidget);
    expect(find.text('프로젝트'), findsOneWidget);
    expect(find.text('앱 GitHub 저장소'), findsOneWidget);
    expect(find.text('kordoc 4.21.30 (chrisryugj)'), findsOneWidget);
    expect(find.text('Node.js 22.23.3'), findsOneWidget);
    expect(thirdPartyComponents.every((c) => c.license.isNotEmpty), isTrue);
    expect(
      thirdPartyComponents.every(
        (c) => c.repository.startsWith('https://github.com/'),
      ),
      isTrue,
    );
  });
}
