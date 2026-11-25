import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:docfit_ui/features/common/file_selection.dart';
import 'package:docfit_ui/features/doc_grid/doc_grid_screen.dart';
import 'package:docfit_ui/features/photo_album/photo_album_screen.dart';

// FILEOPS1: 다중 선택과 파일 작업. 합성 목록과 가짜 API 만 쓰며 실제 파일은 없다.

const _png =
    'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==';
const _modal = ValueKey('photoViewerModal');
const _names = ['a', 'b', 'c', 'd'];

class _Api {
  final opened = <String>[];
  final starts = <String, List<Map<String, Object?>>>{};
  final calls = <String>[];

  /// 실패시킬 경로.
  final fail = <String>{};
  int runningPolls = 0;
  Map<String, Object?>? _last;

  List<Map<String, Object?>> startsOf(String method) => starts[method] ?? [];

  Future<Object?> call(String method, List<Object?> args) async {
    final action = '${args.first}';
    calls.add('$method.$action');
    final payload = args.length > 1
        ? Map<String, Object?>.from(args[1] as Map)
        : <String, Object?>{};
    List<Map<String, Object?>> photos() => [
      for (final n in _names)
        {'path': '/p/$n.jpg', 'filename': '$n.jpg', 'date_taken': '', 'tags': <String>[]},
    ];
    switch (action) {
      case 'open':
        opened.add('${payload['path']}');
        return {'ok': true};
      case 'list_folders':
        return {'ok': true, 'folders': [{'path': '/p'}]};
      case 'list_by_date':
        return {
          'ok': true,
          'groups': [
            {'label': '앞', 'count': 2, 'photos': photos().sublist(0, 2)},
            {'label': '뒤', 'count': 2, 'photos': photos().sublist(2)},
          ],
        };
      case 'place_groups':
        return {'ok': true, 'groups': [{'label': '폴더', 'photos': photos()}]};
      case 'search':
        return {'ok': true, 'results': photos()};
      case 'find_duplicates':
        return {
          'ok': true,
          'groups': [
            {
              'representative': '/p/a.jpg',
              'items': [for (final p in photos().sublist(0, 2)) {...p, 'distance': 0}],
            },
          ],
        };
      case 'list_default':
        return {
          'ok': true,
          'documents': [
            for (final n in _names)
              {'path': '/d/$n.pdf', 'name': '$n.pdf', 'ext': 'pdf', 'size': 1},
          ],
        };
      case 'thumbnail':
        return {'ok': true, 'data': _png};
      case 'file_op_start':
        starts.putIfAbsent(method, () => []).add(payload);
        _last = payload;
        return {'ok': true, 'task_id': 't', 'state': 'running'};
      case 'file_op_progress':
        if (runningPolls > 0) {
          runningPolls--;
          return {'ok': true, 'state': 'running', 'done': 1, 'total': 2};
        }
        return {'ok': true, 'state': 'complete', 'results': _results(_last!)};
    }
    return {'ok': true, 'tags': <Object>[]};
  }

  List<Map<String, Object?>> _results(Map<String, Object?> payload) {
    final op = '${payload['op']}';
    return [
      for (final path in (payload['paths'] as List).cast<String>())
        if (fail.contains(path))
          {'path': path, 'name': path.split('/').last, 'ok': false, 'message': '열려 있습니다.'}
        else
          {
            'path': path,
            'name': path.split('/').last,
            'ok': true,
            'listed': op != 'delete',
            if (op == 'rename') ...{
              'new_path': '${path.substring(0, path.lastIndexOf('/'))}/${payload['new_name']}.${path.split('.').last}',
              'new_name': '${payload['new_name']}.${path.split('.').last}',
            },
            if (op == 'copy' || op == 'move') ...{
              'new_path': '${payload['destination']}/${path.split('/').last}',
              'new_name': path.split('/').last,
            },
          },
    ];
  }
}

Future<void> _pump(WidgetTester tester, Widget screen) async {
  tester.view.physicalSize = const Size(1200, 800);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  await tester.pumpWidget(MaterialApp(home: Scaffold(body: screen)));
  await tester.pumpAndSettle();
}

/// 더블클릭 판정 시간을 넘겨 다음 클릭이 새 클릭이 되게 한다.
Future<void> _settle(WidgetTester tester) async {
  for (var i = 0; i < 4; i++) {
    await tester.pump(const Duration(milliseconds: 200));
  }
  await tester.pumpAndSettle();
}

Future<void> _click(WidgetTester tester, Finder finder, [LogicalKeyboardKey? modifier]) async {
  if (modifier != null) await tester.sendKeyDownEvent(modifier);
  await tester.tap(finder);
  if (modifier != null) await tester.sendKeyUpEvent(modifier);
  await _settle(tester);
}

Future<void> _ctrl(WidgetTester tester, LogicalKeyboardKey key) async {
  await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
  await tester.sendKeyEvent(key);
  await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
  await _settle(tester);
}

Future<void> _key(WidgetTester tester, LogicalKeyboardKey key) async {
  await tester.sendKeyEvent(key);
  await _settle(tester);
}

Future<void> _press(WidgetTester tester, String key) async {
  await tester.tap(find.byKey(ValueKey(key)));
  await _settle(tester);
}

void _expectCount(int count) => expect(
  (find.byKey(const ValueKey('fileSelectionCount')).evaluate().single.widget as Text).data,
  '$count개 선택',
);

Finder _doc(String n) => find.byKey(ValueKey('docCard-/d/$n.pdf'));
Finder _photo(String n) => find.byKey(ValueKey('photoCard-/p/$n.jpg'));

const _shift = LogicalKeyboardKey.shiftLeft;
const _control = LogicalKeyboardKey.controlLeft;

void main() {
  test('선택 모델: 범위·추가·전체·결과 반영', () {
    const order = ['1', '2', '3', '4', '5'];
    final selection = FileSelection()..click('2', order);
    selection.click('4', order, range: true);
    expect(selection.visible(order), ['2', '3', '4']);
    // 범위는 기준(2)에서 다시 잡는다.
    selection.click('1', order, range: true);
    expect(selection.visible(order), ['1', '2']);
    selection.click('5', order, toggle: true);
    expect(selection.visible(order), ['1', '2', '5']);
    selection.click('4', order, toggle: true, range: true);
    expect(selection.visible(order), ['1', '2', '4', '5']);
    selection.click('5', order, toggle: true);
    expect(selection.visible(order), ['1', '2', '4']);
    // 화면에 없는 항목은 작업 대상이 아니다.
    expect(selection.visible(['4', '9']), ['4']);
    selection.applyResults('rename', [
      {'path': '4', 'ok': true, 'listed': true, 'new_path': '4b'},
    ]);
    selection.applyResults('delete', [
      {'path': '1', 'ok': true, 'listed': false},
      {'path': '2', 'ok': false},
    ]);
    expect(selection.paths, {'2', '4b'});
    selection.selectAll(order);
    expect(selection.paths, order.toSet());
    selection.clear();
    selection.click('3', order, range: true);
    expect(selection.paths, {'3'}, reason: '기준이 없으면 한 개만');
  });

  testWidgets('문서뷰: 클릭·Ctrl·Shift·Ctrl+A·Esc 선택과 개수', (tester) async {
    final api = _Api();
    await _pump(tester, DocGridScreen(api: api.call));
    _expectCount(0);
    await _click(tester, _doc('a'));
    _expectCount(1);
    await _click(tester, _doc('c'), _shift);
    _expectCount(3);
    await _click(tester, _doc('b'), _control);
    _expectCount(2);
    await _click(tester, _doc('d'), _control);
    _expectCount(3);
    await _click(tester, _doc('b'));
    _expectCount(1);
    await _ctrl(tester, LogicalKeyboardKey.keyA);
    _expectCount(4);
    await _key(tester, LogicalKeyboardKey.escape);
    _expectCount(0);
    await _key(tester, LogicalKeyboardKey.delete);
    expect(find.byKey(const ValueKey('fileOpDeleteDialog')), findsNothing);
    expect(api.startsOf('doc_grid'), isEmpty);
  });

  testWidgets('문서뷰: Ctrl 로 두 번 눌러도 열지 않는다', (tester) async {
    final api = _Api();
    await _pump(tester, DocGridScreen(api: api.call));
    await tester.sendKeyDownEvent(_control);
    await tester.tap(_doc('a'));
    await tester.pump(const Duration(milliseconds: 80));
    await tester.tap(_doc('a'));
    await tester.sendKeyUpEvent(_control);
    await _settle(tester);
    expect(api.opened, isEmpty);
  });

  testWidgets('문서뷰: Delete 는 이름을 보여 확인받고, 성공한 것만 목록에서 뺀다', (tester) async {
    final api = _Api()..fail.add('/d/b.pdf');
    await _pump(tester, DocGridScreen(api: api.call));
    await _click(tester, _doc('a'));
    await _click(tester, _doc('c'), _shift);

    await _key(tester, LogicalKeyboardKey.delete);
    final dialog = find.byKey(const ValueKey('fileOpDeleteDialog'));
    expect(find.descendant(of: dialog, matching: find.text('파일 3개를 휴지통으로 보낼까요?')), findsOneWidget);
    for (final name in ['a.pdf', 'b.pdf', 'c.pdf']) {
      expect(find.descendant(of: dialog, matching: find.text(name)), findsOneWidget);
    }
    await _press(tester, 'fileOpDeleteCancel');
    expect(api.startsOf('doc_grid'), isEmpty);

    // 확인창의 처음 초점은 취소다.
    await _key(tester, LogicalKeyboardKey.delete);
    await _key(tester, LogicalKeyboardKey.enter);
    expect(dialog, findsNothing);
    expect(api.startsOf('doc_grid'), isEmpty);

    await _press(tester, 'fileOpDelete');
    await _press(tester, 'fileOpDeleteConfirm');
    expect(api.startsOf('doc_grid').single, {
      'op': 'delete',
      'paths': ['/d/a.pdf', '/d/b.pdf', '/d/c.pdf'],
    });
    expect(find.byKey(const ValueKey('fileOpProblemDialog')), findsOneWidget);
    expect(find.text('성공 2개 · 실패 1개'), findsOneWidget);
    expect(find.text('열려 있습니다.'), findsOneWidget);
    expect(_doc('a'), findsNothing);
    expect(_doc('b'), findsOneWidget);
    expect(_doc('c'), findsNothing);
    expect(_doc('d'), findsOneWidget);
    _expectCount(1);
  });

  testWidgets('문서뷰: F2 이름 변경, Ctrl+C 클립보드, 폴더로 복사', (tester) async {
    final api = _Api()..runningPolls = 1;
    await _pump(tester, DocGridScreen(api: api.call, directoryPicker: () async => '/d/새폴더'));
    await _click(tester, _doc('b'));
    await _key(tester, LogicalKeyboardKey.f2);
    final field = find.byKey(const ValueKey('fileOpRenameField'));
    expect(tester.widget<TextField>(field).controller!.text, 'b');
    expect(tester.widget<TextField>(field).decoration!.suffixText, '.pdf');
    await tester.enterText(field, '고친 이름');
    await _press(tester, 'fileOpRenameConfirm');
    expect(api.startsOf('doc_grid').single, {
      'op': 'rename',
      'paths': ['/d/b.pdf'],
      'new_name': '고친 이름',
    });
    expect(find.byKey(const ValueKey('docCard-/d/고친 이름.pdf')), findsOneWidget);
    expect(_doc('b'), findsNothing);
    _expectCount(1);

    await _click(tester, _doc('a'));
    await _ctrl(tester, LogicalKeyboardKey.keyC);
    expect(api.startsOf('doc_grid').last, {'op': 'clipboard', 'paths': ['/d/a.pdf']});

    await _press(tester, 'fileOpCopy');
    expect(api.startsOf('doc_grid').last, {
      'op': 'copy',
      'paths': ['/d/a.pdf'],
      'destination': '/d/새폴더',
    });
    expect(_doc('a'), findsOneWidget);
    expect(find.byKey(const ValueKey('docCard-/d/새폴더/a.pdf')), findsOneWidget);

    await _click(tester, _doc('c'));
    await _press(tester, 'fileOpMove');
    expect(api.startsOf('doc_grid').last['op'], 'move');
    expect(_doc('c'), findsNothing);
    expect(find.byKey(const ValueKey('docCard-/d/새폴더/c.pdf')), findsOneWidget);
  });

  testWidgets('문서뷰: 대상 폴더를 고르지 않으면 아무것도 하지 않는다', (tester) async {
    final api = _Api();
    await _pump(tester, DocGridScreen(api: api.call, directoryPicker: () async => null));
    await _click(tester, _doc('a'));
    await _press(tester, 'fileOpMove');
    expect(api.startsOf('doc_grid'), isEmpty);
  });

  testWidgets('사진뷰: 한 번 클릭은 선택+크게 보기, Ctrl·Shift 클릭은 선택만', (tester) async {
    final api = _Api();
    await _pump(tester, PhotoAlbumScreen(api: api.call));
    await _click(tester, _photo('b'), _control);
    expect(find.byKey(_modal), findsNothing);
    _expectCount(1);
    // 범위는 묶음을 넘어 화면 순서를 따른다.
    await _click(tester, _photo('d'), _shift);
    expect(find.byKey(_modal), findsNothing);
    _expectCount(3);
    expect(api.opened, isEmpty);

    await _click(tester, _photo('c'));
    expect(find.byKey(_modal), findsOneWidget);
    // 둘째 묶음의 사진도 누른 그 사진이 뜬다.
    expect(find.descendant(of: find.byKey(_modal), matching: find.text('c.jpg')), findsWidgets);
    await _key(tester, LogicalKeyboardKey.escape);
    expect(find.byKey(_modal), findsNothing);
    _expectCount(1);

    await _ctrl(tester, LogicalKeyboardKey.keyA);
    _expectCount(4);
    await _key(tester, LogicalKeyboardKey.escape);
    _expectCount(0);
  });

  testWidgets('사진뷰: 선택 모드는 클릭이 추가·해제이고 Shift 는 범위', (tester) async {
    final api = _Api();
    await _pump(tester, PhotoAlbumScreen(api: api.call));
    await _press(tester, 'photoAlbumToggleSelectionButton');
    await _click(tester, _photo('a'));
    await _click(tester, _photo('c'));
    _expectCount(2);
    await _click(tester, _photo('a'));
    _expectCount(1);
    await _click(tester, _photo('d'), _shift);
    expect(find.byKey(_modal), findsNothing);
    expect(find.text('4장 선택됨'), findsOneWidget);
  });

  testWidgets('사진뷰: 삭제 결과가 묶음 목록에 반영된다', (tester) async {
    final api = _Api();
    await _pump(tester, PhotoAlbumScreen(api: api.call));
    await _click(tester, _photo('a'), _control);
    await _click(tester, _photo('b'), _control);
    await _key(tester, LogicalKeyboardKey.delete);
    await _press(tester, 'fileOpDeleteConfirm');
    expect(api.startsOf('photo_album').single['paths'], ['/p/a.jpg', '/p/b.jpg']);
    expect(_photo('a'), findsNothing);
    expect(find.byKey(const ValueKey('photoGroupHeader-앞')), findsNothing, reason: '빈 묶음');
    expect(find.text('뒤 (2장)'), findsOneWidget);
    _expectCount(0);
  });

  testWidgets('사진뷰: 폴더별 보기에서 옮기면 다시 묶는다', (tester) async {
    final api = _Api();
    await _pump(tester, PhotoAlbumScreen(api: api.call, directoryPicker: () async => '/q'));
    await tester.tap(find.text('폴더별'));
    await _settle(tester);
    await _click(tester, _photo('a'), _control);
    final before = api.calls.where((c) => c == 'photo_album.place_groups').length;
    await _press(tester, 'fileOpMove');
    expect(api.startsOf('photo_album').single['destination'], '/q');
    expect(api.calls.where((c) => c == 'photo_album.place_groups').length, before + 1);
  });

  testWidgets('사진뷰: 검색 결과와 중복 보기에서도 선택·삭제한다', (tester) async {
    final api = _Api();
    await _pump(tester, PhotoAlbumScreen(api: api.call));
    await _click(tester, _photo('a'), _control);
    await tester.enterText(find.byKey(const ValueKey('photoAlbumSearchField')), 'jpg');
    await _press(tester, 'photoAlbumSearchSubmitButton');
    expect(find.byKey(const ValueKey('photoAlbumSearchResultsView')), findsOneWidget);
    _expectCount(0);
    await _click(tester, _photo('b'), _control);
    await _click(tester, _photo('c'), _shift);
    await _press(tester, 'fileOpDelete');
    await _press(tester, 'fileOpDeleteConfirm');
    expect(api.startsOf('photo_album').single['paths'], ['/p/b.jpg', '/p/c.jpg']);
    expect(_photo('b'), findsNothing);
    expect(_photo('a'), findsOneWidget);

    await _press(tester, 'photoAlbumFindDuplicatesButton');
    Finder duplicate(String n) => find.byKey(ValueKey('duplicatePhotoCard-/p/$n.jpg'));
    await _click(tester, duplicate('a'), _control);
    await _click(tester, duplicate('b'), _shift);
    expect(find.byKey(_modal), findsNothing);
    _expectCount(2);
    await _click(tester, duplicate('b'), _control);
    await _press(tester, 'fileOpDelete');
    await _press(tester, 'fileOpDeleteConfirm');
    expect(api.startsOf('photo_album').last['paths'], ['/p/a.jpg']);
    expect(find.byKey(const ValueKey('photoAlbumNoDuplicatesText')), findsOneWidget);
  });
}
