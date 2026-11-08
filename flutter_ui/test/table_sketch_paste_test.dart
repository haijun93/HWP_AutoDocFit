import 'package:docfit_ui/features/table_sketch/table_sketch_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

Map<String, dynamic> samplePastedTable({String first = '항목구분'}) => {
  'ok': true,
  'rows': 2,
  'cols': 2,
  'seq': 12,
  'warnings': ['대각선은(는) 표그림판에서 표현하지 못해 제외했습니다.'],
  'sketch': {
    'grid': {'cols': 36, 'rows': 50, 'cell_mm': 5.0},
    'lines': [
      for (final y in [1, 5, 9]) {'x1': 1, 'y1': y, 'x2': 9, 'y2': y},
      for (final x in [1, 5, 9]) {'x1': x, 'y1': 1, 'x2': x, 'y2': 9},
    ],
    'texts': [
      {'x': 1, 'y': 1, 'text': first, 'bold': true, 'align': 'center'},
    ],
    'fills': [
      {'x1': 1, 'y1': 1, 'x2': 5, 'y2': 5, 'color': '#E2EFDA'},
    ],
  },
};

void main() {
  group('표그림판 한/글 표 붙여넣기 (TPASTE)', () {
    late List<Map> previews;
    late List<Map> pasteCalls;
    late Object? Function() pasteResponse;

    Future<Object?> api(String name, List<Object?> args) async {
      if (name == 'table_sketch' && args[0] == 'preview') {
        previews.add(args[1] as Map);
        return {'ok': true, 'rows': 0, 'cols': 0, 'cells': [], 'warnings': []};
      }
      if (name == 'table_sketch' && args[0] == 'paste_hwp') {
        pasteCalls.add(args[1] as Map);
        return pasteResponse();
      }
      if (name == 'table_sketch' && args[0] == 'load_draft') {
        return {'ok': true, 'exists': false};
      }
      return {'ok': true};
    }

    Future<void> open(
      WidgetTester tester, {
      Size size = const Size(1400, 900),
    }) async {
      tester.view.physicalSize = size;
      tester.view.devicePixelRatio = 1.0;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      previews = [];
      pasteCalls = [];
      await tester.pumpWidget(
        MaterialApp(
          home: TableSketchScreen(api: api),
        ),
      );
      await tester.pump(const Duration(seconds: 1));
    }

    testWidgets('붙여넣기 단추 클릭 시 겹쳐 보이고 적용 및 되돌리기(Ctrl+Z)', (tester) async {
      pasteResponse = () => samplePastedTable(first: '결산표');
      await open(tester);

      final pasteButton = find.byKey(const Key('sketchPasteHwp'));
      expect(pasteButton, findsOneWidget);
      expect(find.text('붙여넣기'), findsOneWidget);

      // 1. 단추 클릭
      await tester.tap(pasteButton);
      await tester.pump();
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 300));

      expect(pasteCalls, hasLength(1));
      expect(pasteCalls.first['cell_mm'], 5.0);

      // 겹쳐 보기 오버레이 및 배너 표시 확인
      expect(find.byKey(const Key('recognizeOverlay')), findsOneWidget);
      expect(find.byKey(const Key('recognizeCanvasOverlay')), findsOneWidget);
      expect(find.textContaining('붙여넣은 표: 2행 × 2열'), findsOneWidget);
      expect(find.textContaining('대각선은(는) 표그림판에서 표현하지 못해 제외했습니다.'), findsOneWidget);

      final undo = find.byKey(const Key('undoButton'));
      expect(tester.widget<IconButton>(undo).onPressed, isNull);

      // 2. [취소] 클릭 시 원래대로 유지
      await tester.tap(find.byKey(const Key('recognizeCancel')));
      await tester.pump();
      expect(find.byKey(const Key('recognizeOverlay')), findsNothing);
      expect(find.byKey(const Key('recognizeCanvasOverlay')), findsNothing);

      // 3. 다시 단추 클릭 후 [적용]
      await tester.tap(pasteButton);
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 300));

      expect(find.byKey(const Key('recognizeOverlay')), findsOneWidget);
      await tester.tap(find.byKey(const Key('recognizeApply')));
      await tester.pump(const Duration(seconds: 1));

      expect(find.byKey(const Key('recognizeOverlay')), findsNothing);
      expect(find.textContaining('붙여넣기 전으로 돌아갑니다'), findsOneWidget);
      expect(previews.last['texts'], hasLength(1));
      expect((previews.last['texts'] as List).first['text'], '결산표');
      expect(tester.widget<IconButton>(undo).onPressed, isNotNull);

      // 4. 되돌리기 (Ctrl+Z / undoButton)
      await tester.tap(undo);
      await tester.pump(const Duration(seconds: 1));
      expect(previews.last['texts'], isEmpty);
      expect(tester.widget<IconButton>(undo).onPressed, isNull);
    });

    testWidgets('Ctrl+V 단축키로 한/글 표 붙여넣기 겹쳐 보기', (tester) async {
      pasteResponse = () => samplePastedTable(first: '단축키표');
      await open(tester);

      // Ctrl+V 키 입력
      await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
      await tester.sendKeyEvent(LogicalKeyboardKey.keyV);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 300));

      expect(pasteCalls, hasLength(1));
      expect(find.byKey(const Key('recognizeOverlay')), findsOneWidget);
      expect(find.textContaining('붙여넣은 표: 2행 × 2열'), findsOneWidget);

      await tester.tap(find.byKey(const Key('recognizeApply')));
      await tester.pump(const Duration(seconds: 1));
      expect(find.textContaining('붙여넣기 전으로 돌아갑니다'), findsOneWidget);
    });

    testWidgets('클립보드에 표가 없으면 안내 스낵바', (tester) async {
      pasteResponse = () => {
        'ok': false,
        'message': '클립보드에 한/글 표가 없습니다. 한/글에서 표를 선택해 복사(Ctrl+C)한 뒤 다시 눌러 주세요.',
      };
      await open(tester);

      await tester.tap(find.byKey(const Key('sketchPasteHwp')));
      await tester.pump();
      await tester.pump();

      expect(find.byKey(const Key('recognizeOverlay')), findsNothing);
      expect(find.textContaining('클립보드에 한/글 표가 없습니다'), findsOneWidget);
    });
  });
}
