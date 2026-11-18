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

  testWidgets('PDF page metadata determines card count and failed renders keep numbered cards', (tester) async {
    tester.binding.window.physicalSizeTestValue = const Size(1024, 768);
    tester.binding.window.devicePixelRatioTestValue = 1;
    addTearDown(tester.binding.window.clearPhysicalSizeTestValue);
    addTearDown(tester.binding.window.clearDevicePixelRatioTestValue);
    final actions = <String>[];
    Future<Object?> api(String name, List<Object?> args) async {
      final payload = args.single as Map;
      final action = payload['action'] as String;
      actions.add(action);
      if (action == 'info') {
        return {'ok': true, 'data': {'page_count': 2, 'pages': [{'number': 1}, {'number': 2}]}};
      }
      return {'ok': false, 'error': {'message': 'render unavailable'}};
    }
    await tester.pumpWidget(MaterialApp(home: Scaffold(body: PdfEditWorkspace(api: api))));
    final target = tester.widget<DragTarget<String>>(find.byType(DragTarget<String>));
    target.onAcceptWithDetails!(DragTargetDetails(data: 'sample.pdf', offset: Offset.zero));
    await tester.pump();
    await tester.pumpAndSettle();
    expect(find.byKey(const Key('pdfEditThumbnailGrid')), findsOneWidget);
    expect(find.text('2'), findsOneWidget);
    expect(find.text('3'), findsNothing);
    expect(actions, contains('info'));
    expect(actions, contains('thumbnail'));
  });

}
