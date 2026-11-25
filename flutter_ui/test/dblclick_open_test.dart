import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:docfit_ui/features/doc_grid/doc_grid_screen.dart';
import 'package:docfit_ui/features/photo_album/photo_album_screen.dart';

// DBLOPEN1: 더블클릭 → 연결 앱으로 열기. 합성 목록과 가짜 API 만 쓴다.

const _png =
    'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==';

const _photos = [
  {'path': '/photos/a.jpg', 'filename': 'a.jpg', 'date_taken': '', 'tags': ['표']},
  {'path': '/photos/b.jpg', 'filename': 'b.jpg', 'date_taken': '', 'tags': ['표']},
];

const _card = ValueKey('photoCard-/photos/b.jpg');
const _modal = ValueKey('photoViewerModal');

class _Api {
  final opened = <String>[];
  final actions = <String>[];
  Object? openResponse = const {'ok': true};

  Future<Object?> call(String method, List<Object?> args) async {
    final action = '${args.first}';
    actions.add('$method.$action');
    final payload = args.length > 1 ? args[1] as Map : const {};
    switch (action) {
      case 'open':
        opened.add('$method:${payload['path']}');
        return openResponse;
      case 'list_folders':
        return {
          'ok': true,
          'folders': [
            {'path': '/photos'},
          ],
        };
      case 'list_by_date':
        return {
          'ok': true,
          'groups': [
            {'label': '2026년 11월', 'photos': _photos},
          ],
        };
      case 'list_tags':
        return {
          'ok': true,
          'tags': [
            {'tag': '표', 'count': 2},
          ],
        };
      case 'search':
        return {'ok': true, 'results': _photos};
      case 'find_duplicates':
        return {
          'ok': true,
          'groups': [
            {
              'representative': '/photos/a.jpg',
              'items': [
                {..._photos[0], 'distance': 0},
                {..._photos[1], 'distance': 1},
              ],
            },
          ],
        };
      case 'list_default':
        return {
          'ok': true,
          'documents': [
            for (final name in ['가.hwpx', '나.pdf'])
              {'path': '/docs/$name', 'name': name, 'ext': name.split('.').last, 'size': 1},
          ],
        };
      case 'thumbnail':
        return {'ok': true, 'data': _png};
    }
    return {'ok': true};
  }
}

Future<void> _pump(WidgetTester tester, Widget screen) async {
  tester.view.physicalSize = const Size(1200, 800);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  await tester.pumpWidget(MaterialApp(home: Scaffold(body: screen)));
  await tester.pumpAndSettle();
}

Future<void> _doubleTap(WidgetTester tester, Finder finder) async {
  await tester.tap(finder);
  await tester.pump(const Duration(milliseconds: 80));
  await tester.tap(finder);
  await tester.pump(const Duration(milliseconds: 400));
  await tester.pumpAndSettle();
}

Future<void> _singleTap(WidgetTester tester, Finder finder) async {
  await tester.tap(finder);
  await tester.pump(const Duration(milliseconds: 400));
  await tester.pumpAndSettle();
}

/// 묶음 격자 / 검색 결과 / 중복 보기에서 같은 규칙을 확인한다.
Future<void> _checkPhotoList(WidgetTester tester, _Api api, Finder card) async {
  await _doubleTap(tester, card);
  expect(api.opened, ['photo_album:/photos/b.jpg']);
  expect(find.byKey(_modal), findsNothing, reason: '더블클릭에 크게 보기가 함께 뜨면 안 된다');

  await _singleTap(tester, card);
  expect(find.byKey(_modal), findsOneWidget);
  expect(api.opened, hasLength(1));
}

void main() {
  testWidgets('사진뷰 묶음 격자: 더블클릭은 열기만, 한 번 클릭은 크게 보기', (tester) async {
    final api = _Api();
    await _pump(tester, PhotoAlbumScreen(api: api.call));
    await _checkPhotoList(tester, api, find.byKey(_card));
  });

  testWidgets('사진뷰 검색 결과: 더블클릭으로 연다', (tester) async {
    final api = _Api();
    await _pump(tester, PhotoAlbumScreen(api: api.call));
    await tester.tap(find.byKey(const ValueKey('photoTagFilter-표')));
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('photoAlbumSearchResultsView')), findsOneWidget);
    await _checkPhotoList(tester, api, find.byKey(_card));
  });

  testWidgets('사진뷰 중복 보기: 더블클릭으로 연다', (tester) async {
    final api = _Api();
    await _pump(tester, PhotoAlbumScreen(api: api.call));
    await tester.tap(find.text('중복 사진 찾기'));
    await tester.pumpAndSettle();
    await _checkPhotoList(
      tester,
      api,
      find.byKey(const ValueKey('duplicatePhotoCard-/photos/b.jpg')),
    );
  });

  testWidgets('크게 보기 창의 사진 더블클릭은 지금 보이는 사진을 연다', (tester) async {
    final api = _Api();
    await _pump(tester, PhotoAlbumScreen(api: api.call));
    await _singleTap(tester, find.byKey(_card));
    await tester.tap(find.byKey(const ValueKey('photoViewerPrevButton')));
    await tester.pumpAndSettle();
    await _doubleTap(tester, find.byKey(const ValueKey('photoViewerImageArea')));
    expect(api.opened, ['photo_album:/photos/a.jpg']);
    expect(find.byKey(_modal), findsOneWidget);
  });

  testWidgets('별표 단추는 더블클릭 판정을 기다리지 않고 파일을 열지 않는다', (tester) async {
    final api = _Api();
    await _pump(tester, PhotoAlbumScreen(api: api.call));
    final star = find.byKey(const ValueKey('photoCardFavorite-/photos/b.jpg'));
    await tester.tap(star);
    await tester.pump();
    expect(api.actions, contains('photo_album.toggle_favorite'));
    await tester.pumpAndSettle();
    await _doubleTap(tester, star);
    expect(api.opened, isEmpty);
    expect(find.byKey(_modal), findsNothing);
  });

  testWidgets('선택 모드의 연속 클릭은 선택 전환이며 열지 않는다', (tester) async {
    final api = _Api();
    await _pump(tester, PhotoAlbumScreen(api: api.call));
    await tester.tap(find.byKey(const ValueKey('photoAlbumToggleSelectionButton')));
    await tester.pumpAndSettle();
    await _doubleTap(tester, find.byKey(_card));
    expect(api.opened, isEmpty);
    expect(find.byKey(_modal), findsNothing);
  });

  testWidgets('사진 열기 실패 메시지를 보여 준다', (tester) async {
    final api = _Api()
      ..openResponse = const {
        'ok': false,
        'code': 'FILE_NOT_FOUND',
        'message': '파일을 찾을 수 없습니다: b.jpg',
      };
    await _pump(tester, PhotoAlbumScreen(api: api.call));
    await _doubleTap(tester, find.byKey(_card));
    expect(find.text('파일을 찾을 수 없습니다: b.jpg'), findsOneWidget);
  });

  testWidgets('문서뷰: 누르면 선택 표시, 더블클릭하면 연다', (tester) async {
    final api = _Api();
    await _pump(tester, DocGridScreen(api: api.call));

    double borderWidth(String name) {
      final frame = tester.widget<Container>(
        find.byKey(ValueKey('docCardFrame-/docs/$name')),
      );
      return ((frame.decoration! as BoxDecoration).border! as Border).top.width;
    }

    final first = find.byKey(const ValueKey('docCard-/docs/가.hwpx'));
    final second = find.byKey(const ValueKey('docCard-/docs/나.pdf'));
    expect(borderWidth('가.hwpx'), 1.0);

    await _singleTap(tester, first);
    expect(borderWidth('가.hwpx'), 2.5);
    expect(api.opened, isEmpty);

    await _doubleTap(tester, second);
    expect(borderWidth('가.hwpx'), 1.0);
    expect(borderWidth('나.pdf'), 2.5);
    expect(api.opened, ['doc_grid:/docs/나.pdf']);

    // 마우스 오른쪽 단추는 선택을 바꾸지 않는다.
    await tester.tap(first, buttons: kSecondaryButton);
    await tester.pumpAndSettle();
    expect(borderWidth('가.hwpx'), 1.0);
  });
}
