import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:docfit_ui/features/qms_bench/qms_bench_screen.dart';
import 'package:docfit_ui/features/workspace/tab_navigation_shell.dart';

void main() {
  testWidgets('개발자 모드가 꺼져 있으면 사이드메뉴에서 검색 벤치마크(15)가 노출되지 않는다', (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        home: TabNavigationShell(
          developerMode: false,
          documentEditor: const SizedBox(),
          tableTemplates: const SizedBox(),
          tableSketch: const SizedBox(),
          diagram: const SizedBox(),
          mindMap: const SizedBox(),
          fileConversion: const SizedBox(),
          paint: const SizedBox(),
          fileRename: const SizedBox(),
          timemachine: const SizedBox(),
        ),
      ),
    );
    await tester.pump();

    expect(find.byKey(const Key('workspaceTab-15')), findsNothing);
    expect(find.text('검색 벤치마크'), findsNothing);
  });

  testWidgets('개발자 모드가 켜져 있으면 사이드메뉴에 검색 벤치마크(15)가 노출된다', (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        home: TabNavigationShell(
          developerMode: true,
          savedSidebarExpanded: ValueNotifier<bool?>(true),
          documentEditor: const SizedBox(),
          tableTemplates: const SizedBox(),
          tableSketch: const SizedBox(),
          diagram: const SizedBox(),
          mindMap: const SizedBox(),
          fileConversion: const SizedBox(),
          paint: const SizedBox(),
          fileRename: const SizedBox(),
          timemachine: const SizedBox(),
        ),
      ),
    );
    await tester.pump();

    expect(find.byKey(const Key('workspaceTab-15')), findsOneWidget);
    expect(find.text('검색 벤치마크'), findsOneWidget);
  });

  testWidgets('환경 상태 카드 렌더링 및 새로고침', (tester) async {
    final mockDetect = {
      'ok': true,
      'command': 'detect',
      'everything': {
        'installed': true,
        'running': true,
        'status': 'gui_client_running',
        'guidance': 'Everything GUI 클라이언트가 실행 중입니다.',
        'version': '1.4.1.1024',
      },
      'kfind': {
        'running': true,
        'pipe_exists': true,
      },
    };

    Future<dynamic> mockApi(String endpoint, List<dynamic> params) async {
      if (endpoint == 'qms_bench') {
        final payload = params.first as Map;
        if (payload['action'] == 'detect') {
          return {'ok': true, 'data': mockDetect};
        }
        if (payload['action'] == 'list_results') {
          return {'ok': true, 'data': {'results': []}};
        }
      }
      return {'ok': true};
    }

    await tester.pumpWidget(
      MaterialApp(
        home: QmsBenchScreen(api: mockApi),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.byKey(const Key('qmsBenchEnvironmentCard')), findsOneWidget);
    expect(find.text('Everything GUI 실행 중 (IPC 측정 가능)'), findsOneWidget);
    expect(find.text('버전: 1.4.1.1024'), findsOneWidget);
    expect(find.text('색인 파이프 준비 완료'), findsOneWidget);
  });

  testWidgets('시작 및 취소 상태 전환', (tester) async {
    String currentStatus = 'idle';

    Future<dynamic> mockApi(String endpoint, List<dynamic> params) async {
      if (endpoint == 'qms_bench') {
        final payload = params.first as Map;
        final action = payload['action'];
        if (action == 'detect') {
          return {'ok': true, 'data': {}};
        }
        if (action == 'start') {
          currentStatus = 'running';
          return {'ok': true, 'started': true};
        }
        if (action == 'status') {
          return {
            'ok': true,
            'data': {
              'status': currentStatus,
              'events': [
                {
                  'event': 'progress',
                  'phase': 'warm',
                  'query_id': 'q1',
                  'repeat': 1,
                  'latency_ms': 2.34,
                  'result_count': 12,
                }
              ],
            }
          };
        }
        if (action == 'cancel') {
          currentStatus = 'cancelled';
          return {'ok': true, 'cancelled': true};
        }
        if (action == 'list_results') {
          return {'ok': true, 'data': {'results': []}};
        }
      }
      return {'ok': true};
    }

    await tester.pumpWidget(
      MaterialApp(
        home: QmsBenchScreen(api: mockApi),
      ),
    );
    await tester.pumpAndSettle();

    // 시작 버튼 누름
    final startButton = find.byKey(const Key('qmsBenchStartButton'));
    expect(startButton, findsOneWidget);
    await tester.tap(startButton);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 350));

    expect(find.text('측정 진행 중...'), findsOneWidget);
    expect(find.textContaining('현재 질의: q1'), findsOneWidget);

    // 취소 버튼 누름
    final cancelButton = find.byKey(const Key('qmsBenchCancelButton'));
    expect(cancelButton, findsOneWidget);
    await tester.tap(cancelButton);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 350));

    expect(find.text('사용자에 의해 취소됨'), findsOneWidget);
  });

  testWidgets('결과 요약 표 및 측정 불가 사유 표시', (tester) async {
    final mockResult = {
      'schema_version': 1,
      'run': {
        'run_id': 'run-sample-999',
        'adapter': 'everything',
        'everything': {
          'status': 'available',
          'unavailable_reasons': [
            'es.exe not found in PATH',
          ],
        },
      },
      'queries': [
        {
          'digest': {'id': 'q1', 'kind': 'korean_short'},
          'statistics': {'p50_ms': 1.45, 'p95_ms': 2.89},
          'result_count': 50,
          'errors': 0,
        },
      ],
      'resources': {
        'working_set_peak_bytes': 10485760,
        'private_bytes_peak_bytes': 5242880,
        'cpu_user_ms': 10,
        'cpu_kernel_ms': 5,
      },
    };

    Future<dynamic> mockApi(String endpoint, List<dynamic> params) async {
      if (endpoint == 'qms_bench') {
        final payload = params.first as Map;
        final action = payload['action'];
        if (action == 'detect') return {'ok': true, 'data': {}};
        if (action == 'start') return {'ok': true, 'started': true};
        if (action == 'status') {
          return {
            'ok': true,
            'data': {
              'status': 'completed',
              'result': mockResult,
              'events': [],
            }
          };
        }
        if (action == 'list_results') return {'ok': true, 'data': {'results': []}};
      }
      return {'ok': true};
    }

    await tester.pumpWidget(
      MaterialApp(
        home: QmsBenchScreen(api: mockApi),
      ),
    );
    await tester.pumpAndSettle();

    // 시작 눌러서 completed 결과 불러오기
    await tester.tap(find.byKey(const Key('qmsBenchStartButton')));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 350));

    expect(find.byKey(const Key('qmsBenchResultSummaryTable')), findsOneWidget);
    expect(find.text('최근 측정 결과 요약 (run-sample-999)'), findsOneWidget);
    expect(find.text('• es.exe not found in PATH'), findsOneWidget);
    expect(find.text('q1'), findsOneWidget);
    expect(find.text('1.45'), findsOneWidget);
  });
}
