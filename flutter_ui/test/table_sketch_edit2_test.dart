import 'package:docfit_ui/features/table_sketch/table_sketch_models.dart';
import 'package:docfit_ui/features/table_sketch/table_sketch_painter.dart';
import 'package:docfit_ui/features/table_sketch/table_sketch_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('EDIT2 - Infinite Canvas & Reorigin', () {
    test('toJson(forTable: true) shifts negative and offset coordinates to origin', () {
      final doc = TableSketchDoc(
        grid: const GridConfig(cols: 36, rows: 50, cellMm: 5.0),
        orientation: 'landscape',
        lines: [
          const SketchLine(x1: -5, y1: -2, x2: 5, y2: -2),
          const SketchLine(x1: -5, y1: -2, x2: -5, y2: 8),
        ],
        texts: [
          const SketchText(x: -4, y: 0, text: '제목'),
        ],
        arrows: [
          const SketchArrow(x1: -3, y1: 1, x2: 2, y2: 1),
        ],
      );
      doc.paintCells({(-5, -2), (0, 0)}, '#FFEB3B');

      final tableJson = doc.toJson(forTable: true);
      expect(tableJson['orientation'], 'landscape');

      final lines = (tableJson['lines'] as List).cast<Map>();
      // minX는 -5이므로 shift +5 -> x1: 0, x2: 10
      // minY는 -2이므로 shift +2 -> y1: 0, y2: 0
      expect(lines[0]['x1'], 0);
      expect(lines[0]['y1'], 0);
      expect(lines[0]['x2'], 10);
      expect(lines[0]['y2'], 0);

      // 세로선: (-5, -2) -> (-5, 8) => (0, 0) -> (0, 10)
      expect(lines[1]['x1'], 0);
      expect(lines[1]['y1'], 0);
      expect(lines[1]['x2'], 0);
      expect(lines[1]['y2'], 10);

      // 글자: (-4, 0) => (1, 2)
      final texts = (tableJson['texts'] as List).cast<Map>();
      expect(texts[0]['x'], 1);
      expect(texts[0]['y'], 2);

      // 화살표: (-3, 1) -> (2, 1) => (2, 3) -> (7, 3)
      final arrows = (tableJson['arrows'] as List).cast<Map>();
      expect(arrows[0]['x1'], 2);
      expect(arrows[0]['y1'], 3);
      expect(arrows[0]['x2'], 7);
      expect(arrows[0]['y2'], 3);

      // 채우기: (-5, -2) => (0, 0), (0, 0) => (5, 2)
      final fills = (tableJson['fills'] as List).cast<Map>();
      expect(fills.any((f) => f['x'] == 0 && f['y'] == 0), isTrue);
      expect(fills.any((f) => f['x'] == 5 && f['y'] == 2), isTrue);

      // 그리드 크기 자동 확장 검증 (10+2 = 12 이상)
      final grid = tableJson['grid'] as Map;
      expect((grid['cols'] as int) >= 12, isTrue);
      expect((grid['rows'] as int) >= 12, isTrue);
    });

    test('TableSketchDoc backwards compatibility for legacy JSON', () {
      final legacyJson = {
        'grid': {'cols': 36, 'rows': 50, 'cell_mm': 5.0},
        'lines': [
          {'x1': 0, 'y1': 0, 'x2': 10, 'y2': 0, 'width': 'thin', 'style': 'solid'},
        ],
        'texts': [],
        // orientation, view_offset_x, view_offset_y, zoom 누락
      };

      final doc = TableSketchDoc.fromJson(legacyJson);
      expect(doc.orientation, 'portrait');
      expect(doc.viewOffsetX, 0.0);
      expect(doc.viewOffsetY, 0.0);
      expect(doc.zoom, 1.0);

      final cloned = doc.clone();
      expect(cloned.orientation, 'portrait');
      expect(cloned.viewOffsetX, 0.0);
      expect(cloned.viewOffsetY, 0.0);
      expect(cloned.zoom, 1.0);
    });

    test('snapLineCoords works in negative space beyond paper', () {
      // 음수 좌표에서의 가로선 스냅
      final snap = snapLineCoords(-10, -5, 5, -4, 36, 50);
      expect(snap, isNotNull);
      expect(snap!.x, 5);
      expect(snap.y, -5); // y는 -5로 스냅
    });

    test('fitRecognizedSketch respects orientation', () {
      final doc10 = TableSketchDoc(
        grid: const GridConfig(cols: 10, rows: 5),
        lines: [
          const SketchLine(x1: 0, y1: 0, x2: 10, y2: 0),
        ],
      );
      // 세로 기준 (targetWidthUnits 32): 32 ~/ 10 = scale 3
      final fittedPortrait = fitRecognizedSketch(doc10, orientation: 'portrait');
      expect(fittedPortrait.lines.single.x2, 30);
      expect(fittedPortrait.orientation, 'portrait');

      // 가로 기준 (targetWidthUnits 48): 48 ~/ 10 = scale 4 (최대 4)
      final fittedLandscape = fitRecognizedSketch(doc10, orientation: 'landscape');
      expect(fittedLandscape.lines.single.x2, 40);
      expect(fittedLandscape.orientation, 'landscape');
    });
  });

  group('EDIT2 - TableSketchScreen Widget Tests', () {
    late List<List<Object?>> calls;

    Future<Object?> api(String name, List<Object?> args) async {
      calls.add([name, ...args]);
      if (name == 'table_sketch' && args.isNotEmpty && args[0] == 'preview') {
        final sketch = args[1] as Map;
        final orientation = sketch['orientation'] as String? ?? 'portrait';
        final maxMm = orientation == 'landscape' ? 237.0 : 150.0;
        final lines = (sketch['lines'] as List? ?? []).cast<Map>();
        double widthMm = 50.0;
        final warnings = <String>[];
        if (lines.any((l) => (l['x2'] as int) > 50)) {
          widthMm = 300.0;
        }
        if (widthMm > maxMm) {
          warnings.add('표 폭(${widthMm.toInt()}mm)이 A4 본문 폭(${maxMm.toInt()}mm)을 초과합니다.');
        }
        return {
          'ok': true,
          'rows': 4,
          'cols': 4,
          'cells': [],
          'width_mm': widthMm,
          'height_mm': 20.0,
          'warnings': warnings,
        };
      }
      if (name == 'table_sketch' && args.isNotEmpty && args[0] == 'save') {
        return {'ok': true};
      }
      return {
        'ok': true,
        'rows': 1,
        'cols': 1,
        'cells': [],
        'width_mm': 10.0,
        'height_mm': 10.0,
        'warnings': [],
      };
    }

    setUp(() {
      calls = [];
    });

    testWidgets('New sketch button on clean canvas resets directly without dialog', (tester) async {
      tester.view.physicalSize = const Size(1280, 800);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);

      await tester.pumpWidget(MaterialApp(home: TableSketchScreen(api: api)));
      await tester.pumpAndSettle();

      final newButton = find.byKey(const Key('sketchNewButton'));
      expect(newButton, findsOneWidget);

      await tester.tap(newButton);
      await tester.pumpAndSettle();

      // 변경사항이 없었으므로 다이얼로그가 뜨지 않아야 함
      expect(find.byKey(const Key('sketchNewDialog')), findsNothing);
    });

    testWidgets('New sketch flow with dirty canvas: Cancel, Discard, and Save', (tester) async {
      tester.view.physicalSize = const Size(1280, 800);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);

      await tester.pumpWidget(MaterialApp(home: TableSketchScreen(api: api)));
      await tester.pumpAndSettle();

      final canvasFinder = find.byKey(const Key('sketchCanvas'));
      final canvasTopLeft = tester.getTopLeft(canvasFinder);

      // 선 하나 그리기 (작업 변경사항 생성)
      final gesture = await tester.startGesture(canvasTopLeft + const Offset(16, 16));
      await gesture.moveTo(canvasTopLeft + const Offset(160, 16));
      await gesture.up();
      await tester.pumpAndSettle();

      // 1. 취소 테스트
      await tester.tap(find.byKey(const Key('sketchNewButton')));
      await tester.pumpAndSettle();

      expect(find.byKey(const Key('sketchNewDialog')), findsOneWidget);
      await tester.tap(find.byKey(const Key('sketchNewCancel')));
      await tester.pumpAndSettle();

      // 다이얼로그 닫힘, 내용 보존
      expect(find.byKey(const Key('sketchNewDialog')), findsNothing);

      // 2. 저장 안 함(Discard) 테스트
      // 먼저 방향 변경 (A4 가로)
      await tester.tap(find.text('A4 가로'));
      await tester.pumpAndSettle();

      await tester.tap(find.byKey(const Key('sketchNewButton')));
      await tester.pumpAndSettle();

      expect(find.byKey(const Key('sketchNewDialog')), findsOneWidget);
      await tester.tap(find.byKey(const Key('sketchNewDiscard')));
      await tester.pumpAndSettle();

      // 전체 초기화되었는지 확인 (다이얼로그 닫힘, 방향이 다시 세로로 복원)
      expect(find.byKey(const Key('sketchNewDialog')), findsNothing);
      expect(find.textContaining('A4 세로'), findsOneWidget);

      // 3. 저장(Save) 테스트
      // 다시 선 하나 그리기
      final gesture2 = await tester.startGesture(canvasTopLeft + const Offset(32, 32));
      await gesture2.moveTo(canvasTopLeft + const Offset(160, 32));
      await gesture2.up();
      await tester.pumpAndSettle();

      await tester.tap(find.byKey(const Key('sketchNewButton')));
      await tester.pumpAndSettle();

      expect(find.byKey(const Key('sketchNewDialog')), findsOneWidget);
      await tester.tap(find.byKey(const Key('sketchNewSave')));
      await tester.pumpAndSettle();

      // 저장 호출 확인
      final saveCalls = calls.where((c) => c[0] == 'table_sketch' && c[1] == 'save').toList();
      expect(saveCalls.isNotEmpty, isTrue);
      expect(find.text('저장되었습니다.'), findsOneWidget);
    });

    testWidgets('Ctrl+N keyboard shortcut triggers new sketch', (tester) async {
      tester.view.physicalSize = const Size(1280, 800);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);

      await tester.pumpWidget(MaterialApp(home: TableSketchScreen(api: api)));
      await tester.pumpAndSettle();

      final canvasFinder = find.byKey(const Key('sketchCanvas'));
      final canvasTopLeft = tester.getTopLeft(canvasFinder);

      // 선 그리기
      final gesture = await tester.startGesture(canvasTopLeft + const Offset(16, 16));
      await gesture.moveTo(canvasTopLeft + const Offset(160, 16));
      await gesture.up();
      await tester.pumpAndSettle();

      // Ctrl+N 키보드 입력
      await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
      await tester.sendKeyEvent(LogicalKeyboardKey.keyN);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
      await tester.pumpAndSettle();

      expect(find.byKey(const Key('sketchNewDialog')), findsOneWidget);
      await tester.tap(find.byKey(const Key('sketchNewCancel')));
      await tester.pumpAndSettle();
    });

    testWidgets('A4 orientation toggle changes paper guide and occupancy ratio', (tester) async {
      tester.view.physicalSize = const Size(1280, 800);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);

      await tester.pumpWidget(MaterialApp(home: TableSketchScreen(api: api)));
      await tester.pumpAndSettle();

      // 초기 세로 상태 확인
      expect(find.byKey(const Key('sketchOrientationToggle')), findsOneWidget);
      final occupancyFinder = find.byKey(const Key('sketchPaperOccupancyText'));
      expect(occupancyFinder, findsOneWidget);
      expect(find.textContaining('A4 세로'), findsOneWidget);

      // 가로 버튼 클릭
      await tester.tap(find.text('A4 가로'));
      await tester.pumpAndSettle();

      // 디바운스 대기
      await tester.pump(const Duration(milliseconds: 600));
      await tester.pumpAndSettle();

      // 가로 상태 반영 확인
      expect(find.textContaining('A4 가로'), findsOneWidget);

      final previewCalls = calls.where((c) => c[0] == 'table_sketch' && c[1] == 'preview').toList();
      expect(previewCalls.isNotEmpty, isTrue);
      final lastSketch = previewCalls.last[2] as Map;
      expect(lastSketch['orientation'], 'landscape');
    });

    testWidgets('Fit to screen and paper guide toggle buttons operate correctly', (tester) async {
      tester.view.physicalSize = const Size(1280, 800);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);

      await tester.pumpWidget(MaterialApp(home: TableSketchScreen(api: api)));
      await tester.pumpAndSettle();

      final fitButton = find.byKey(const Key('fitToScreenButton'));
      expect(fitButton, findsOneWidget);
      await tester.tap(fitButton);
      await tester.pumpAndSettle();

      final guideToggle = find.byKey(const Key('togglePaperGuide'));
      expect(guideToggle, findsOneWidget);
      await tester.tap(guideToggle);
      await tester.pumpAndSettle();

      expect(tester.takeException(), isNull);
    });
  });
}
