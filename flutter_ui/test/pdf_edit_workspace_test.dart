import 'package:docfit_ui/features/pdf_edit/pdf_edit_workspace.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('PDF 편집 드롭 영역과 비활성 실행 상태를 표시한다', (tester) async {
    tester.binding.window.physicalSizeTestValue = const Size(1024, 768);
    tester.binding.window.devicePixelRatioTestValue = 1;
    addTearDown(tester.binding.window.clearPhysicalSizeTestValue);
    addTearDown(tester.binding.window.clearDevicePixelRatioTestValue);
    final calls = <List<Object?>>[];
    Future<Object?> api(String name, List<Object?> args) async {
      calls.add([name, ...args]);
      return {'ok': false, 'error': {'message': '테스트 안내'}};
    }

    await tester.pumpWidget(MaterialApp(home: Scaffold(body: PdfEditWorkspace(api: api))));
    expect(find.text('PDF 편집'), findsOneWidget);
    expect(find.text('파일 선택'), findsOneWidget);
    expect(find.byKey(const Key('pdfEditDropZone')), findsOneWidget);
    expect(find.text('PDF를 여기로 끌어다 놓거나 클릭해서 선택'), findsOneWidget);
    expect(tester.widget<FilledButton>(find.byKey(const Key('pdfEditRun'))).onPressed, isNull);
    expect(find.byKey(const Key('pdfEditTool_페이지삭제')), findsOneWidget);
    expect(calls, isEmpty);
  });

  testWidgets('작업 도구 선택 상태를 강조한다', (tester) async {
    Future<Object?> api(String name, List<Object?> args) async => {'ok': true};
    await tester.pumpWidget(MaterialApp(home: Scaffold(body: PdfEditWorkspace(api: api))));
    await tester.tap(find.byKey(const Key('pdfEditTool_회전')));
    await tester.pumpAndSettle();
    final chip = tester.widget<InkWell>(find.byKey(const Key('pdfEditTool_회전')));
    expect(chip.onTap, isNotNull);
    expect(find.text('회전'), findsOneWidget);
  });
}
