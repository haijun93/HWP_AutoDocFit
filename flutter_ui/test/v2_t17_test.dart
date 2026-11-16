import 'package:docfit_ui/main.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('T17 원본·결과 비교 버튼 제거 회귀 시험 및 기존 결과 버튼 동작 (결과 화면)', () {
    testWidgets(
      'T17 결과 화면에서 「원본·결과 비교」 버튼 부재(findsNothing) 검증 및 단일 결과 열기·폴더 보기 검증 (폭 420px)',
      (tester) async {
        tester.view.physicalSize = const Size(420, 800);
        tester.view.devicePixelRatio = 1.0;
        addTearDown(() {
          tester.view.resetPhysicalSize();
          tester.view.resetDevicePixelRatio();
        });

        final calledCalls = <Map<String, Object?>>[];

        Future<Object?> api(String method, List<Object?> args) async {
          calledCalls.add({'method': method, 'args': args});

          if (method == 'get_state') {
            return {
              'status': '처리 완료',
              'files': [
                {'name': '보고서.hwpx', 'path': r'C:\doc\보고서.hwpx'},
              ],
              'results': [
                {
                  'name': '보고서(자간조정).hwpx',
                  'path': r'C:\doc\보고서(자간조정).hwpx',
                  'source': '보고서.hwpx',
                  'pages': 3,
                },
              ],
              'completion': {
                'message': '처리 완료 · 성공 1개 / 실패 0개',
                'details': ['성공 1개 · 실패 0개'],
              },
              'card_options': {},
            };
          }
          return {};
        }

        await tester.pumpWidget(DocFitApp(api: api));
        await tester.pumpAndSettle();

        // 4번째 탭 '4. 결과 확인'으로 이동
        expect(find.text('4. 결과 확인'), findsOneWidget);
        await tester.tap(find.text('4. 결과 확인'));
        await tester.pumpAndSettle();

        // 회귀 검증: 「원본·결과 비교」 버튼 및 관련 위젯들이 표시되지 않는지(findsNothing) 확인
        expect(find.text('원본·결과 비교'), findsNothing);
        expect(find.byKey(const Key('compareResultButton')), findsNothing);
        expect(find.byKey(const Key('compareItem-0')), findsNothing);
        expect(find.byKey(const Key('compareErrorMessage')), findsNothing);

        // 420px 폭에서 오버플로 없음 검증
        expect(tester.takeException(), isNull);

        // 결과 목록 아이템의 '결과 파일 열기' IconButton 확인 및 탭
        final openItemFinder = find.byTooltip('결과 파일 열기');
        expect(openItemFinder, findsOneWidget);
        await tester.ensureVisible(openItemFinder);
        await tester.pumpAndSettle();
        await tester.tap(openItemFinder);
        await tester.pumpAndSettle();

        expect(
          calledCalls.any(
            (c) => c['method'] == 'open_result' && (c['args'] as List)[0] == 0,
          ),
          isTrue,
        );

        // 결과 목록 아이템의 '폴더에서 보기' IconButton 확인 및 탭
        final showItemFinder = find.byTooltip('폴더에서 보기');
        expect(showItemFinder, findsOneWidget);
        await tester.ensureVisible(showItemFinder);
        await tester.pumpAndSettle();
        await tester.tap(showItemFinder);
        await tester.pumpAndSettle();

        expect(
          calledCalls.any(
            (c) => c['method'] == 'show_result' && (c['args'] as List)[0] == 0,
          ),
          isTrue,
        );

        // 하단 '결과 파일 열기' FilledButton 확인 및 탭 (단일 결과)
        final openResultsFinder = find.widgetWithText(
          FilledButton,
          '결과 파일 열기',
        );
        expect(openResultsFinder, findsOneWidget);
        await tester.ensureVisible(openResultsFinder);
        await tester.pumpAndSettle();
        await tester.tap(openResultsFinder);
        await tester.pumpAndSettle();

        expect(
          calledCalls.any((c) => c['method'] == 'open_results'),
          isTrue,
        );
        expect(tester.takeException(), isNull);
      },
    );

    testWidgets(
      'T17 결과 2개 이상일 때 「결과 모두 열기」 정상 표시·호출 및 비교 버튼 부재 검증 (폭 420px)',
      (tester) async {
        tester.view.physicalSize = const Size(420, 800);
        tester.view.devicePixelRatio = 1.0;
        addTearDown(() {
          tester.view.resetPhysicalSize();
          tester.view.resetDevicePixelRatio();
        });

        final calledCalls = <Map<String, Object?>>[];

        Future<Object?> api(String method, List<Object?> args) async {
          calledCalls.add({'method': method, 'args': args});

          if (method == 'get_state') {
            return {
              'status': '처리 완료',
              'files': [
                {'name': 'doc1.hwpx', 'path': r'C:\doc1.hwpx'},
                {'name': 'doc2.hwpx', 'path': r'C:\doc2.hwpx'},
              ],
              'results': [
                {
                  'name': 'doc1(결과).hwpx',
                  'path': r'C:\doc1(결과).hwpx',
                  'source': 'doc1.hwpx',
                  'pages': 1,
                },
                {
                  'name': 'doc2(결과).hwpx',
                  'path': r'C:\doc2(결과).hwpx',
                  'source': 'doc2.hwpx',
                  'pages': 2,
                },
              ],
              'completion': {'message': '처리 완료'},
              'card_options': {},
            };
          }
          return {};
        }

        await tester.pumpWidget(DocFitApp(api: api));
        await tester.pumpAndSettle();

        await tester.tap(find.text('4. 결과 확인'));
        await tester.pumpAndSettle();

        // 비교 버튼 부재 확인
        expect(find.text('원본·결과 비교'), findsNothing);
        expect(find.byKey(const Key('compareResultButton')), findsNothing);
        expect(find.byKey(const Key('compareItem-0')), findsNothing);
        expect(find.byKey(const Key('compareItem-1')), findsNothing);

        // 하단 '결과 모두 열기' FilledButton 확인 및 탭
        final openAllFinder = find.widgetWithText(
          FilledButton,
          '결과 모두 열기',
        );
        expect(openAllFinder, findsOneWidget);
        await tester.ensureVisible(openAllFinder);
        await tester.pumpAndSettle();
        await tester.tap(openAllFinder);
        await tester.pumpAndSettle();

        expect(
          calledCalls.any((c) => c['method'] == 'open_results'),
          isTrue,
        );
        expect(tester.takeException(), isNull);
      },
    );

    testWidgets('T17 결과 목록이 없을 때 결과 열기 버튼 비활성화 및 비교 버튼 부재 검증', (tester) async {
      tester.view.physicalSize = const Size(420, 800);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(() {
        tester.view.resetPhysicalSize();
        tester.view.resetDevicePixelRatio();
      });

      Future<Object?> api(String method, List<Object?> args) async {
        if (method == 'get_state') {
          return {
            'status': '대기 중',
            'files': [],
            'results': [],
            'completion': {},
            'card_options': {},
          };
        }
        return {};
      }

      await tester.pumpWidget(DocFitApp(api: api));
      await tester.pumpAndSettle();

      await tester.tap(find.text('4. 결과 확인'));
      await tester.pumpAndSettle();

      // 비교 버튼 부재 확인
      expect(find.text('원본·결과 비교'), findsNothing);
      expect(find.byKey(const Key('compareResultButton')), findsNothing);

      // results가 없으므로 FilledButton('결과 파일 열기')이 비활성화(onPressed == null) 상태여야 함
      final button = tester.widget<FilledButton>(
        find.widgetWithText(FilledButton, '결과 파일 열기'),
      );
      expect(button.onPressed, isNull);
      expect(tester.takeException(), isNull);
    });
  });
}
