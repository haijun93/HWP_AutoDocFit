import 'package:docfit_ui/features/table_sketch/table_sketch_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

/// 자동 저장본은 묻지 않고 바로 복구하고, 알림의 '버리기'로만 되돌린다(사용자 요청 2026-11-07).
void main() {
  Future<List<String>> open(WidgetTester tester) async {
    final calls = <String>[];
    Future<Object?> api(String command, List<Object?> args) async {
      if (command == 'table_sketch') {
        calls.add('${args.first}');
        switch (args.first) {
          case 'load_draft':
            return {
              'exists': true,
              'sketch': {'grid': {'cols': 3, 'rows': 3, 'cell_mm': 5.0}, 'lines': [], 'texts': [], 'fills': []},
            };
          case 'preview':
            return {'ok': true};
        }
      }
      return {'ok': true};
    }

    await tester.pumpWidget(MaterialApp(home: Scaffold(body: TableSketchScreen(api: api))));
    await tester.pumpAndSettle();
    return calls;
  }

  testWidgets('draft is restored automatically without asking', (tester) async {
    final calls = await open(tester);
    expect(find.text('복구 가능한 작업이 있습니다'), findsNothing);
    expect(find.byType(AlertDialog), findsNothing);
    expect(find.byKey(const ValueKey('draft-restored')), findsOneWidget);
    expect(calls, contains('preview'));
    expect(calls, isNot(contains('clear_draft')));
  });

  testWidgets('discard from the notice clears the draft', (tester) async {
    final calls = await open(tester);
    await tester.tap(find.text('버리기'));
    await tester.pumpAndSettle();
    expect(calls, contains('clear_draft'));
  });
}
