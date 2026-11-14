import 'package:docfit_ui/features/timemachine/search_engine_status.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('all explicit transitions have labels; absent status is unknown', () {
    expect(searchEngineStatusText({}), isNull);
    for (final phase in ['legacy', 'ready', 'pending', 'seeding', 'fallback']) {
      expect(searchEngineStatusText({'search_engine_status': {'transition': phase}}), isNotNull);
    }
  });
  testWidgets('fallback reason is inspectable and legacy is visible', (tester) async {
    await tester.pumpWidget(const MaterialApp(home: Scaffold(body: SearchEngineStatus(status: {
      'search_engine_status': {'transition':'fallback', 'engine_reason':'service_unavailable'},
    }))));
    expect(find.text('검색: 기존 엔진으로 자동 대체'), findsOneWidget);
    expect(find.byTooltip('service_unavailable'), findsOneWidget);
  });
}
