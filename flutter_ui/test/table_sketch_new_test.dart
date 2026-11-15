import 'package:docfit_ui/features/table_sketch/table_sketch_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('TableSketch 새로 그리기 Tests', () {
    late List<Map> previews;
    late List<List<Object?>> calls;
    bool saveOk = true;

    Future<Object?> api(String name, List<Object?> args) async {
      calls.add([name, ...args]);
      if (name == 'table_sketch') {
        final action = args[0] as String;
        if (action == 'preview') {
          previews.add(args[1] as Map);
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
        if (action == 'save') {
          final payload = args[1] as Map;
          return {
            'ok': saveOk,
            'name': payload['name'],
            if (!saveOk) 'message': '저장 권한이 없습니다.',
          };
        }
        if (action == 'clear_draft') {
          return {'ok': true};
        }
        if (action == 'save_draft' || action == 'load_draft') {
          return {'ok': true, 'exists': false};
        }
      }
      return {'ok': true};
    }

    Future<Offset Function(double, double)> open(WidgetTester tester) async {
      previews = [];
      calls = [];
      saveOk = true;
      tester.view.physicalSize = const Size(1280, 900);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      await tester.pumpWidget(MaterialApp(home: TableSketchScreen(api: api)));
      await tester.pumpAndSettle();
      final origin = tester.getTopLeft(find.byKey(const Key('sketchCanvas')));
      return (gx, gy) => origin + Offset(gx * 16, gy * 16);
    }

    testWidgets(
      '빈 작업 상태에서는 저장 안내창 없이 바로 초기화되고 Ctrl+N 단축키 및 툴팁 위젯이 정상 동작한다',
      (tester) async {
        await open(tester);

        // 1. Tooltip 위젯이 newSketchButton을 감싸고 있는지 확인
        final tooltipFinder = find.byWidgetPredicate(
          (w) =>
              w is Tooltip &&
              w.message == '새로 그리기 (Ctrl+N)' &&
              w.child is IconButton &&
              (w.child as IconButton).key == const Key('newSketchButton'),
        );
        expect(tooltipFinder, findsOneWidget);

        // 2. 빈 상태에서 새로 그리기 버튼 클릭 시 대화상자 없이 바로 초기화
        await tester.tap(find.byKey(const Key('newSketchButton')));
        await tester.pumpAndSettle();
        expect(find.text('작업 내용을 저장하시겠습니까?'), findsNothing);

        // 3. 단축키 Ctrl+N 입력 시에도 예외 없이 초기화 동작
        await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
        await tester.sendKeyEvent(LogicalKeyboardKey.keyN);
        await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
        await tester.pumpAndSettle();
        expect(find.text('작업 내용을 저장하시겠습니까?'), findsNothing);
        expect(tester.takeException(), isNull);
      },
    );

    testWidgets(
      '작업 내용 또는 설정 변경 시 안내창이 뜨며, 취소 시 유지되고 저장 안 함 시 모든 설정이 초기화된다',
      (tester) async {
        final at = await open(tester);

        // 사각형 도구 선택 및 그리기
        await tester.tap(find.byKey(const Key('toolRect')));
        await tester.pumpAndSettle();
        await tester.tap(find.byKey(const Key('lineWidthThick')));
        await tester.pumpAndSettle();

        // 캔버스에 사각형 배치
        await tester.tapAt(at(3, 3));
        await tester.pump(const Duration(milliseconds: 600));
        await tester.pumpAndSettle();
        expect(previews.isNotEmpty, isTrue);
        expect((previews.last['lines'] as List).isNotEmpty, isTrue);

        // 서식 이름 변경
        await tester.enterText(find.byKey(const Key('sketchNameField')), '수정된 서식');
        await tester.pumpAndSettle();

        // 새로 그리기 버튼 클릭 -> 안내창 확인
        await tester.tap(find.byKey(const Key('newSketchButton')));
        await tester.pumpAndSettle();
        expect(find.text('새로 그리기'), findsOneWidget);
        expect(find.text('작업 내용을 저장하시겠습니까?'), findsOneWidget);

        // 1. 취소 클릭 시 다이얼로그가 닫히고 기존 내용 유지
        await tester.tap(find.byKey(const Key('sketchNewCancelButton')));
        await tester.pumpAndSettle();
        expect(find.text('작업 내용을 저장하시겠습니까?'), findsNothing);
        expect(
          tester.widget<TextField>(find.byKey(const Key('sketchNameField'))).controller?.text,
          '수정된 서식',
        );

        // 2. 다시 새로 그리기 클릭 후 '저장 안 함' 선택 시 모든 작업 및 설정 초기화
        await tester.tap(find.byKey(const Key('newSketchButton')));
        await tester.pumpAndSettle();
        await tester.tap(find.byKey(const Key('sketchNewDiscardButton')));
        await tester.pump(const Duration(milliseconds: 600));
        await tester.pumpAndSettle();

        // 초기화 검증: 이름 칸, 선 목록, 기본 도구(직선) 및 굵기(thin) 복귀
        expect(
          tester.widget<TextField>(find.byKey(const Key('sketchNameField'))).controller?.text,
          '새 표 서식',
        );
        expect(previews.last['lines'] as List, isEmpty);
        expect(tester.takeException(), isNull);
      },
    );

    testWidgets(
      '저장 선택 시 저장을 마치고 초기화되며, 저장 실패 시에는 초기화하지 않는다',
      (tester) async {
        final at = await open(tester);

        // 사각형 배치
        await tester.tap(find.byKey(const Key('toolRect')));
        await tester.pumpAndSettle();
        await tester.tapAt(at(4, 4));
        await tester.pump(const Duration(milliseconds: 600));
        await tester.pumpAndSettle();

        // Case 1: 저장 실패 시 초기화 안 됨
        saveOk = false;
        await tester.tap(find.byKey(const Key('newSketchButton')));
        await tester.pumpAndSettle();
        await tester.tap(find.byKey(const Key('sketchNewSaveButton')));
        await tester.pump(const Duration(milliseconds: 600));
        await tester.pumpAndSettle();

        // 저장 실패 메시지가 뜨고 초기화되지 않음
        expect(find.textContaining('저장 실패'), findsOneWidget);
        expect((previews.last['lines'] as List).isNotEmpty, isTrue);

        // Case 2: 이름이 비어있을 때 이름을 먼저 묻고, 입력 후 저장 성공 시 초기화됨
        saveOk = true;
        await tester.enterText(find.byKey(const Key('sketchNameField')), '');
        await tester.pumpAndSettle();

        await tester.tap(find.byKey(const Key('newSketchButton')));
        await tester.pumpAndSettle();
        await tester.tap(find.byKey(const Key('sketchNewSaveButton')));
        await tester.pumpAndSettle();

        // 이름 묻는 대화상자 확인
        expect(find.text('표 서식 저장'), findsOneWidget);
        await tester.enterText(find.byKey(const Key('sketchSaveNameInput')), '새로저장된서식');
        await tester.pumpAndSettle();
        await tester.tap(find.widgetWithText(FilledButton, '저장'));
        await tester.pump(const Duration(milliseconds: 600));
        await tester.pumpAndSettle();

        // 저장 API 호출 확인 및 초기화 검증
        final saveCalls = calls
            .where((c) => c[0] == 'table_sketch' && c[1] == 'save')
            .toList();
        expect(saveCalls.isNotEmpty, isTrue);
        final lastSavePayload = saveCalls.last[2] as Map;
        expect(lastSavePayload['name'], '새로저장된서식');

        // 초기화 완료 확인
        expect(
          tester.widget<TextField>(find.byKey(const Key('sketchNameField'))).controller?.text,
          '새 표 서식',
        );
        expect(previews.last['lines'] as List, isEmpty);
        expect(tester.takeException(), isNull);
      },
    );
  });
}
