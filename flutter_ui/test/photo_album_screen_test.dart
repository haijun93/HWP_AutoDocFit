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
    testWidgets('태그 필터와 선택 사진 태그 편집 패널 연결', (tester) async {
      tester.view.physicalSize = const Size(1100, 760);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      final actions = <String>[];
      Future<Object?> mockApi(String endpoint, List<Object?> params) async {
        final action = params.first as String;
        actions.add(action);
        if (action == 'list_folders') {
          return {
            'ok': true,
            'folders': [
              {'path': '/photos'},
            ],
          };
        }
        if (action == 'list_by_date') {
          return {
            'ok': true,
            'groups': [
              {
                'year': 2026,
                'month': 11,
                'label': '2026년 11월',
                'photos': [
                  {
                    'path': '/photos/a.jpg',
                    'filename': 'a.jpg',
                    'date_taken': '',
                    'tags': ['기존'],
                  },
                ],
              },
            ],
          };
        }
        if (action == 'list_tags') {
          return {
            'ok': true,
            'tags': [
              {'tag': '기존', 'count': 1},
            ],
          };
        }
        if (action == 'add_tag') {
          return {
            'ok': true,
            'tags': ['기존', '새 태그'],
          };
        }
        if (action == 'thumbnail') return {'ok': true, 'data': testBase64Png};
        return {'ok': true};
      }

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(body: PhotoAlbumScreen(api: mockApi)),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('photoTagFilter-기존')), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('photoTagFilter-기존')));
      await tester.pumpAndSettle();
      expect(actions, contains('search'));
      await tester.tap(find.byKey(const ValueKey('photoTagFilter-기존')));
      await tester.pumpAndSettle();
      await tester.tap(
        find.byKey(const ValueKey('photoAlbumToggleSelectionButton')),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const ValueKey('photoCard-/photos/a.jpg')));
      await tester.pumpAndSettle();
      expect(
        find.byKey(const ValueKey('photoAlbumTagEditorPanel')),
        findsOneWidget,
      );
      await tester.enterText(
        find.byKey(const ValueKey('photoAlbumTagInput')),
        '새 태그',
      );
      await tester.tap(find.byKey(const ValueKey('photoAlbumAddTagButton')));
      await tester.pumpAndSettle();
      expect(actions, contains('add_tag'));
      expect(find.text('새 태그'), findsOneWidget);
    });


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
      // 기본 범위를 자동 스캔하고 폴더 선택 UI는 노출하지 않는다.
      expect(calls, contains('photo_album:scan_default'));
      expect(find.byKey(const ValueKey('photoAlbumPickFolderButton')), findsNothing);
    });

    testWidgets('빈 기본 범위 안내 문구 노출', (tester) async {
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

      expect(find.text('등록된 사진이 없습니다.'), findsOneWidget);
      expect(find.byKey(const ValueKey('photoAlbumEmptyPhotosText')), findsOneWidget);
      expect(find.byKey(const ValueKey('photoAlbumPickFolderButton')), findsNothing);
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

    testWidgets('날짜별 → 폴더별/카메라별/위치별 전환 시 place_groups 명령과 group_by 인자 전달 및 그룹 렌더링 검증', (tester) async {
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
                    'path': '/test/photos/p1.jpg',
                    'filename': 'p1.jpg',
                    'date_taken': '2026-11-10',
                    'size': 1024,
                  },
                ],
              }
            ],
          };
        }
        if (action == 'place_groups') {
          final groupBy = payload['group_by'] as String?;
          if (groupBy == 'folder') {
            return {
              'ok': true,
              'group_by': 'folder',
              'groups': [
                {
                  'key': '/test/photos',
                  'label': '현장A폴더',
                  'count': 1,
                  'items': [
                    {
                      'path': '/test/photos/p1.jpg',
                      'name': 'p1.jpg',
                      'date': '2026-11-10',
                    }
                  ],
                }
              ],
            };
          } else if (groupBy == 'camera') {
            return {
              'ok': true,
              'group_by': 'camera',
              'groups': [
                {
                  'key': 'Canon EOS R5',
                  'label': 'Canon EOS R5',
                  'count': 1,
                  'items': [
                    {
                      'path': '/test/photos/p1.jpg',
                      'name': 'p1.jpg',
                      'date': '2026-11-10',
                    }
                  ],
                }
              ],
            };
          } else if (groupBy == 'location') {
            return {
              'ok': true,
              'group_by': 'location',
              'groups': [
                {
                  'key': '37.55,126.95',
                  'label': '위도 37.55°, 경도 126.95°',
                  'count': 1,
                  'items': [
                    {
                      'path': '/test/photos/p1.jpg',
                      'name': 'p1.jpg',
                      'date': '2026-11-10',
                    }
                  ],
                }
              ],
            };
          }
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

      // 초기: 날짜별
      expect(find.text('2026년 11월 (1장)'), findsOneWidget);

      // 1. 폴더별 전환
      await tester.tap(find.text('폴더별'));
      await tester.pumpAndSettle();

      final folderCall = apiCalls.lastWhere((c) => c['action'] == 'place_groups');
      expect((folderCall['payload'] as Map)['group_by'], 'folder');
      expect(find.text('현장A폴더 (1장)'), findsOneWidget);
      expect(find.byKey(const ValueKey('photoCard-/test/photos/p1.jpg')), findsOneWidget);

      // 2. 카메라별 전환
      await tester.tap(find.text('카메라별'));
      await tester.pumpAndSettle();

      final cameraCall = apiCalls.lastWhere((c) => c['action'] == 'place_groups');
      expect((cameraCall['payload'] as Map)['group_by'], 'camera');
      expect(find.text('Canon EOS R5 (1장)'), findsOneWidget);

      // 3. 위치별 전환
      await tester.tap(find.text('위치별'));
      await tester.pumpAndSettle();

      final locationCall = apiCalls.lastWhere((c) => c['action'] == 'place_groups');
      expect((locationCall['payload'] as Map)['group_by'], 'location');
      expect(find.text('위도 37.55°, 경도 126.95° (1장)'), findsOneWidget);

      // 사진 클릭 시 크게 보기 모달 연동 확인
      await tester.tap(find.byKey(const ValueKey('photoCard-/test/photos/p1.jpg')));
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('photoViewerModal')), findsOneWidget);
    });

    testWidgets('중복 사진 찾기 클릭 시 find_duplicates 호출 및 대표 배지/거리·빈 결과 표시 검증', (tester) async {
      tester.view.physicalSize = const Size(1200, 800);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);

      final apiCalls = <Map<String, Object?>>[];
      var returnEmptyDuplicates = false;

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
                    'path': '/test/photos/dup1.jpg',
                    'filename': 'dup1.jpg',
                    'date_taken': '2026-11-10',
                    'size': 2048,
                  },
                  {
                    'path': '/test/photos/dup2.jpg',
                    'filename': 'dup2.jpg',
                    'date_taken': '2026-11-10',
                    'size': 1024,
                  },
                ],
              }
            ],
          };
        }
        if (action == 'find_duplicates') {
          if (returnEmptyDuplicates) {
            return {
              'ok': true,
              'threshold': 6,
              'groups': <Map<String, Object?>>[],
            };
          }
          return {
            'ok': true,
            'threshold': 6,
            'groups': [
              {
                'group_id': 1,
                'count': 2,
                'representative': '/test/photos/dup1.jpg',
                'items': [
                  {
                    'path': '/test/photos/dup1.jpg',
                    'name': 'dup1.jpg',
                    'size': 2048,
                    'date': '2026-11-10',
                    'distance': 0,
                  },
                  {
                    'path': '/test/photos/dup2.jpg',
                    'name': 'dup2.jpg',
                    'size': 1024,
                    'date': '2026-11-10',
                    'distance': 3,
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

      // 중복 사진 찾기 클릭
      await tester.tap(find.byKey(const ValueKey('photoAlbumFindDuplicatesButton')));
      await tester.pumpAndSettle();

      final dupCall = apiCalls.firstWhere((c) => c['action'] == 'find_duplicates');
      expect((dupCall['payload'] as Map)['folder'], '');
      expect((dupCall['payload'] as Map)['threshold'], 6);

      // 대표 배지 및 거리 표시 확인
      expect(find.byKey(const ValueKey('photoAlbumDuplicatesView')), findsOneWidget);
      expect(find.text('대표'), findsOneWidget);
      expect(find.text('거리 0'), findsOneWidget);
      expect(find.text('거리 3'), findsOneWidget);

      // 클릭 시 크게 보기 모달 연동 확인
      await tester.tap(find.byKey(const ValueKey('duplicatePhotoCard-/test/photos/dup2.jpg')));
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('photoViewerModal')), findsOneWidget);
      expect(find.text('2 / 2'), findsOneWidget);

      // 모달 닫기
      await tester.tap(find.byKey(const ValueKey('photoViewerCloseButton')));
      await tester.pumpAndSettle();

      // 빈 결과 안내 검증
      returnEmptyDuplicates = true;
      await tester.tap(find.byKey(const ValueKey('photoAlbumFindDuplicatesButton')));
      await tester.pumpAndSettle();
      expect(find.text('유사·중복 사진이 없습니다.'), findsOneWidget);
    });

    testWidgets('상세 보기에서 사본 편집 열기 → 회전/밝기 조정 → edit_copy 호출(원본 보존 안내 표시) 검증', (tester) async {
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
                    'path': '/test/photos/orig.jpg',
                    'filename': 'orig.jpg',
                    'date_taken': '2026-11-10',
                    'size': 2048,
                  },
                ],
              }
            ],
          };
        }
        if (action == 'edit_copy') {
          return {
            'ok': true,
            'source': payload['path'],
            'output_path': '/test/photos/orig_edited.jpg',
            'width': 800,
            'height': 600,
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

      // 사진 클릭하여 크게 보기 열기
      await tester.tap(find.byKey(const ValueKey('photoCard-/test/photos/orig.jpg')));
      await tester.pumpAndSettle();

      // 사본 편집 버튼 클릭
      await tester.tap(find.byKey(const ValueKey('photoViewerEditCopyButton')));
      await tester.pumpAndSettle();

      // 원본 보존 안내 문구 확인
      expect(find.text('원본 사진은 그대로 보존되고 새 사본으로 저장됩니다'), findsOneWidget);

      // 90° 회전 선택
      await tester.tap(find.byKey(const ValueKey('photoEditRotate90')));
      await tester.pumpAndSettle();

      // 밝기 슬라이더 조정 (오른쪽으로 드래그하여 1.0 초과로 변경)
      final brightnessSlider = find.byKey(const ValueKey('photoEditBrightnessSlider'));
      await tester.drag(brightnessSlider, const Offset(100, 0));
      await tester.pumpAndSettle();

      // 사본으로 저장 클릭
      await tester.tap(find.byKey(const ValueKey('photoEditSaveCopyButton')));
      await tester.pumpAndSettle();

      // edit_copy 호출 및 인자 확인
      final editCall = apiCalls.firstWhere((c) => c['action'] == 'edit_copy');
      final editPayload = editCall['payload'] as Map<String, Object?>;
      expect(editPayload['path'], '/test/photos/orig.jpg');
      expect(editPayload['rotate_deg'], 90);
      expect((editPayload['brightness'] as double) > 1.0, isTrue);

      // 저장 완료 스낵바 확인
      expect(find.textContaining('/test/photos/orig_edited.jpg'), findsOneWidget);
    });

    testWidgets('슬라이드쇼 켜기 → 3초 경과 시 다음 사진 이동 검증', (tester) async {
      tester.view.physicalSize = const Size(1200, 800);
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
                    'path': '/test/photos/slide1.jpg',
                    'filename': 'slide1.jpg',
                    'date_taken': '2026-11-10',
                    'size': 1024,
                  },
                  {
                    'path': '/test/photos/slide2.jpg',
                    'filename': 'slide2.jpg',
                    'date_taken': '2026-11-11',
                    'size': 2048,
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

      // 첫 번째 사진 클릭하여 크게 보기 열기
      await tester.tap(find.byKey(const ValueKey('photoCard-/test/photos/slide1.jpg')));
      await tester.pumpAndSettle();

      expect(find.text('1 / 2'), findsOneWidget);
      expect(find.text('슬라이드쇼'), findsOneWidget);

      // 슬라이드쇼 재생 시작
      await tester.tap(find.byKey(const ValueKey('photoViewerSlideshowButton')));
      await tester.pump();
      expect(find.text('일시정지'), findsOneWidget);

      // 3초 경과 시 다음 사진(2 / 2)으로 자동 이동 확인
      await tester.pump(const Duration(seconds: 3));
      await tester.pump();
      expect(find.text('2 / 2'), findsOneWidget);

      // 추가 3초 경과 시 첫 사진(1 / 2)으로 순환 확인
      await tester.pump(const Duration(seconds: 3));
      await tester.pump();
      expect(find.text('1 / 2'), findsOneWidget);

      // 모달 닫기 (dispose 시 타이머 정상 해제 확인)
      await tester.tap(find.byKey(const ValueKey('photoViewerCloseButton')));
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('photoViewerModal')), findsNothing);
    });

    testWidgets('사진뷰: 백그라운드 진행·취소와 제외 폴더 편집을 제공한다', (tester) async {
      tester.view.physicalSize = const Size(1200, 800);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);

      var cancelled = false;
      final calls = <String>[];
      Future<Object?> mockApi(String endpoint, List<Object?> params) async {
        final action = params.first as String;
        calls.add(action);
        if (action == 'scan_default') {
          return {'ok': true, 'task_id': 'photo-task'};
        }
        if (action == 'scan_progress') {
          return {
            'ok': true,
            'state': cancelled ? 'cancelled' : 'running',
          'visited': 23,
            if (cancelled) 'result': {'scanned': 0, 'added': 0, 'updated': 0},
          };
        }
        if (action == 'scan_cancel') {
          cancelled = true;
          return {'ok': true};
        }
        if (action == 'list_folders') {
          return {'ok': true, 'folders': <Map<String, Object?>>[]};
        }
        if (action == 'list_by_date') {
          return {'ok': true, 'groups': <Map<String, Object?>>[]};
        }
        if (action == 'excluded_folders_get') {
          return {'ok': true, 'folders': <String>[]};
        }
        if (action == 'excluded_folder_add') {
          final payload = params.length > 1 && params[1] is Map ? params[1] as Map : {};
          return {'ok': true, 'folders': [payload['folder']]};
        }
        if (action == 'excluded_folder_remove') {
          return {'ok': true, 'folders': <String>[]};
        }
        return {'ok': true};
      }

      await tester.pumpWidget(MaterialApp(home: Scaffold(body: PhotoAlbumScreen(
        api: mockApi,
        directoryPicker: () async => '/test/photos-excluded',
      ))));
      await tester.pump(const Duration(milliseconds: 140));
      expect(find.textContaining('훑은 항목'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('photoAlbumCancelScanButton')));
      await tester.pump(const Duration(milliseconds: 140));
      await tester.pumpAndSettle();
      expect(calls, contains('scan_cancel'));
      expect(find.byKey(const ValueKey('photoAlbumExcludedFoldersButton')), findsOneWidget);

      await tester.tap(find.byKey(const ValueKey('photoAlbumExcludedFoldersButton')));
      await tester.pumpAndSettle();
      await tester.tap(find.text('폴더 추가'));
      await tester.pumpAndSettle();
      expect(calls, contains('excluded_folder_add'));
      expect(find.text('/test/photos-excluded'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('photoAlbumRemoveExcluded-0')));
      await tester.pumpAndSettle();
      expect(calls, contains('excluded_folder_remove'));
      expect(find.text('/test/photos-excluded'), findsNothing);
    });
  });
}
