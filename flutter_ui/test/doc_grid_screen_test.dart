import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:docfit_ui/features/doc_grid/doc_grid_screen.dart';

// 1x1 유효 PNG base64 데이터
const _kTestPngBase64 =
    'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==';

void main() {
  testWidgets('문서뷰: 폴더를 선택하면 빈 폴더 안내 문구가 표시된다', (tester) async {
    tester.view.physicalSize = const Size(1200, 800);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);

    final calls = <List<Object?>>[];
    Future<Object?> mockApi(String method, List<Object?> args) async {
      calls.add([method, ...args]);
      if (method == 'doc_grid' && args.isNotEmpty && args[0] == 'list') {
        return {
          'ok': true,
          'documents': <Map<String, Object?>>[],
          'folder': '/test/empty_folder',
        };
      }
      return {'ok': true};
    }

    await tester.pumpWidget(
      MaterialApp(
        home: DocGridScreen(
          api: mockApi,
          directoryPicker: () async => '/test/empty_folder',
        ),
      ),
    );

    // 초기 상태: 폴더 선택 안내
    expect(find.text('폴더를 선택하여 문서를 확인하세요.'), findsOneWidget);
    expect(find.byKey(const ValueKey('docGridPickFolderButton')), findsOneWidget);

    // 폴더 선택 버튼 누름
    await tester.tap(find.byKey(const ValueKey('docGridPickFolderButton')));
    await tester.pumpAndSettle();

    // 백엔드 list 호출 검증
    expect(calls, hasLength(1));
    expect(calls.first[0], 'doc_grid');
    expect(calls.first[1], 'list');

    // 빈 폴더 결과 확인
    expect(find.byKey(const ValueKey('docGridEmptyText')), findsOneWidget);
    expect(find.text('지원하는 문서가 없습니다.'), findsOneWidget);
    expect(find.text('/test/empty_folder'), findsOneWidget);
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
        if (action == 'list') {
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
        if (action == 'list') {
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
}
