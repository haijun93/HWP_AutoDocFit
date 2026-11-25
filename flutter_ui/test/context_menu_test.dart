import 'package:flutter/gestures.dart' show kSecondaryButton;
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:docfit_ui/features/common/file_handoff.dart';
import 'package:docfit_ui/features/photo_album/photo_album_screen.dart';
import 'package:docfit_ui/features/workspace/sidebar_menu_settings.dart';
import 'package:docfit_ui/features/workspace/tab_navigation_shell.dart';
import 'package:docfit_ui/main.dart';

import 'workspace_test.dart' show initialState;

// CTXMENU1: 문서뷰·사진뷰 오른쪽 클릭 메뉴. 앱 전체(DocFitApp)를 가짜 API 로 띄우고
// 메뉴를 누른 뒤 대상 화면에 파일이 실제로 올라왔는지 본다. 실제 파일은 없다.

const _png =
    'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==';

const _docs = [
  r'C:\d\alpha.pdf',
  r'C:\d\bravo.pdf',
  r'C:\d\charlie.hwpx',
  r'C:\d\delta.xlsx',
];
const _photos = [r'C:\p\one.jpg', r'C:\p\two.jpg', r'C:\p\raw.heic'];

String _name(String path) => path.split(r'\').last;

class _Api {
  final state = initialState();
  final calls = <String>[];
  final added = <List<String>>[];
  final pdfInfo = <String>[];
  final opened = <String>[];
  final fileOps = <Map<String, Object?>>[];
  final boxes = <Map<String, Object?>>[
    {'id': 1, 'name': '기존 폴더'},
  ];
  final items = <Map<String, Object?>>[];
  final notebooks = <Map<String, Object?>>[
    {'id': 7, 'name': '기존 자료함'},
  ];
  final sources = <Map<String, Object?>>[];
  String conversionMode = 'flutter';

  /// add_paths 가 아무것도 넣지 않는 경우(형식 거절 등).
  bool rejectPaths = false;

  Future<Object?> call(String method, List<Object?> args) async {
    switch (method) {
      case 'get_state':
        return Map<String, dynamic>.from(state);
      case 'set_mode':
        state['mode'] = args.single;
        calls.add('set_mode:${args.single}');
        return Map<String, dynamic>.from(state);
      case 'add_paths':
        final paths = (args.single as List).cast<String>();
        added.add(paths);
        if (!rejectPaths) {
          state['files'] = [
            ...(state['files'] as List),
            for (final p in paths) {'path': p, 'name': _name(p), 'folder': r'C:\d'},
          ];
        }
        return Map<String, dynamic>.from(state);
      case 'conversion_window':
        return {'ok': true, 'data': {'mode': conversionMode}};
      case 'docview_window':
      case 'photoview_window':
        return {'ok': true, 'data': {'mode': 'flutter'}};
      case 'pdf_edit':
        final payload = args.single as Map;
        if (payload['action'] == 'info') {
          pdfInfo.add('${payload['source']}');
          return {'ok': true, 'data': {'pages': [1, 2]}};
        }
        return {'ok': true, 'data': {'data': _png}};
      case 'library':
        return _library(Map<String, Object?>.from(args.single as Map));
      case 'notebook':
        return _notebook(Map<String, Object?>.from(args.single as Map));
      case 'doc_grid':
      case 'photo_album':
        return _view(method, args);
      default:
        return {'ok': true};
    }
  }

  Object _library(Map<String, Object?> r) {
    calls.add('library.${r['cmd']}');
    switch (r['cmd']) {
      case 'box_list':
        return {'ok': true, 'boxes': boxes};
      case 'box_create':
        boxes.add({'id': boxes.length + 1, 'name': r['name']});
        return {'ok': true, 'id': boxes.length};
      case 'item_search':
        return {
          'ok': true,
          'items': [for (final i in items) if (i['box_id'] == r['box_id']) i],
        };
      case 'relation_list':
        return {'ok': true, 'relations': <Object>[]};
      case 'item_add':
        items.add({...r, 'id': items.length + 1});
        return {'ok': true, 'id': items.length};
      case 'item_get':
        return {'ok': true, 'item': items[(r['id'] as int) - 1]};
      case 'lib_take_drop':
        return {'ok': true, 'paths': <String>[]};
      default:
        return {'ok': true};
    }
  }

  Object _notebook(Map<String, Object?> r) {
    calls.add('notebook.${r['cmd']}');
    switch (r['cmd']) {
      case 'nb_list':
        return {'ok': true, 'notebooks': notebooks};
      case 'nb_source_list':
        return {
          'ok': true,
          'sources': [for (final s in sources) if (s['notebook_id'] == r['notebook_id']) s],
        };
      case 'nb_note_list':
        return {'ok': true, 'notes': <Object>[]};
      case 'nb_source_add':
        sources.add({
          'id': sources.length + 1,
          'notebook_id': r['notebook_id'],
          'display_name': _name('${r['source_path']}'),
          'kind': 'pdf',
          'status': 'ok',
          'paragraph_count': 1,
          'origin': r['source_path'],
        });
        return {'ok': true, 'id': sources.length};
      case 'nb_take_drop':
        return {'ok': true, 'paths': <String>[]};
      default:
        return {'ok': true};
    }
  }

  Object _view(String method, List<Object?> args) {
    final payload = args.length > 1
        ? Map<String, Object?>.from(args[1] as Map)
        : <String, Object?>{};
    switch (args.first) {
      case 'list_default':
        return {
          'ok': true,
          'documents': [
            for (final p in _docs)
              {'path': p, 'name': _name(p), 'ext': p.split('.').last, 'size': 1},
          ],
        };
      case 'list_folders':
        return {'ok': true, 'folders': [{'path': r'C:\p'}]};
      case 'list_by_date':
        return {
          'ok': true,
          'groups': [
            {
              'label': '묶음',
              'photos': [
                for (final p in _photos)
                  {'path': p, 'filename': _name(p), 'date_taken': '', 'tags': <String>[]},
              ],
            },
          ],
        };
      case 'thumbnail':
        return {'ok': true, 'data': _png};
      case 'open':
        opened.add('${payload['path']}');
        return {'ok': true};
      case 'file_op_start':
        fileOps.add(payload);
        return {'ok': true, 'task_id': 't'};
      case 'file_op_progress':
        return {
          'ok': true,
          'state': 'complete',
          'results': [
            for (final p in (fileOps.last['paths'] as List))
              {'path': p, 'name': _name('$p'), 'ok': true, 'listed': false},
          ],
        };
      default:
        return {'ok': true};
    }
  }
}

Future<void> _settle(WidgetTester tester) async {
  for (var i = 0; i < 12; i++) {
    await tester.pump(const Duration(milliseconds: 200));
  }
}

Future<_Api> _start(WidgetTester tester, int tab, [_Api? existing]) async {
  tester.view.physicalSize = const Size(1600, 1200);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  final api = existing ?? _Api();
  await tester.pumpWidget(DocFitApp(api: api.call));
  await _settle(tester);
  await tester.tap(find.byKey(Key('workspaceTab-$tab')));
  await _settle(tester);
  addTearDown(() async => tester.pumpWidget(const SizedBox()));
  return api;
}

Finder _doc(String path) => find.byKey(ValueKey('docCard-$path'));
Finder _photo(String path) => find.byKey(ValueKey('photoCard-$path'));
Finder _menu(String id) => find.byKey(ValueKey('fileMenu-$id'));

Future<void> _rightClick(WidgetTester tester, Finder target) async {
  await tester.tap(target, buttons: kSecondaryButton);
  await tester.pumpAndSettle();
}

Future<void> _ctrlClick(WidgetTester tester, Finder target) async {
  await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
  await tester.tap(target);
  await tester.pump();
  await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
  await tester.pump(const Duration(milliseconds: 500));
}

Future<void> _choose(WidgetTester tester, String id) async {
  await tester.tap(_menu(id));
  await _settle(tester);
}

bool _enabled(WidgetTester tester, String id) =>
    tester.widget<PopupMenuItem<String>>(_menu(id)).enabled;

String _count(WidgetTester tester) => tester
    .widget<Text>(find.byKey(const ValueKey('fileSelectionCount')))
    .data!;

Future<void> _closeMenu(WidgetTester tester) async {
  await tester.sendKeyEvent(LogicalKeyboardKey.escape);
  await tester.pumpAndSettle();
}

void main() {
  testWidgets('문서뷰: 선택 밖 오른쪽 클릭은 그 파일만, 선택 안은 선택 전체가 대상', (tester) async {
    final api = await _start(tester, kTabDocGrid);
    await _ctrlClick(tester, _doc(_docs[0]));
    await _ctrlClick(tester, _doc(_docs[1]));
    expect(_count(tester), '2개 선택');

    await _rightClick(tester, _doc(_docs[1]));
    expect(_count(tester), '2개 선택');
    for (final id in ['open', 'rename']) {
      expect(_enabled(tester, id), isFalse, reason: id);
    }
    expect(find.text('한 개만 선택했을 때 쓸 수 있습니다'), findsNWidgets(2));
    await _choose(tester, 'delete');
    await tester.tap(find.byKey(const ValueKey('fileOpDeleteConfirm')));
    await _settle(tester);
    expect(api.fileOps.single['op'], 'delete');
    expect(api.fileOps.single['paths'], [_docs[0], _docs[1]]);

    await _ctrlClick(tester, _doc(_docs[2]));
    await _rightClick(tester, _doc(_docs[3]));
    expect(_count(tester), '1개 선택');
    await _choose(tester, 'open');
    expect(api.opened, [_docs[3]]);
  });

  testWidgets('문서뷰: 공통 작업(복사·이름 변경)과 형식에 맞지 않는 메뉴 비활성', (tester) async {
    final api = await _start(tester, kTabDocGrid);
    await _rightClick(tester, _doc(_docs[3])); // xlsx
    for (final id in ['open', 'clipboard', 'copy', 'move', 'rename', 'delete', 'virtualFolder', 'notebook']) {
      expect(_enabled(tester, id), isTrue, reason: id);
    }
    for (final id in ['documentEditor', 'conversion', 'pdfEdit']) {
      expect(_enabled(tester, id), isFalse, reason: id);
    }
    expect(find.text('지원하지 않는 형식이 있습니다(.xlsx)'), findsNWidgets(2));
    expect(find.text('PDF 파일만 편집할 수 있습니다'), findsOneWidget);
    await _choose(tester, 'clipboard');
    expect(api.fileOps.single['op'], 'clipboard');

    await _rightClick(tester, _doc(_docs[2]));
    await _choose(tester, 'rename');
    expect(find.byKey(const ValueKey('fileOpRenameDialog')), findsOneWidget);
  });

  testWidgets('문서뷰 → 문서편집: 문서 선택 목록에 파일이 올라온다', (tester) async {
    final api = await _start(tester, kTabDocGrid);
    await _ctrlClick(tester, _doc(_docs[0]));
    await _ctrlClick(tester, _doc(_docs[2]));
    await _rightClick(tester, _doc(_docs[2]));
    await _choose(tester, 'documentEditor');
    expect(api.added.single, [_docs[0], _docs[2]]);
    expect(find.text('선택한 문서 2개'), findsOneWidget);
    expect(find.text('alpha.pdf').hitTestable(), findsOneWidget);
    expect(find.text('charlie.hwpx').hitTestable(), findsOneWidget);
  });

  testWidgets('문서뷰 → 파일 변환: 변환 화면의 파일 목록에 올라온다', (tester) async {
    final api = await _start(tester, kTabDocGrid);
    await _rightClick(tester, _doc(_docs[2]));
    await _choose(tester, 'conversion');
    expect(api.added.single, [_docs[2]]);
    expect(api.calls, contains('set_mode:file_format'));
    expect(find.byKey(const Key('conversionWorkspace')).hitTestable(), findsOneWidget);
    expect(find.byKey(const Key('conversionRemove-0')).hitTestable(), findsOneWidget);
    expect(find.text('charlie.hwpx').hitTestable(), findsOneWidget);
  });

  testWidgets('파일 변환이 Win32 창 방식이면 넣지 않고 알린다', (tester) async {
    final api = await _start(tester, kTabDocGrid, _Api()..conversionMode = 'native');
    await _rightClick(tester, _doc(_docs[2]));
    await _choose(tester, 'conversion');
    expect(api.added, isEmpty);
    expect(find.textContaining('새 창(Win32) 방식'), findsOneWidget);
    expect(_doc(_docs[2]).hitTestable(), findsOneWidget);
  });

  testWidgets('목록에 넣지 못하면 화면을 옮기지 않고 알린다', (tester) async {
    await _start(tester, kTabDocGrid, _Api()..rejectPaths = true);
    await _rightClick(tester, _doc(_docs[0]));
    await _choose(tester, 'documentEditor');
    expect(find.textContaining('파일을 목록에 넣지 못했습니다'), findsOneWidget);
    expect(_doc(_docs[0]).hitTestable(), findsOneWidget);
  });

  testWidgets('숨긴 메뉴의 화면은 비활성이고 이유를 보인다', (tester) async {
    addTearDown(() => hiddenTabsNotifier.value = {});
    await _start(tester, kTabDocGrid);
    hiddenTabsNotifier.value = {kTabPdfEdit};
    await _settle(tester);
    await _rightClick(tester, _doc(_docs[0]));
    expect(_enabled(tester, 'pdfEdit'), isFalse);
    expect(find.text('메뉴에서 숨긴 화면입니다(설정 > 쓸 메뉴 고르기)'), findsOneWidget);
    expect(_enabled(tester, 'conversion'), isTrue);
    await _closeMenu(tester);
  });

  testWidgets('문서뷰 → PDF 편집: 한 개는 열고 여러 개는 합칠 목록', (tester) async {
    final api = await _start(tester, kTabDocGrid);
    await _rightClick(tester, _doc(_docs[0]));
    await _choose(tester, 'pdfEdit');
    expect(api.pdfInfo, [_docs[0]]);
    expect(find.textContaining('alpha.pdf').hitTestable(), findsWidgets);

    await tester.tap(find.byKey(Key('workspaceTab-$kTabDocGrid')));
    await _settle(tester);
    await _ctrlClick(tester, _doc(_docs[1])); // alpha 는 선택돼 있다.
    await _rightClick(tester, _doc(_docs[1]));
    await _choose(tester, 'pdfEdit');
    expect(api.pdfInfo, [_docs[0], _docs[0]]);
    for (final name in ['alpha.pdf', 'bravo.pdf']) {
      expect(find.textContaining(name).hitTestable(), findsWidgets, reason: name);
    }
  });

  testWidgets('PDF 편집 중이면 버릴지 묻고, 계속 편집을 고르면 바꾸지 않는다', (tester) async {
    final api = await _start(tester, kTabDocGrid);
    await _rightClick(tester, _doc(_docs[0]));
    await _choose(tester, 'pdfEdit');
    await tester.tap(find.text('쪽 1').first);
    await _settle(tester);
    await tester.tap(find.byTooltip('쪽 삭제').first);
    await _settle(tester);

    Future<void> sendBravo() async {
      await tester.tap(find.byKey(Key('workspaceTab-$kTabDocGrid')));
      await _settle(tester);
      await _rightClick(tester, _doc(_docs[1]));
      await _choose(tester, 'pdfEdit');
      expect(find.text('저장하지 않은 PDF 편집'), findsOneWidget);
    }

    await sendBravo();
    await tester.tap(find.text('계속 편집'));
    await _settle(tester);
    expect(api.pdfInfo, [_docs[0]]);
    await sendBravo();
    await tester.tap(find.text('버리고 열기'));
    await _settle(tester);
    expect(api.pdfInfo, [_docs[0], _docs[1]]);
  });

  testWidgets('문서뷰 → 가상 폴더: 고른 폴더에 항목이 올라온다', (tester) async {
    final api = await _start(tester, kTabDocGrid);
    await _rightClick(tester, _doc(_docs[0]));
    await _choose(tester, 'virtualFolder');
    expect(find.byKey(const ValueKey('boxPickDialog')), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('boxPick-0')));
    await _settle(tester);
    expect(api.items.single['source_path'], _docs[0]);
    expect(api.items.single['box_id'], 1);
    expect(find.text('alpha.pdf').hitTestable(), findsWidgets);
    expect(find.textContaining('파일 1개를 가상 폴더에 넣었습니다.'), findsOneWidget);

    // 다시 열 때 같은 파일이 또 들어가지 않는다.
    await tester.tap(find.byKey(Key('workspaceTab-$kTabDocGrid')));
    await _settle(tester);
    await tester.tap(find.byKey(const ValueKey('library-open')));
    await _settle(tester);
    expect(find.byKey(const ValueKey('boxPickDialog')), findsNothing);
    expect(api.items, hasLength(1));
  });

  testWidgets('가상 폴더: 새로 만들어 넣기와 취소', (tester) async {
    final api = await _start(tester, kTabDocGrid);
    await _rightClick(tester, _doc(_docs[3]));
    await _choose(tester, 'virtualFolder');
    await tester.tap(find.byKey(const ValueKey('boxPickCancel')));
    await _settle(tester);
    expect(api.items, isEmpty);

    await tester.tap(find.byKey(Key('workspaceTab-$kTabDocGrid')));
    await _settle(tester);
    await _rightClick(tester, _doc(_docs[3]));
    await _choose(tester, 'virtualFolder');
    await tester.enterText(find.byKey(const ValueKey('boxPickNewName')), '새 묶음');
    await tester.tap(find.byKey(const ValueKey('boxPickCreate')));
    await _settle(tester);
    expect(api.boxes.last['name'], '새 묶음');
    expect(api.items.single['box_id'], 2);
    expect(find.text('delta.xlsx').hitTestable(), findsWidgets);
  });

  testWidgets('문서뷰 → 업무자료함: 고른 자료함의 출처에 올라온다', (tester) async {
    final api = await _start(tester, kTabDocGrid);
    await _rightClick(tester, _doc(_docs[0]));
    await _choose(tester, 'notebook');
    await tester.tap(find.byKey(const ValueKey('boxPick-0')));
    await _settle(tester);
    expect(api.sources.single['origin'], _docs[0]);
    expect(api.sources.single['notebook_id'], 7);
    expect(find.textContaining('alpha.pdf').hitTestable(), findsWidgets);
    expect(find.textContaining('출처를 추가했습니다.'), findsOneWidget);
  });

  testWidgets('사진뷰: 메뉴 구성, 사진 편집 창, 크게 보기 예약 취소', (tester) async {
    await _start(tester, kTabPhotoAlbum);
    // 왼쪽 클릭 직후 오른쪽 클릭해도 크게 보기가 뒤늦게 열리지 않는다.
    await tester.tap(_photo(_photos[0]));
    await tester.pump(const Duration(milliseconds: 50));
    await _rightClick(tester, _photo(_photos[0]));
    await tester.pump(const Duration(seconds: 1));
    expect(find.byKey(const ValueKey('photoViewerModal')), findsNothing);
    for (final id in ['open', 'clipboard', 'copy', 'move', 'rename', 'delete', 'photoEdit', 'conversion', 'virtualFolder', 'photoExport', 'photoSheet']) {
      expect(_enabled(tester, id), isTrue, reason: id);
    }
    for (final id in ['documentEditor', 'pdfEdit', 'notebook']) {
      expect(_menu(id), findsNothing, reason: id);
    }
    await _choose(tester, 'photoEdit');
    final dialog = find.widgetWithText(AlertDialog, '사본 편집');
    expect(dialog, findsOneWidget);
    expect(find.descendant(of: dialog, matching: find.textContaining('one.jpg')), findsWidgets);
  });

  testWidgets('사진뷰: 여러 장·지원하지 않는 형식은 비활성, 사진대지는 선택 전체', (tester) async {
    await _start(tester, kTabPhotoAlbum);
    await _ctrlClick(tester, _photo(_photos[0]));
    await _ctrlClick(tester, _photo(_photos[2]));
    await _rightClick(tester, _photo(_photos[2]));
    expect(_enabled(tester, 'photoEdit'), isFalse);
    expect(_enabled(tester, 'conversion'), isFalse);
    expect(find.text('지원하지 않는 형식이 있습니다(.heic)'), findsOneWidget);
    await _choose(tester, 'photoSheet');
    expect(find.textContaining('2장'), findsWidgets);
  });

  testWidgets('사진뷰 → 파일 변환·가상 폴더', (tester) async {
    final api = await _start(tester, kTabPhotoAlbum);
    await _rightClick(tester, _photo(_photos[1]));
    await _choose(tester, 'conversion');
    expect(api.added.single, [_photos[1]]);
    expect(find.byKey(const Key('conversionRemove-0')).hitTestable(), findsOneWidget);
    expect(find.text('two.jpg').hitTestable(), findsOneWidget);

    await tester.tap(find.byKey(Key('workspaceTab-$kTabPhotoAlbum')));
    await _settle(tester);
    await _rightClick(tester, _photo(_photos[0]));
    await _choose(tester, 'virtualFolder');
    await tester.tap(find.byKey(const ValueKey('boxPick-0')));
    await _settle(tester);
    expect(api.items.single['source_path'], _photos[0]);
    expect(find.text('one.jpg').hitTestable(), findsWidgets);
  });

  testWidgets('화면만 따로 띄우면 연결 메뉴는 없다', (tester) async {
    final api = _Api();
    await tester.pumpWidget(MaterialApp(home: PhotoAlbumScreen(api: api.call)));
    await _settle(tester);
    await _rightClick(tester, _photo(_photos[0]));
    expect(_menu('delete'), findsOneWidget);
    expect(_menu('photoEdit'), findsOneWidget);
    expect(_menu('conversion'), findsNothing);
    await _closeMenu(tester);
  });

  testWidgets('사진뷰: 스크롤·다시 그리기 뒤에도 누른 사진이 크게 열린다', (tester) async {
    // 순번이 아니라 경로로 여는지 확인(DBLOPEN1 발견, FILEOPS1 수정).
    tester.view.physicalSize = const Size(900, 700);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    final names = [for (var i = 0; i < 60; i++) 'n${i.toString().padLeft(2, '0')}'];
    Future<Object?> api(String method, List<Object?> args) async => switch (args.first) {
      'list_by_date' => {
        'ok': true,
        'groups': [
          for (final half in [0, 30])
            {
              'label': '묶음 $half',
              'photos': [
                for (final n in names.skip(half).take(30))
                  {'path': '/p/$n.jpg', 'filename': '$n.jpg', 'tags': <String>[]},
              ],
            },
        ],
      },
      'thumbnail' => {'ok': true, 'data': _png},
      _ => {'ok': true},
    };
    await tester.pumpWidget(MaterialApp(home: PhotoAlbumScreen(api: api)));
    await _settle(tester);
    final target = find.byKey(const ValueKey('photoCard-/p/n45.jpg'));
    final scrollable = find
        .descendant(
          of: find.byKey(const ValueKey('photoAlbumGroupedGridView')),
          matching: find.byType(Scrollable),
        )
        .first;
    await tester.scrollUntilVisible(target, 300, scrollable: scrollable);
    await tester.drag(scrollable, const Offset(0, 120));
    await _settle(tester);
    await tester.drag(scrollable, const Offset(0, -120));
    await _settle(tester);
    await tester.ensureVisible(target);
    await _settle(tester);
    await tester.tap(target);
    await _settle(tester);
    final modal = find.byKey(const ValueKey('photoViewerModal'));
    expect(modal, findsOneWidget);
    expect(find.descendant(of: modal, matching: find.textContaining('n45.jpg')), findsWidgets);
  });

  test('대상별 형식·개수 이유', () {
    expect(fileTargetReason(FileTarget.pdfEdit, ['a.PDF']), isNull);
    expect(fileTargetReason(FileTarget.conversion, ['a.doc']), contains('.doc'));
    expect(fileTargetReason(FileTarget.virtualFolder, List.filled(201, 'a.txt')), contains('200개'));
    expect(fileTargetReason(FileTarget.documentEditor, ['README']), contains('확장자 없음'));
  });
}
