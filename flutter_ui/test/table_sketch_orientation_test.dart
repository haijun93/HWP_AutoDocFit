import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:docfit_ui/features/table_sketch/table_sketch_models.dart';
import 'package:docfit_ui/features/table_sketch/table_sketch_screen.dart';

void main() {
  setUp(() {
    TableSketchScreen.resetLastOrientation();
  });

  tearDown(() {
    TableSketchScreen.resetLastOrientation();
  });

  group('TableSketch A4 Orientation Tests', () {
    testWidgets('방향 전환 시 용지 차지 표시 갱신 및 가이드 격자 크기 변경', (tester) async {
      tester.view.physicalSize = const Size(1280, 900);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);

      Future<Object?> api(String name, List<Object?> args) async {
        if (name == 'table_sketch' && args[0] == 'preview') {
          return {
            'ok': true,
            'rows': 2,
            'cols': 2,
            'cells': [],
            'width_mm': 50.0,
            'height_mm': 40.0,
            'warnings': [],
          };
        }
        return {'ok': true};
      }

      await tester.pumpWidget(MaterialApp(home: TableSketchScreen(api: api)));
      await tester.pumpAndSettle();

      // 기본 세로 방향 및 용지 차지 표시 확인
      expect(find.byKey(const Key('paperOrientationPortrait')), findsOneWidget);
      expect(find.byKey(const Key('paperOrientationLandscape')), findsOneWidget);
      expect(find.textContaining('용지 차지: 50.0mm × 40.0mm (A4 세로)'), findsOneWidget);

      // 가로 방향으로 전환
      await tester.tap(find.byKey(const Key('paperOrientationLandscape')));
      await tester.pumpAndSettle();

      // 용지 차지 표시에 A4 가로 반영 확인
      expect(find.textContaining('용지 차지: 50.0mm × 40.0mm (A4 가로)'), findsOneWidget);

      // 다시 세로 방향으로 복귀
      await tester.tap(find.byKey(const Key('paperOrientationPortrait')));
      await tester.pumpAndSettle();
      expect(find.textContaining('용지 차지: 50.0mm × 40.0mm (A4 세로)'), findsOneWidget);
    });

    testWidgets('표 폭이 본문 폭을 넘을 때 경고 발생 및 가로 전환 시 경고 해제', (tester) async {
      tester.view.physicalSize = const Size(1280, 900);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);

      // 표 폭이 180mm인 경우: 세로 본문 폭(150mm) 초과, 가로 본문 폭(237mm) 이내
      Future<Object?> api(String name, List<Object?> args) async {
        if (name == 'table_sketch' && args[0] == 'preview') {
          return {
            'ok': true,
            'rows': 4,
            'cols': 4,
            'cells': [],
            'width_mm': 180.0,
            'height_mm': 60.0,
            'warnings': [],
          };
        }
        return {'ok': true};
      }

      await tester.pumpWidget(MaterialApp(home: TableSketchScreen(api: api)));
      await tester.pumpAndSettle();

      // 세로 방향일 때 본문 폭(150mm) 초과 경고 표시 확인
      expect(find.textContaining('A4 세로 본문 폭(150mm)을 넘습니다'), findsOneWidget);

      // 가로 방향으로 전환
      await tester.tap(find.byKey(const Key('paperOrientationLandscape')));
      await tester.pumpAndSettle();

      // 가로 본문 폭(237mm) 이내이므로 경고 해제 확인
      expect(find.textContaining('본문 폭'), findsNothing);

      // 다시 세로 방향으로 전환 시 경고 재발생 확인
      await tester.tap(find.byKey(const Key('paperOrientationPortrait')));
      await tester.pumpAndSettle();
      expect(find.textContaining('A4 세로 본문 폭(150mm)을 넘습니다'), findsOneWidget);
    });

    testWidgets('저장 및 불러오기 시 orientation 보존 및 기존 파일 호환성', (tester) async {
      tester.view.physicalSize = const Size(1280, 900);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);

      // 1. 모델 직렬화/역직렬화 검증
      final landscapeDoc = TableSketchDoc(orientation: 'landscape');
      final json = landscapeDoc.toJson();
      expect(json['orientation'], 'landscape');

      final restored = TableSketchDoc.fromJson(json);
      expect(restored.orientation, 'landscape');

      // 기존 파일(orientation 필드 없음)은 세로로 역직렬화
      final legacyDoc = TableSketchDoc.fromJson({'grid': {'cols': 36, 'rows': 50}});
      expect(legacyDoc.orientation, 'portrait');

      // 2. 화면에서 가로 방향 선택 후 서식 저장 시 orientation 전달 확인
      final savedCalls = <Map<String, dynamic>>[];
      Future<Object?> api(String name, List<Object?> args) async {
        if (name == 'table_sketch') {
          if (args[0] == 'preview') {
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
          if (args[0] == 'save') {
            final payload = args[1] as Map;
            savedCalls.add(Map<String, dynamic>.from(payload));
            return {'ok': true};
          }
        }
        return {'ok': true};
      }

      await tester.pumpWidget(MaterialApp(home: TableSketchScreen(api: api)));
      await tester.pumpAndSettle();

      await tester.tap(find.byKey(const Key('paperOrientationLandscape')));
      await tester.pumpAndSettle();

      await tester.enterText(find.byKey(const Key('sketchNameField')), '가로표');
      await tester.tap(find.byKey(const Key('sketchSaveButton')));
      await tester.pumpAndSettle();

      expect(savedCalls.isNotEmpty, isTrue);
      final savedSketch = savedCalls.last['sketch'] as Map;
      expect(savedSketch['orientation'], 'landscape');
    });
  });
}
