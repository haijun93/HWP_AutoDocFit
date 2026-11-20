import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:docfit_ui/features/doc_grid/doc_grid_screen.dart';

// 1x1 유효 PNG base64 데이터
const _kTestPngBase64 =
    'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==';

void main() {
  testWidgets('스캔 중 가짜 백분율 대신 실제 진행 정보를 표시한다', (tester) async {
    tester.view.physicalSize = const Size(1200, 800);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    Future<Object?> mockApi(String method, List<Object?> args) async {
      if (args.first == 'list_default') return {'ok': true, 'task_id': 'scan'};
      if (args.first == 'scan_progress') return {'ok': true, 'state': 'running', 'visited': 120, 'current_folder': 'C:/docs'};
      return {'ok': true};
    }
    await tester.pumpWidget(MaterialApp(home: DocGridScreen(api: mockApi)));
    await tester.pump(const Duration(milliseconds: 150));
    expect(find.textContaining('훑은 항목 120개'), findsOneWidget);
    expect(find.textContaining('99%'), findsNothing);
    expect(find.textContaining('현재 폴더: C:/docs'), findsOneWidget);
    await tester.pumpWidget(const MaterialApp(home: SizedBox.shrink()));
    await tester.pump(const Duration(milliseconds: 140));
    await tester.pump(const Duration(milliseconds: 140));
  });

  testWidgets('문서뷰: 기본 범위를 불러와 빈 결과를 표시하고 폴더 선택 UI를 노출하지 않는다', (tester) async {
    tester.view.physicalSize = const Size(1200, 800);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);

    final calls = <List<Object?>>[];
    Future<Object?> mockApi(String method, List<Object?> args) async {
      calls.add([method, ...args]);
      if (method == 'doc_grid' && args.isNotEmpty && args[0] == 'list_default') {
        return {
          'ok': true,
          'documents': <Map<String, Object?>>[],
        };
      }
      return {'ok': true};
    }

    await tester.pumpWidget(
      MaterialApp(
        home: DocGridScreen(
          api: mockApi,
        ),
      ),
    );
    await tester.pumpAndSettle();

    // 기본 범위 API를 자동 호출하고 폴더 선택 UI는 제공하지 않는다.
    expect(calls.where((c) => c[0] == 'doc_grid' && c[1] == 'list_default'), hasLength(1));
    expect(find.byKey(const ValueKey('docGridPickFolderButton')), findsNothing);

    // 빈 폴더 결과 확인
    expect(find.byKey(const ValueKey('docGridEmptyText')), findsOneWidget);
    expect(find.text('지원하는 문서가 없습니다.'), findsOneWidget);
    expect(find.text('모든 고정 드라이브'), findsOneWidget);
    expect(find.text('0개 문서'), findsOneWidget);
  });

  testWidgets('문서뷰: 문서가 있는 폴더 선택 시 격자로 표시되고 썸네일을 요청한다', (tester) async {
    tester.view.physicalSize = const Size(1200, 800);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);

    final calls = <List<Object?>>[];
    final documents = [
      {
        'path': '/test/docs/보고서.hwpx',
        'name': '보고서.hwpx',
        'stem': '보고서',
        'ext': 'hwpx',
        'size': 20480,
        'mtime': 1700000000.0,
        'cloud': false,
        'has_thumb': true,
      },
      {
        'path': '/test/docs/통계자료.xlsx',
        'name': '통계자료.xlsx',
        'stem': '통계자료',
        'ext': 'xlsx',
        'size': 1048576,
        'mtime': 1700000000.0,
        'cloud': false,
        'has_thumb': false,
      },
      {
        'path': '/test/docs/안내문.pdf',
        'name': '안내문.pdf',
        'stem': '안내문',
        'ext': 'pdf',
        'size': 512,
        'mtime': 1700000000.0,
        'cloud': false,
        'has_thumb': false,
      },
    ];

    Future<Object?> mockApi(String method, List<Object?> args) async {
      calls.add([method, ...args]);
      if (method == 'doc_grid' && args.isNotEmpty) {
        final action = args[0];
        if (action == 'list_default') {
          return {
            'ok': true,
            'documents': documents,
            'folder': '/test/docs',
          };
        }
        if (action == 'thumbnail') {
          return {
            'ok': true,
            'type': 'image',
            'data': _kTestPngBase64,
          };
        }
      }
      return {'ok': true};
    }

    await tester.pumpWidget(
      MaterialApp(
        home: DocGridScreen(
          api: mockApi,
          initialFolder: '/test/docs',
        ),
      ),
    );
    await tester.pumpAndSettle();

    // 1. 문서 목록 및 배지 확인
    expect(find.byKey(const ValueKey('docGridView')), findsOneWidget);
    expect(find.text('보고서.hwpx'), findsOneWidget);
    expect(find.text('통계자료.xlsx'), findsOneWidget);
    expect(find.text('안내문.pdf'), findsOneWidget);
    expect(find.text('3개 문서'), findsOneWidget);

    // 2. 확장자 배지 렌더링 확인
    expect(find.text('HWPX'), findsOneWidget);
    expect(find.text('XLSX'), findsOneWidget);
    expect(find.text('PDF'), findsOneWidget);

    // 3. 파일 크기 표시 확인
    expect(find.text('20.0 KB'), findsOneWidget);
    expect(find.text('1.0 MB'), findsOneWidget);
    expect(find.text('512 B'), findsOneWidget);

    // 4. 화면에 보일 때 썸네일 비동기 요청 발생 확인
    final thumbCalls = calls.where(
      (c) => c.length >= 2 && c[0] == 'doc_grid' && c[1] == 'thumbnail',
    );
    expect(thumbCalls.length, greaterThanOrEqualTo(1));

    // 썸네일 이미지 렌더링 확인
    expect(find.byType(Image), findsWidgets);
  });

  testWidgets('문서뷰: 문서 카드를 더블클릭하면 연결 프로그램으로 열기를 호출한다', (tester) async {
    tester.view.physicalSize = const Size(1200, 800);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);

    final calls = <List<Object?>>[];
    final documents = [
      {
        'path': '/test/docs/문서.hwpx',
        'name': '문서.hwpx',
        'stem': '문서',
        'ext': 'hwpx',
        'size': 1024,
        'mtime': 1700000000.0,
        'cloud': false,
        'has_thumb': false,
      },
    ];

    Future<Object?> mockApi(String method, List<Object?> args) async {
      calls.add([method, ...args]);
      if (method == 'doc_grid' && args.isNotEmpty) {
        final action = args[0];
        if (action == 'list_default') {
          return {
            'ok': true,
            'documents': documents,
            'folder': '/test/docs',
          };
        }
        if (action == 'open') {
          final payload = args.length > 1 && args[1] is Map ? args[1] as Map : {};
          return {
            'ok': true,
            'path': payload['path'],
          };
        }
      }
      return {'ok': true};
    }

    await tester.pumpWidget(
      MaterialApp(
        home: DocGridScreen(
          api: mockApi,
          initialFolder: '/test/docs',
        ),
      ),
    );
    await tester.pumpAndSettle();

    // 문서 카드 찾기
    final cardFinder = find.byKey(const ValueKey('docCard-/test/docs/문서.hwpx'));
    expect(cardFinder, findsOneWidget);

    // 더블클릭 시뮬레이션
    await tester.tap(cardFinder);
    await tester.pump(const Duration(milliseconds: 50));
    await tester.tap(cardFinder);
    await tester.pumpAndSettle();

    // open 호출 확인
    final openCalls = calls.where(
      (c) => c.length >= 2 && c[0] == 'doc_grid' && c[1] == 'open',
    );
    expect(openCalls, hasLength(1));
    final openPayload = openCalls.first[2] as Map;
    expect(openPayload['path'], '/test/docs/문서.hwpx');
  });

  testWidgets('문서뷰: 백그라운드 진행·취소와 제외 폴더 편집을 제공한다', (tester) async {
    tester.view.physicalSize = const Size(1200, 800);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);

    var cancelled = false;
    final calls = <String>[];
    Future<Object?> mockApi(String method, List<Object?> args) async {
      final action = args.first as String;
      calls.add(action);
      if (action == 'list_default') return {'ok': true, 'task_id': 'doc-task'};
      if (action == 'scan_progress') {
        return {
          'ok': true,
          'state': cancelled ? 'cancelled' : 'running',
          'visited': 17,
          if (cancelled) 'result': {'documents': <Map<String, Object?>>[]},
        };
      }
      if (action == 'excluded_folders_get') return {'ok': true, 'folders': <String>[]};
      if (action == 'excluded_folder_add') return {'ok': true, 'folders': [args[1] is Map ? (args[1] as Map)['folder'] : '']};
      if (action == 'excluded_folder_remove') return {'ok': true, 'folders': <String>[]};
      if (action == 'scan_cancel') {
        cancelled = true;
        return {'ok': true};
      }
      return {'ok': true};
    }

    await tester.pumpWidget(MaterialApp(home: DocGridScreen(
      api: mockApi,
      directoryPicker: () async => '/test/excluded',
    )));
    await tester.pump(const Duration(milliseconds: 140));
    expect(find.textContaining('훑은 항목 17개'), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('docGridCancelScanButton')));
    await tester.pump(const Duration(milliseconds: 140));
    expect(calls, contains('scan_cancel'));
    expect(find.byKey(const ValueKey('docGridExcludedFoldersButton')), findsOneWidget);

    await tester.tap(find.byKey(const ValueKey('docGridExcludedFoldersButton')));
    await tester.pumpAndSettle();
    await tester.tap(find.text('폴더 추가'));
    await tester.pumpAndSettle();
    expect(calls, contains('excluded_folder_add'));
    expect(find.text('/test/excluded'), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('docGridRemoveExcluded-0')));
    await tester.pumpAndSettle();
    expect(calls, contains('excluded_folder_remove'));
    expect(find.text('/test/excluded'), findsNothing);
  });

  testWidgets('문서뷰: 썸네일 크기 슬라이더가 격자 크기를 바꾸고 저장하며 다시 열면 복원한다', (tester) async {
    tester.view.physicalSize = const Size(1200, 800);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);

    final docs = [
      for (var i = 0; i < 6; i++)
        {
          'path': '/test/docs/문서$i.pdf',
          'name': '문서$i.pdf',
          'stem': '문서$i',
          'ext': 'pdf',
          'size': 512,
          'mtime': 1700000000.0,
          'cloud': true,
        },
    ];
    double? stored;
    final calls = <List<Object?>>[];
    Future<Object?> mockApi(String method, List<Object?> args) async {
      calls.add([method, ...args]);
      switch (args.first) {
        case 'list_default':
          return {'ok': true, 'documents': docs};
        case 'thumb_size_get':
          return {'ok': true, 'size': stored ?? 190.0};
        case 'thumb_size_set':
          stored = ((args[1] as Map)['size'] as num).toDouble();
          return {'ok': true, 'size': stored};
      }
      return {'ok': true};
    }

    double cardWidth() => tester.getSize(find.byKey(const ValueKey('docCard-/test/docs/문서0.pdf'))).width;
    double extent() {
      final grid = tester.widget<GridView>(find.byKey(const ValueKey('docGridView')));
      return (grid.gridDelegate as SliverGridDelegateWithMaxCrossAxisExtent).maxCrossAxisExtent;
    }

    await tester.pumpWidget(MaterialApp(home: DocGridScreen(api: mockApi)));
    await tester.pumpAndSettle();
    expect(extent(), 190.0);
    final before = cardWidth();
    expect(before, lessThanOrEqualTo(190.0));

    // 슬라이더를 최소로 → 카드 폭 감소, 목록 유지, 저장 호출
    final slider = find.byKey(const ValueKey('docGridThumbSizeSlider'));
    expect(slider, findsOneWidget);
    await tester.drag(slider, const Offset(-200, 0));
    await tester.pumpAndSettle();
    expect(extent(), 100.0);
    expect(cardWidth(), lessThan(before));
    expect(find.text('문서5.pdf'), findsOneWidget);
    expect(stored, 100.0);

    // 다시 열면 저장값 복원
    await tester.pumpWidget(const MaterialApp(home: SizedBox.shrink()));
    await tester.pumpWidget(MaterialApp(home: DocGridScreen(api: mockApi)));
    await tester.pumpAndSettle();
    expect(extent(), 100.0);
    expect(calls.where((c) => c[1] == 'thumb_size_get'), hasLength(2));
  });
}
