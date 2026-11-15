import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:docfit_ui/features/table_sketch/table_sketch_models.dart';
import 'package:docfit_ui/features/table_sketch/table_sketch_screen.dart';

void main() {
  group('TableSketch Infinite Canvas Tests (SKETCH-INF)', () {
    test('음수 좌표 표를 toJson(forTable: true)로 내보낼 때 그린 범위를 원점(0,0) 기준으로 옮긴다', () {
      final doc = TableSketchDoc(
        grid: const GridConfig(cols: 36, rows: 50),
        lines: [
          const SketchLine(x1: -6, y1: -4, x2: 4, y2: -4),
          const SketchLine(x1: -6, y1: -4, x2: -6, y2: 6),
          const SketchLine(x1: 4, y1: -4, x2: 4, y2: 6),
          const SketchLine(x1: -6, y1: 6, x2: 4, y2: 6),
        ],
        texts: [
          const SketchText(x: -4, y: -2, text: '음수 칸'),
        ],
        fills: [
          const SketchFill(x1: -6, y1: -4, x2: 4, y2: 6, color: '#FEF9C3'),
        ],
        arrows: [
          const SketchArrow(x1: -6, y1: -4, x2: -2, y2: -4),
        ],
      );

      final exported = doc.toJson(forTable: true);
      final lines = (exported['lines'] as List).cast<Map<String, dynamic>>();
      final texts = (exported['texts'] as List).cast<Map<String, dynamic>>();
      final fills = (exported['fills'] as List).cast<Map<String, dynamic>>();
      final arrows = (exported['arrows'] as List).cast<Map<String, dynamic>>();
      final grid = exported['grid'] as Map<String, dynamic>;

      // minX는 -6, minY는 -4이므로 dx=+6, dy=+4 만큼 이동하여 원점 기준(0,0)으로 이동
      expect(lines.first['x1'], 0);
      expect(lines.first['y1'], 0);
      expect(lines.first['x2'], 10);
      expect(lines.first['y2'], 0);

      expect(texts.single['x'], 2);
      expect(texts.single['y'], 2);
      expect(texts.single['text'], '음수 칸');

      expect(fills.single['x1'], 0);
      expect(fills.single['y1'], 0);
      expect(fills.single['x2'], 10);
      expect(fills.single['y2'], 10);

      expect(arrows.single['x1'], 0);
      expect(arrows.single['y1'], 0);
      expect(arrows.single['x2'], 4);
      expect(arrows.single['y2'], 0);

      // 격자 크기도 원점 이동 후 범위를 수용하도록 보장
      expect(grid['cols'], greaterThanOrEqualTo(10));
      expect(grid['rows'], greaterThanOrEqualTo(10));
    });

    test('기존 파일 열기(고정 격자 JSON 하위 호환성 유지)', () {
      final legacyJson = {
        'orientation': 'portrait',
        'grid': {'cols': 36, 'rows': 50, 'cell_mm': 5.0},
        'lines': [
          {'x1': 0, 'y1': 0, 'x2': 10, 'y2': 0, 'width': 'thin', 'style': 'solid'},
        ],
        'texts': [
          {'x': 1, 'y': 1, 'text': '기존 데이터', 'bold': false, 'align': 'center'},
        ],
      };

      final doc = TableSketchDoc.fromJson(legacyJson);
      expect(doc.grid.cols, 36);
      expect(doc.grid.rows, 50);
      expect(doc.lines.length, 1);
      expect(doc.texts.single.text, '기존 데이터');

      final saved = doc.toJson();
      expect((saved['lines'] as List).length, 1);
      expect((saved['texts'] as List).length, 1);
    });

    testWidgets('가운데 단추(kMiddleMouseButton) 끌기 및 스페이스+끌기로 캔버스를 이동한다', (tester) async {
      tester.view.physicalSize = const Size(1280, 900);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);

      Future<Object?> api(String name, List<Object?> args) async {
        return {'ok': true};
      }

      await tester.pumpWidget(MaterialApp(home: TableSketchScreen(api: api)));
      await tester.pumpAndSettle();

      final canvasFinder = find.byKey(const Key('sketchCanvas'));
      expect(canvasFinder, findsOneWidget);
      final initialTopLeft = tester.getTopLeft(canvasFinder);

      // 1. 가운데 단추 끌기로 이동 (kMiddleMouseButton)
      final center = tester.getCenter(canvasFinder);
      final pointer = TestPointer(1, PointerDeviceKind.mouse, 0, kMiddleMouseButton);
      await tester.sendEventToBinding(pointer.down(center));
      await tester.pump();
      await tester.sendEventToBinding(pointer.move(center + const Offset(50, 40)));
      await tester.pump();
      await tester.sendEventToBinding(pointer.up());
      await tester.pumpAndSettle();

      final afterMiddlePan = tester.getTopLeft(canvasFinder);
      expect(afterMiddlePan.dx, closeTo(initialTopLeft.dx + 50, 0.5));
      expect(afterMiddlePan.dy, closeTo(initialTopLeft.dy + 40, 0.5));

      // 2. 100% 맞춤 단추로 리셋 시 원위치 복귀
      await tester.tap(find.byTooltip('100% 맞춤'));
      await tester.pumpAndSettle();
      final afterReset = tester.getTopLeft(canvasFinder);
      expect(afterReset.dx, closeTo(initialTopLeft.dx, 0.5));
      expect(afterReset.dy, closeTo(initialTopLeft.dy, 0.5));

      // 3. 스페이스+끌기로 이동
      await tester.sendKeyDownEvent(LogicalKeyboardKey.space);
      await tester.pump();
      final spacePointer = TestPointer(2, PointerDeviceKind.mouse);
      await tester.sendEventToBinding(spacePointer.down(center));
      await tester.pump();
      await tester.sendEventToBinding(spacePointer.move(center + const Offset(-30, -25)));
      await tester.pump();
      await tester.sendEventToBinding(spacePointer.up());
      await tester.pump();
      await tester.sendKeyUpEvent(LogicalKeyboardKey.space);
      await tester.pumpAndSettle();

      final afterSpacePan = tester.getTopLeft(canvasFinder);
      expect(afterSpacePan.dx, closeTo(initialTopLeft.dx - 30, 0.5));
      expect(afterSpacePan.dy, closeTo(initialTopLeft.dy - 25, 0.5));
    });

    testWidgets('격자 밖 좌표(기본 cols=36 초과)로 선을 그릴 때 clamp되지 않고 선이 생성된다', (tester) async {
      tester.view.physicalSize = const Size(1400, 900);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);

      final previews = <Map>[];
      Future<Object?> api(String name, List<Object?> args) async {
        if (name == 'table_sketch' && args[0] == 'preview') {
          previews.add(args[1] as Map);
        }
        return {'ok': true};
      }

      await tester.pumpWidget(MaterialApp(home: TableSketchScreen(api: api)));
      await tester.pumpAndSettle();

      final origin = tester.getTopLeft(find.byKey(const Key('sketchCanvas')));
      const cellSize = 16.0;
      Offset at(double gx, double gy) => origin + Offset(gx * cellSize, gy * cellSize);

      // (2, 5)에서 시작해서 cols(36)를 넘는 (42, 5)까지 가로선 그리기
      final draw = await tester.startGesture(at(2, 5));
      await draw.moveTo(at(42, 5));
      await draw.up();
      await tester.pump(const Duration(milliseconds: 600));
      await tester.pumpAndSettle();

      expect(previews.isNotEmpty, isTrue);
      final lastLines = (previews.last['lines'] as List).cast<Map>();
      expect(lastLines.isNotEmpty, isTrue);
      // 36으로 잘리지 않고 42까지 선이 이어짐
      expect(lastLines.any((l) => l['x2'] == 42 && l['y2'] == 5), isTrue);
    });
  });
}
