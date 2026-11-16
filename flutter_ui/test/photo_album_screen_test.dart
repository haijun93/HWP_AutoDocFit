import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:docfit_ui/features/photo_album/photo_album_screen.dart';
import 'package:docfit_ui/features/workspace/tab_navigation_shell.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  // 테스트용 1x1 투명 PNG base64
  const testBase64Png =
      'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==';

  group('PhotoAlbumScreen 위젯 시험', () {
    testWidgets('앱 왼쪽 탭 목록에 사진뷰 탭 노출 및 전환 확인', (tester) async {
      tester.view.physicalSize = const Size(1200, 800);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);

      final calls = <String>[];
      Future<Object?> mockApi(String endpoint, List<Object?> params) async {
        calls.add('$endpoint:${params.first}');
        if (endpoint == 'photo_album') {
          final action = params.first as String;
          if (action == 'list_folders') {
            return {'ok': true, 'folders': <Map<String, Object?>>[]};
          }
        }
        return {'ok': true};
      }

      await tester.pumpWidget(
        MaterialApp(
          home: TabNavigationShell(
            documentEditor: const SizedBox(),
            tableTemplates: const SizedBox(),
            tableSketch: const SizedBox(),
            diagram: const SizedBox(),
            mindMap: const SizedBox(),
            fileConversion: const SizedBox(),
            paint: const SizedBox(),
            fileRename: const SizedBox(),
            timemachine: const SizedBox(),
            docGrid: const SizedBox(),
            photoAlbum: PhotoAlbumScreen(api: mockApi),
            initialIndex: kTabPhotoAlbum,
          ),
        ),
      );
      await tester.pumpAndSettle();

      // 사진뷰 탭 텍스트 확인
      expect(find.text('사진뷰'), findsOneWidget);
      // 사진뷰 화면의 폴더 선택 단추 확인
      expect(find.byKey(const ValueKey('photoAlbumPickFolderButton')), findsOneWidget);
    });

    testWidgets('등록된 폴더가 없을 때 안내 문구 노출', (tester) async {
      tester.view.physicalSize = const Size(1000, 700);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);

      Future<Object?> mockApi(String endpoint, List<Object?> params) async {
        if (endpoint == 'photo_album' && params.first == 'list_folders') {
          return {'ok': true, 'folders': <Map<String, Object?>>[]};
        }
        return {'ok': true};
      }

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: PhotoAlbumScreen(api: mockApi),
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('사진 폴더를 선택하거나 등록하세요.'), findsOneWidget);
      expect(find.byKey(const ValueKey('photoAlbumEmptyFolderPrompt')), findsOneWidget);
    });

    testWidgets('연·월 묶음 격자 표시 및 썸네일 API 비동기 요청', (tester) async {
      tester.view.physicalSize = const Size(1000, 700);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);

      final apiCalls = <Map<String, Object?>>[];
      Future<Object?> mockApi(String endpoint, List<Object?> params) async {
        final action = params.first as String;
        final payload = params.length > 1 && params[1] is Map
            ? (params[1] as Map).cast<String, Object?>()
            : <String, Object?>{};
        apiCalls.add({'action': action, 'payload': payload});

        if (action == 'list_folders') {
          return {
            'ok': true,
            'folders': [
              {'path': '/test/photos', 'registered_at': '2026-11-17T00:00:00'}
            ]
          };
        }
        if (action == 'list_by_date') {
          return {
            'ok': true,
            'folder': '/test/photos',
            'groups': [
              {
                'year': 2026,
                'month': 11,
                'label': '2026년 11월',
                'photos': [
                  {
                    'path': '/test/photos/IMG_0001.JPG',
                    'folder': '/test/photos',
                    'filename': 'IMG_0001.JPG',
                    'size': 204800,
                    'mtime': 1731700000.0,
                    'date_taken': '2026-11-15 10:00:00',
                    'orientation': 1,
                    'favorite': false,
                    'cloud': false,
                    'tags': ['공사', '착공'],
                  },
                  {
                    'path': '/test/photos/IMG_0002.JPG',
                    'folder': '/test/photos',
                    'filename': 'IMG_0002.JPG',
                    'size': 409600,
                    'mtime': 1731710000.0,
                    'date_taken': '2026-11-16 11:30:00',
                    'orientation': 1,
                    'favorite': true,
                    'cloud': false,
                    'tags': ['배근'],
                  },
                ],
              }
            ],
          };
        }
        if (action == 'thumbnail') {
          return {
            'ok': true,
            'path': payload['path'],
            'data': testBase64Png,
          };
        }
        return {'ok': true};
      }

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: PhotoAlbumScreen(api: mockApi),
          ),
        ),
      );
      await tester.pumpAndSettle();

      // 연·월 그룹 헤더 확인
      expect(find.byKey(const ValueKey('photoGroupHeader-2026년 11월')), findsOneWidget);
      expect(find.text('2026년 11월 (2장)'), findsOneWidget);

      // 사진 카드 확인
      expect(find.byKey(const ValueKey('photoCard-/test/photos/IMG_0001.JPG')), findsOneWidget);
      expect(find.byKey(const ValueKey('photoCard-/test/photos/IMG_0002.JPG')), findsOneWidget);

      // 썸네일 API 호출 확인
      final thumbCalls = apiCalls.where((c) => c['action'] == 'thumbnail').toList();
      expect(thumbCalls.length, greaterThanOrEqualTo(2));
      expect(
        thumbCalls.any((c) => (c['payload'] as Map)['path'] == '/test/photos/IMG_0001.JPG'),
        isTrue,
      );
    });

    testWidgets('사진 클릭 시 크게 보기 모달 및 키보드 좌/우/Esc 조작', (tester) async {
      tester.view.physicalSize = const Size(1000, 700);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);

      Future<Object?> mockApi(String endpoint, List<Object?> params) async {
        final action = params.first as String;
        if (action == 'list_folders') {
          return {
            'ok': true,
            'folders': [
              {'path': '/test/photos'}
            ]
          };
        }
        if (action == 'list_by_date') {
          return {
            'ok': true,
            'folder': '/test/photos',
            'groups': [
              {
                'year': 2026,
                'month': 11,
                'label': '2026년 11월',
                'photos': [
                  {
                    'path': '/test/photos/photo1.jpg',
                    'filename': 'photo1.jpg',
                    'date_taken': '2026-11-10 09:00:00',
                    'size': 1024,
                    'favorite': false,
                    'tags': ['사전'],
                  },
                  {
                    'path': '/test/photos/photo2.jpg',
                    'filename': 'photo2.jpg',
                    'date_taken': '2026-11-11 14:00:00',
                    'size': 2048,
                    'favorite': true,
                    'tags': ['완료'],
                  },
                ],
              }
            ],
          };
        }
        if (action == 'thumbnail') {
          return {'ok': true, 'data': testBase64Png};
        }
        return {'ok': true};
      }

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: PhotoAlbumScreen(api: mockApi),
          ),
        ),
      );
      await tester.pumpAndSettle();

      // 첫 번째 사진 클릭
      await tester.tap(find.byKey(const ValueKey('photoCard-/test/photos/photo1.jpg')));
      await tester.pumpAndSettle();

      // 크게 보기 모달 확인
      expect(find.byKey(const ValueKey('photoViewerModal')), findsOneWidget);
      expect(find.byKey(const ValueKey('photoViewerIndexText')), findsOneWidget);
      expect(find.text('1 / 2'), findsOneWidget);
      expect(
        find.descendant(
          of: find.byKey(const ValueKey('photoViewerModal')),
          matching: find.text('photo1.jpg'),
        ),
        findsOneWidget,
      );

      // 키보드 오른쪽 화살표 키로 다음 사진 이동
      await tester.sendKeyEvent(LogicalKeyboardKey.arrowRight);
      await tester.pumpAndSettle();
      expect(find.text('2 / 2'), findsOneWidget);
      expect(
        find.descendant(
          of: find.byKey(const ValueKey('photoViewerModal')),
          matching: find.text('photo2.jpg'),
        ),
        findsOneWidget,
      );

      // 키보드 왼쪽 화살표 키로 이전 사진 이동
      await tester.sendKeyEvent(LogicalKeyboardKey.arrowLeft);
      await tester.pumpAndSettle();
      expect(find.text('1 / 2'), findsOneWidget);
      expect(
        find.descendant(
          of: find.byKey(const ValueKey('photoViewerModal')),
          matching: find.text('photo1.jpg'),
        ),
        findsOneWidget,
      );

      // 키보드 Esc 키로 닫기
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('photoViewerModal')), findsNothing);
    });

    testWidgets('즐겨찾기 토글, 태그 추가/삭제, 검색 API 호출 확인', (tester) async {
      tester.view.physicalSize = const Size(1200, 800);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);

      final apiCalls = <Map<String, Object?>>[];
      Future<Object?> mockApi(String endpoint, List<Object?> params) async {
        final action = params.first as String;
        final payload = params.length > 1 && params[1] is Map
            ? (params[1] as Map).cast<String, Object?>()
            : <String, Object?>{};
        apiCalls.add({'action': action, 'payload': payload});

        if (action == 'list_folders') {
          return {
            'ok': true,
            'folders': [
              {'path': '/test/photos'}
            ]
          };
        }
        if (action == 'list_by_date') {
          return {
            'ok': true,
            'folder': '/test/photos',
            'groups': [
              {
                'year': 2026,
                'month': 11,
                'label': '2026년 11월',
                'photos': [
                  {
                    'path': '/test/photos/photo1.jpg',
                    'filename': 'photo1.jpg',
                    'date_taken': '2026-11-10 09:00:00',
                    'size': 1024,
                    'favorite': false,
                    'tags': ['현장'],
                  },
                ],
              }
            ],
          };
        }
        if (action == 'toggle_favorite') {
          return {'ok': true, 'favorite': true, 'path': payload['path']};
        }
        if (action == 'add_tag') {
          return {'ok': true, 'tags': ['현장', '1구역'], 'path': payload['path']};
        }
        if (action == 'remove_tag') {
          return {'ok': true, 'tags': <String>[], 'path': payload['path']};
        }
        if (action == 'search') {
          return {
            'ok': true,
            'results': [
              {
                'path': '/test/photos/search_result.jpg',
                'filename': 'search_result.jpg',
                'date_taken': '2026-11-12 10:00:00',
                'size': 2048,
                'favorite': false,
                'tags': ['검색태그'],
              }
            ],
          };
        }
        if (action == 'thumbnail') {
          return {'ok': true, 'data': testBase64Png};
        }
        return {'ok': true};
      }

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: PhotoAlbumScreen(api: mockApi),
          ),
        ),
      );
      await tester.pumpAndSettle();

      // 1. 사진 카드의 즐겨찾기 별표 클릭
      await tester.tap(find.byKey(const ValueKey('photoCardFavorite-/test/photos/photo1.jpg')));
      await tester.pumpAndSettle();

      expect(apiCalls.any((c) => c['action'] == 'toggle_favorite'), isTrue);

      // 2. 모달 열고 태그 추가 및 삭제
      await tester.tap(find.byKey(const ValueKey('photoCard-/test/photos/photo1.jpg')));
      await tester.pumpAndSettle();

      // 태그 입력
      await tester.enterText(find.byKey(const ValueKey('photoViewerTagField')), '1구역');
      await tester.tap(find.byKey(const ValueKey('photoViewerAddTagButton')));
      await tester.pumpAndSettle();

      final addTagCall = apiCalls.firstWhere((c) => c['action'] == 'add_tag');
      expect((addTagCall['payload'] as Map)['tag'], '1구역');

      // 기존 태그 삭제 (현장 칩의 닫기 아이콘 클릭)
      expect(find.byKey(const ValueKey('photoViewerTagChip-현장')), findsOneWidget);
      await tester.tap(find.descendant(
        of: find.byKey(const ValueKey('photoViewerTagChip-현장')),
        matching: find.byIcon(Icons.close),
      ));
      await tester.pumpAndSettle();

      final removeTagCall = apiCalls.firstWhere((c) => c['action'] == 'remove_tag');
      expect((removeTagCall['payload'] as Map)['tag'], '현장');

      // 모달 닫기
      await tester.tap(find.byKey(const ValueKey('photoViewerCloseButton')));
      await tester.pumpAndSettle();

      // 3. 파일명/태그 검색
      await tester.enterText(find.byKey(const ValueKey('photoAlbumSearchField')), '검색어');
      await tester.tap(find.byKey(const ValueKey('photoAlbumSearchSubmitButton')));
      await tester.pumpAndSettle();

      final searchCall = apiCalls.firstWhere((c) => c['action'] == 'search');
      expect((searchCall['payload'] as Map)['query'], '검색어');
      expect(find.byKey(const ValueKey('photoAlbumSearchResultsView')), findsOneWidget);
      expect(find.text('검색 결과: 1장'), findsOneWidget);
      expect(find.text('search_result.jpg'), findsOneWidget);
    });

    testWidgets('다중 선택 및 공문 첨부용 사본 저장·사진대지 만들기 UI', (tester) async {
      tester.view.physicalSize = const Size(1200, 800);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);

      final apiCalls = <Map<String, Object?>>[];
      Future<Object?> mockApi(String endpoint, List<Object?> params) async {
        final action = params.first as String;
        final payload = params.length > 1 && params[1] is Map
            ? (params[1] as Map).cast<String, Object?>()
            : <String, Object?>{};
        apiCalls.add({'action': action, 'payload': payload});

        if (action == 'list_folders') {
          return {
            'ok': true,
            'folders': [
              {'path': '/test/photos'}
            ]
          };
        }
        if (action == 'list_by_date') {
          return {
            'ok': true,
            'folder': '/test/photos',
            'groups': [
              {
                'year': 2026,
                'month': 11,
                'label': '2026년 11월',
                'photos': [
                  {
                    'path': '/test/photos/img1.jpg',
                    'filename': 'img1.jpg',
                    'date_taken': '2026-11-10',
                    'size': 1000,
                  },
                  {
                    'path': '/test/photos/img2.jpg',
                    'filename': 'img2.jpg',
                    'date_taken': '2026-11-11',
                    'size': 2000,
                  },
                ],
              }
            ],
          };
        }
        if (action == 'batch_export') {
          return {'ok': true, 'results': ['/export/img1.jpg', '/export/img2.jpg']};
        }
        if (action == 'build_photo_sheet') {
          return {'ok': true, 'output_path': payload['output_path'], 'count': 2};
        }
        if (action == 'thumbnail') {
          return {'ok': true, 'data': testBase64Png};
        }
        return {'ok': true};
      }

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: PhotoAlbumScreen(
              api: mockApi,
              directoryPicker: () async => '/mock/export_dir',
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      // 다중 선택 모드 토글
      await tester.tap(find.byKey(const ValueKey('photoAlbumToggleSelectionButton')));
      await tester.pumpAndSettle();

      expect(find.text('0장 선택됨'), findsOneWidget);

      // 전체 선택 클릭
      await tester.tap(find.byKey(const ValueKey('photoAlbumSelectAllButton')));
      await tester.pumpAndSettle();

      expect(find.text('2장 선택됨'), findsOneWidget);

      // 1. 공문 첨부용 사본 저장 클릭
      await tester.tap(find.byKey(const ValueKey('photoAlbumBatchExportButton')));
      await tester.pumpAndSettle();

      expect(apiCalls.any((c) => c['action'] == 'batch_export'), isTrue);
      final exportCall = apiCalls.firstWhere((c) => c['action'] == 'batch_export');
      expect((exportCall['payload'] as Map)['output_dir'], '/mock/export_dir');

      // 다시 2장 선택
      await tester.tap(find.byKey(const ValueKey('photoAlbumSelectAllButton')));
      await tester.pumpAndSettle();

      // 2. 사진대지 만들기 클릭
      await tester.tap(find.byKey(const ValueKey('photoAlbumBuildSheetButton')));
      await tester.pumpAndSettle();

      // 사진대지 대화상자 확인
      expect(find.text('사진대지 만들기'), findsOneWidget);
      expect(find.byKey(const ValueKey('photoSheetLayout2x2')), findsOneWidget);
      expect(find.byKey(const ValueKey('photoSheetLayout2x3')), findsOneWidget);

      // 2x3 레이아웃 선택
      await tester.tap(find.byKey(const ValueKey('photoSheetLayout2x3')));
      await tester.pumpAndSettle();

      // 생성 버튼 클릭
      await tester.tap(find.byKey(const ValueKey('photoAlbumConfirmBuildSheetButton')));
      await tester.pumpAndSettle();

      expect(apiCalls.any((c) => c['action'] == 'build_photo_sheet'), isTrue);
      final sheetCall = apiCalls.firstWhere((c) => c['action'] == 'build_photo_sheet');
      expect((sheetCall['payload'] as Map)['layout'], '2x3');
    });
  });
}
