import 'package:docfit_ui/features/pdf_edit/pdf_edit_workspace.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('PDF 편집 작업과 결과 경로를 표시한다', (tester) async {
    final calls = <List<Object?>>[];
    Future<Object?> api(String name, List<Object?> args) async {
      calls.add([name, ...args]);
      return {'ok': false, 'error': {'message': '테스트 안내'}};
    }

    await tester.pumpWidget(MaterialApp(home: Scaffold(body: PdfEditWorkspace(api: api))));
    expect(find.text('PDF 편집'), findsOneWidget);
    expect(find.text('PDF 파일 고르기'), findsOneWidget);
    expect(find.text('결과 경로 선택'), findsOneWidget);
    await tester.tap(find.byKey(const Key('pdfEditAction')));
    await tester.pumpAndSettle();
    await tester.tap(find.text('페이지 분할').last);
    await tester.pumpAndSettle();
    expect(find.text('분할할 페이지'), findsOneWidget);
    expect(find.textContaining('원본 파일은 유지'), findsOneWidget);
    await tester.tap(find.byKey(const Key('pdfEditAction')));
    await tester.pumpAndSettle();
    await tester.tap(find.text('PDF 합치기').last);
    await tester.pumpAndSettle();
    expect(tester.widget<OutlinedButton>(find.byKey(const Key('pdfEditPickOutput'))).onPressed, isNotNull);
    expect(calls, isEmpty);
  });
}
