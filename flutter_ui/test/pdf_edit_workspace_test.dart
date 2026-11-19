import 'dart:async';

import 'package:docfit_ui/features/pdf_edit/pdf_edit_workspace.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
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

  testWidgets('쪽 클릭, Shift 범위, Ctrl 토글과 전체 선택 및 해제를 반영한다', (tester) async {
    await _loadPdf(tester, (name, args) async => _pdfResponse(args));
    await _tapCard(tester, 1);
    await tester.pump();
    expect(find.byTooltip('왼쪽 90° 회전'), findsOneWidget);

    await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
    await _tapCard(tester, 3);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
    await tester.pump();
    expect(find.byTooltip('왼쪽 90° 회전'), findsNWidgets(3));

    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await _tapCard(tester, 2);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await tester.pump();
    expect(find.byTooltip('왼쪽 90° 회전'), findsNWidgets(2));

    await tester.tap(find.text('전체 선택'));
    await tester.pump();
    expect(find.byTooltip('왼쪽 90° 회전'), findsNWidgets(3));
    await tester.tap(find.text('선택 해제'));
    await tester.pump();
    expect(find.byTooltip('왼쪽 90° 회전'), findsNothing);
  });

  testWidgets('회전과 삭제 작업이 쌓이고 화면 순서가 바뀌며 되돌리기와 다시하기를 반영한다', (tester) async {
    await _loadPdf(tester, (name, args) async => _pdfResponse(args));
    final undo = find.byTooltip('실행 취소');
    final redo = find.byTooltip('다시 실행');
    expect(_iconButton(tester, undo).onPressed, isNull);
    expect(_iconButton(tester, redo).onPressed, isNull);

    await _tapCard(tester, 2);
    await tester.pump();
    await tester.tap(find.byTooltip('오른쪽 90° 회전'));
    await tester.pump();
    expect(find.textContaining('회전 90° · 2'), findsOneWidget);
    expect(_iconButton(tester, undo).onPressed, isNotNull);

    await _tapCard(tester, 3);
    await tester.pump();
    await tester.tap(find.byTooltip('쪽 삭제'));
    await tester.pump();
    expect(find.byKey(const ValueKey(3)), findsNothing);
    expect(find.text('쪽 1'), findsOneWidget);

    await tester.tap(undo);
    await tester.pump();
    expect(find.byKey(const ValueKey(3)), findsOneWidget);
    expect(_iconButton(tester, redo).onPressed, isNotNull);
    await tester.tap(redo);
    await tester.pump();
    expect(find.byKey(const ValueKey(3)), findsNothing);
  });

  testWidgets('방향키와 드래그로 쪽 순서를 바꾸고 저장 시 정수 쪽 목록과 결과 경로를 보낸다', (tester) async {
    final calls = <Map>[];
    await _loadPdf(tester, (name, args) async {
      final payload = args.single as Map;
      calls.add(payload);
      return _pdfResponse(args);
    });
    await _tapCard(tester, 1);
    await tester.pump();
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowRight);
    await tester.pump();
    expect(_cardOrder(tester), [2, 1, 3]);

    await tester.drag(find.byKey(const ValueKey(3)), const Offset(-350, 0));
    await tester.pumpAndSettle();
    expect(_cardOrder(tester), isNot([2, 1, 3]));
    await tester.tap(find.byKey(const Key('pdfEditRun')));
    await tester.pumpAndSettle();
    final edit = calls.lastWhere((call) => call['action'] == 'reorder');
    expect(edit['pages'], [2, 3, 1]);
    expect(edit['pages'], everyElement(isA<int>()));
  });

  testWidgets('회전 후 삭제한 누적 변경을 순서대로 정수 인자로 저장한다', (tester) async {
    final calls = <Map>[];
    await _loadPdf(tester, (name, args) async {
      final payload = args.single as Map;
      calls.add(payload);
      return _pdfResponse(args);
    });
    await _tapCard(tester, 1);
    await tester.pump();
    await tester.tap(find.byTooltip('오른쪽 90° 회전'));
    await tester.pump();
    await _tapCard(tester, 2);
    await tester.pump();
    await tester.tap(find.byTooltip('쪽 삭제'));
    await tester.pump();
    await tester.tap(find.byKey(const Key('pdfEditRun')));
    await tester.pumpAndSettle();
    final edits = calls.where(
      (call) => !['info', 'thumbnail', 'cleanup'].contains(call['action']),
    );
    expect(edits.map((call) => call['action']).toList(), ['rotate', 'delete']);
    expect(edits.map((call) => call['pages']).toList(), [
      [1],
      [2],
    ]);
    expect(
      edits.every(
        (call) => (call['pages'] as List).every((page) => page is int),
      ),
      isTrue,
    );
    expect(edits.last['output'], 'C:/docs/source_편집.pdf');
  });

  testWidgets('분할 잘못된 범위를 비활성화하고 입력 오류 응답을 표시한다', (tester) async {
    await _loadPdf(tester, (name, args) async => _pdfResponse(args));
    await tester.tap(find.byKey(const Key('pdfEditTool_분할')));
    await tester.pump();
    await tester.enterText(find.byKey(const Key('pdfEditPages')), '2-1');
    await tester.pump();
    expect(
      tester
          .widget<FilledButton>(find.byKey(const Key('pdfEditRun')))
          .onPressed,
      isNull,
    );
    expect(find.text('문서 쪽 수 안에서 올바른 범위를 입력해 주세요.'), findsOneWidget);
    await tester.enterText(find.byKey(const Key('pdfEditPages')), '1-4');
    await tester.pump();
    expect(
      tester
          .widget<FilledButton>(find.byKey(const Key('pdfEditRun')))
          .onPressed,
      isNull,
    );
    expect(find.text('문서 쪽 수 안에서 올바른 범위를 입력해 주세요.'), findsOneWidget);
  });

  testWidgets('합치기 입력 순서를 저장 인자로 보낸다', (tester) async {
    final calls = <Map>[];
    const selector = MethodChannel('plugins.flutter.io/file_selector');
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      selector,
      (call) async => call.method == 'openFile'
          ? ['C:/docs/first.pdf', 'C:/docs/second.pdf']
          : null,
    );
    addTearDown(
      () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        selector,
        null,
      ),
    );
    await _loadPdf(tester, (name, args) async {
      final payload = args.single as Map;
      calls.add(payload);
      return _pdfResponse(args);
    });
    await tester.tap(find.byKey(const Key('pdfEditTool_합치기')));
    await tester.pump();
    await tester.tap(find.byKey(const Key('pdfEditPickMergeSources')));
    await tester.pumpAndSettle();
    final list = tester.widget<ReorderableListView>(
      find.byType(ReorderableListView),
    );
    list.onReorder!(0, 2);
    await tester.pump();
    await tester.tap(find.byKey(const Key('pdfEditRun')));
    await tester.pumpAndSettle();
    final merge = calls.singleWhere((call) => call['action'] == 'merge');
    expect(merge['sources'], ['C:/docs/second.pdf', 'C:/docs/first.pdf']);
    expect(merge['output'], 'C:/docs/first_합본.pdf');
  });

  testWidgets('결과 경로가 원본과 같으면 서비스 호출 없이 거부한다', (tester) async {
    const selector = MethodChannel('plugins.flutter.io/file_selector');
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      selector,
      (call) async =>
          call.method == 'getSavePath' ? 'C:/docs/source.pdf' : null,
    );
    addTearDown(
      () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        selector,
        null,
      ),
    );
    final calls = <Map>[];
    await _loadPdf(tester, (name, args) async {
      final payload = args.single as Map;
      calls.add(payload);
      return _pdfResponse(args);
    });
    await tester.tap(find.byKey(const Key('pdfEditPickOutput')));
    await tester.pumpAndSettle();
    await _tapCard(tester, 1);
    await tester.pump();
    await tester.tap(find.byTooltip('쪽 삭제'));
    await tester.pump();
    await tester.tap(find.byKey(const Key('pdfEditRun')));
    await tester.pumpAndSettle();
    expect(calls.where((call) => call['action'] == 'delete'), isEmpty);
    expect(find.textContaining('원본 파일과 다른 경로'), findsOneWidget);
  });

  testWidgets('실행 중 진행 표시와 서비스 오류를 표시한다', (tester) async {
    final response = Completer<Object?>();
    var editing = false;
    await _loadPdf(tester, (name, args) {
      final payload = args.single as Map;
      if (payload['action'] == 'delete') {
        editing = true;
        return response.future;
      }
      return Future.value(_pdfResponse(args));
    });
    await _tapCard(tester, 1);
    await tester.pump();
    await tester.tap(find.byTooltip('쪽 삭제'));
    await tester.pump();
    await tester.tap(find.byKey(const Key('pdfEditRun')));
    await tester.pump();
    expect(editing, isTrue);
    expect(find.byType(LinearProgressIndicator), findsOneWidget);
    response.complete({
      'ok': false,
      'error': {'message': '저장 거부'},
    });
    await tester.pumpAndSettle();
    expect(find.textContaining('저장 거부'), findsOneWidget);
    expect(find.byType(LinearProgressIndicator), findsNothing);
  });
}


typedef _Api = Future<Object?> Function(String, List<Object?>);

Future<void> _loadPdf(WidgetTester tester, _Api api) async {
  tester.binding.window.physicalSizeTestValue = const Size(1024, 768);
  tester.binding.window.devicePixelRatioTestValue = 1;
  addTearDown(tester.binding.window.clearPhysicalSizeTestValue);
  addTearDown(tester.binding.window.clearDevicePixelRatioTestValue);
  await tester.pumpWidget(
    MaterialApp(
      home: Scaffold(body: PdfEditWorkspace(api: api)),
    ),
  );
  final target = tester.widget<DragTarget<String>>(
    find.byType(DragTarget<String>),
  );
  target.onAcceptWithDetails!(
    DragTargetDetails(data: 'C:/docs/source.pdf', offset: Offset.zero),
  );
  await tester.pump();
  await tester.pumpAndSettle();
}

Object? _pdfResponse(List<Object?> args) {
  final payload = args.single as Map;
  if (payload['action'] == 'info') {
    return {
      'ok': true,
      'data': {
        'page_count': 3,
        'pages': [
          {'number': 1},
          {'number': 2},
          {'number': 3},
        ],
      },
    };
  }
  if (payload['action'] == 'thumbnail') return {'ok': false};
  return {
    'ok': true,
    'data': {'output': payload['output']},
  };
}

List<int> _cardOrder(WidgetTester tester) => tester
    .widgetList<Draggable<int>>(find.byType(Draggable<int>))
    .map((draggable) => draggable.data!)
    .toList();

Future<void> _tapCard(WidgetTester tester, int page) async {
  await tester.tap(
    find
        .descendant(
          of: find.byKey(ValueKey(page)),
          matching: find.byType(InkWell),
        )
        .first,
  );
}

IconButton _iconButton(WidgetTester tester, Finder tooltip) => tester.widget(
  find.ancestor(of: tooltip, matching: find.byType(IconButton)).first,
);
