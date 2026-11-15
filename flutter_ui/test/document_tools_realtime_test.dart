import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:docfit_ui/features/tools/document_tools_workspace.dart';

Finder field(String label) => find.byWidgetPredicate(
  (widget) => widget is TextField && widget.decoration?.labelText == label,
);

void main() {
  testWidgets('realtime conversion triggers automatically with 150ms debounce', (
    tester,
  ) async {
    final calls = <String>[];
    await tester.pumpWidget(
      MaterialApp(
        home: DocumentToolsWorkspace(
          api: (method, args) async {
            calls.add('${args.first}');
            return {
              'ok': true,
              'data': {'text': '□ 실시간 변환 완료'},
            };
          },
        ),
      ),
    );

    // Enter text
    await tester.enterText(field('입력'), '# 자동변환 테스트');
    await tester.pump();

    // After 100ms, debounce should not have fired yet
    await tester.pump(const Duration(milliseconds: 100));
    expect(calls, isEmpty);

    // After additional 60ms (total 160ms > 150ms), debounce triggers call
    await tester.pump(const Duration(milliseconds: 60));
    expect(calls, ['convert']);

    // Check that output reflects converted text
    expect(
      tester.widget<TextField>(field('변환 결과')).controller!.text,
      '□ 실시간 변환 완료',
    );
  });

  testWidgets('earlier slow response does not overwrite latest result', (
    tester,
  ) async {
    final completers = <Completer<Map>>[];
    await tester.pumpWidget(
      MaterialApp(
        home: DocumentToolsWorkspace(
          api: (method, args) async {
            final c = Completer<Map>();
            completers.add(c);
            return c.future;
          },
        ),
      ),
    );

    // First input
    await tester.enterText(field('입력'), '첫 번째 입력');
    await tester.pump(const Duration(milliseconds: 200));
    expect(completers.length, 1);

    // Second input before first completes
    await tester.enterText(field('입력'), '두 번째 입력');
    await tester.pump(const Duration(milliseconds: 200));
    expect(completers.length, 2);

    // Complete second (latest) request first
    completers[1].complete({
      'ok': true,
      'data': {'text': '두 번째 최신 결과'},
    });
    await tester.pumpAndSettle();
    expect(
      tester.widget<TextField>(field('변환 결과')).controller!.text,
      '두 번째 최신 결과',
    );

    // Complete first (slow) request later
    completers[0].complete({
      'ok': true,
      'data': {'text': '첫 번째 오래된 결과'},
    });
    await tester.pumpAndSettle();

    // Latest result should NOT be overwritten!
    expect(
      tester.widget<TextField>(field('변환 결과')).controller!.text,
      '두 번째 최신 결과',
    );
  });

  testWidgets('multiple options pipeline applied together', (
    tester,
  ) async {
    final calls = <String>[];
    final payloads = <Map>[];
    await tester.pumpWidget(
      MaterialApp(
        home: DocumentToolsWorkspace(
          api: (method, args) async {
            calls.add('${args.first}');
            payloads.add(args[1] as Map);
            return {
              'ok': true,
              'data': {'text': '금100,000,000원(금일억원)'},
            };
          },
        ),
      ),
    );

    // Select extra options: '숫자 쉼표', '금액 한글화'
    await tester.tap(find.text('숫자 쉼표'));
    await tester.pump();
    await tester.tap(find.text('금액 한글화'));
    await tester.pump();

    // Enter text
    await tester.enterText(field('입력'), '예산 100000000원');
    await tester.pump(const Duration(milliseconds: 200));

    // Pipeline should be called with active options
    expect(calls.last, 'pipeline');
    final options = (payloads.last['options'] as List).cast<String>();
    expect(options, containsAll(['convert', 'commas', 'amount']));
  });

  testWidgets('input exceeding 200KB halts auto conversion and shows warning', (
    tester,
  ) async {
    final calls = <String>[];
    await tester.pumpWidget(
      MaterialApp(
        home: DocumentToolsWorkspace(
          api: (method, args) async {
            calls.add('${args.first}');
            return {
              'ok': true,
              'data': {'text': '수동 변환 결과'},
            };
          },
        ),
      ),
    );

    // Enter large text over 200KB (e.g. 75,000 korean chars = 225,000 bytes)
    final largeText = '대용량 텍스트' * 15000;
    await tester.enterText(field('입력'), largeText);
    await tester.pump(const Duration(milliseconds: 300));

    // Auto conversion must NOT be triggered
    expect(calls, isEmpty);
    expect(find.textContaining('입력이 커서 자동 변환을 멈췄습니다'), findsOneWidget);

    // Manual run still works
    await tester.tap(find.text('실행 / 미리보기'));
    await tester.pumpAndSettle();
    expect(calls, ['convert']);
    expect(
      tester.widget<TextField>(field('변환 결과')).controller!.text,
      '수동 변환 결과',
    );
  });

  testWidgets('large 50000 character text converts smoothly without lag', (
    tester,
  ) async {
    final calls = <String>[];
    await tester.pumpWidget(
      MaterialApp(
        home: DocumentToolsWorkspace(
          api: (method, args) async {
            calls.add('${args.first}');
            return {
              'ok': true,
              'data': {'text': '5만자 변환 완료'},
            };
          },
        ),
      ),
    );

    // 50,000 characters of ascii text (50KB < 200KB)
    final text50k = 'A' * 50000;
    await tester.enterText(field('입력'), text50k);
    await tester.pump(const Duration(milliseconds: 200));

    // Auto conversion triggers for 50k characters
    expect(calls, ['convert']);
    expect(
      tester.widget<TextField>(field('변환 결과')).controller!.text,
      '5만자 변환 완료',
    );
  });
}
