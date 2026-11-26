import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:docfit_ui/features/doc_grid/doc_grid_screen.dart';
import 'package:docfit_ui/features/photo_album/photo_album_screen.dart';

Map<String, Object?> _doc(String name) => {
  'path': '/test/docs/$name',
  'name': name,
  'ext': 'pdf',
  'cloud': true,
};

Map<String, Object?> _photo(String name, String date) => {
  'path': '/test/photos/$name',
  'name': name,
  'filename': name,
  'date_taken': date,
  'tags': <String>[],
  'favorite': false,
  'cloud': true,
};

void _screen(WidgetTester tester) {
  tester.view.physicalSize = const Size(1200, 800);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
}

void main() {
  testWidgets('문서뷰: 검색이 끝나기 전에 받은 행을 보여 주고 이어 붙인 뒤 한 번 정렬한다', (tester) async {
    _screen(tester);
    var finished = false;
    final sinces = <Object?>[];
    Future<Object?> api(String method, List<Object?> args) async {
      final action = args.first as String;
      if (action == 'list_default') return {'ok': true, 'task_id': 't', 'state': 'running'};
      if (action == 'scan_progress') {
        final since = (args[1] as Map)['since'] as int;
        sinces.add(since);
        final all = [_doc('나.pdf'), _doc('다.pdf'), if (finished) _doc('가.pdf')];
        // 한 번에 한 행씩만 줘도 남은 행을 이어서 받는다.
        final rows = all.skip(since).take(1).toList();
        return {
          'ok': true,
          'state': finished ? 'complete' : 'running',
          'visited': all.length,
          'rows': rows,
          'next': since + rows.length,
          'total': all.length,
          if (finished) 'result': {'engine': 'klib'},
        };
      }
      return {'ok': true};
    }

    await tester.pumpWidget(MaterialApp(home: DocGridScreen(api: api)));
    await tester.pump(const Duration(milliseconds: 140));
    await tester.pump();
    expect(find.byKey(const ValueKey('docGridView')), findsOneWidget);
    expect(find.byKey(const ValueKey('docGridScanBanner')), findsOneWidget);
    expect(find.text('2개 문서'), findsOneWidget);

    // 검색 중에도 선택할 수 있다.
    await tester.tap(find.byKey(const ValueKey('docCardFrame-/test/docs/다.pdf')));
    await tester.pump(const Duration(milliseconds: 400));

    finished = true;
    await tester.pump(const Duration(milliseconds: 140));
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('docGridScanBanner')), findsNothing);
    expect(find.text('3개 문서'), findsOneWidget);
    expect(
      tester.getTopLeft(find.byKey(const ValueKey('docCardFrame-/test/docs/가.pdf'))).dx,
      lessThan(tester.getTopLeft(find.byKey(const ValueKey('docCardFrame-/test/docs/나.pdf'))).dx),
    );
    final selected = tester.widget<Container>(
      find.byKey(const ValueKey('docCardFrame-/test/docs/다.pdf')),
    );
    expect(((selected.decoration! as BoxDecoration).border! as Border).top.width, 2.5);
    // 받은 행은 다시 요청하지 않는다.
    expect(sinces.toSet().containsAll([0, 1, 2]), isTrue);
    expect(sinces, orderedEquals([...sinces]..sort((a, b) => (a! as int).compareTo(b! as int))));
  });

  testWidgets('문서뷰: 검색 중 취소해도 받은 목록은 남는다', (tester) async {
    _screen(tester);
    var cancelled = false;
    Future<Object?> api(String method, List<Object?> args) async {
      final action = args.first as String;
      if (action == 'list_default') return {'ok': true, 'task_id': 't', 'state': 'running'};
      if (action == 'scan_cancel') {
        cancelled = true;
        return {'ok': true};
      }
      if (action == 'scan_progress') {
        final since = (args[1] as Map)['since'] as int;
        return {
          'ok': true,
          'state': cancelled ? 'cancelled' : 'running',
          'visited': 1,
          'rows': since == 0 ? [_doc('하나.pdf')] : <Object?>[],
          'next': 1,
          'total': 1,
          if (cancelled) 'result': {'engine': 'klib'},
        };
      }
      return {'ok': true};
    }

    await tester.pumpWidget(MaterialApp(home: DocGridScreen(api: api)));
    await tester.pump(const Duration(milliseconds: 140));
    await tester.pump();
    await tester.tap(find.byKey(const ValueKey('docGridCancelScanButton')));
    await tester.pump(const Duration(milliseconds: 140));
    await tester.pumpAndSettle();
    expect(find.text('1개 문서'), findsOneWidget);
    expect(find.byKey(const ValueKey('docGridScanBanner')), findsNothing);
  });

  testWidgets('문서뷰: 저장 목록을 먼저 보여 주고 바뀐 것만 반영한다', (tester) async {
    _screen(tester);
    var finished = false;
    Future<Object?> api(String method, List<Object?> args) async {
      final action = args.first as String;
      if (action == 'list_default') return {'ok': true, 'task_id': 't', 'state': 'running', 'from_db': true};
      if (action == 'scan_progress') {
        final since = (args[1] as Map)['since'] as int;
        final all = <Map<String, Object?>>[
          _doc('나.pdf'),
          _doc('다.pdf'),
          if (finished) ...[
            {..._doc('나.pdf'), 'size': 2048, 'op': 'update'},
            _doc('가.pdf'),
            {'op': 'remove', 'path': '/test/docs/다.pdf'},
          ],
        ];
        final rows = all.skip(since).toList();
        return {
          'ok': true,
          'state': finished ? 'complete' : 'running',
          'visited': all.length,
          'rows': rows,
          'next': all.length,
          'total': all.length,
          if (finished) 'result': {'engine': 'service'},
        };
      }
      return {'ok': true};
    }

    await tester.pumpWidget(MaterialApp(home: DocGridScreen(api: api)));
    await tester.pump(const Duration(milliseconds: 140));
    await tester.pump();
    expect(find.text('2개 문서'), findsOneWidget);
    expect(find.textContaining('저장된 목록 표시'), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('docCardFrame-/test/docs/다.pdf')));
    await tester.pump(const Duration(milliseconds: 400));

    finished = true;
    await tester.pump(const Duration(milliseconds: 140));
    await tester.pumpAndSettle();
    expect(find.text('2개 문서'), findsOneWidget);
    expect(find.byKey(const ValueKey('docCardFrame-/test/docs/다.pdf')), findsNothing);
    expect(find.text('2.0 KB'), findsOneWidget);
    expect(
      tester.getTopLeft(find.byKey(const ValueKey('docCardFrame-/test/docs/가.pdf'))).dx,
      lessThan(tester.getTopLeft(find.byKey(const ValueKey('docCardFrame-/test/docs/나.pdf'))).dx),
    );
  });

  testWidgets('사진뷰: 검색 중 받은 사진을 월별로 보여 주고 끝나면 DB 목록으로 바꾼다', (tester) async {
    _screen(tester);
    var finished = false;
    var listed = 0;
    Future<Object?> api(String method, List<Object?> args) async {
      final action = args.first as String;
      if (action == 'scan_default') return {'ok': true, 'task_id': 't', 'state': 'running'};
      if (action == 'scan_progress') {
        final since = (args[1] as Map)['since'] as int;
        final all = [_photo('a.jpg', '2025-03-02 10:00:00'), _photo('b.jpg', '2026-01-05 09:00:00')];
        final rows = all.skip(since).toList();
        return {
          'ok': true,
          'state': finished ? 'complete' : 'running',
          'visited': 2,
          'rows': rows,
          'next': since + rows.length,
          'total': all.length,
          if (finished) 'result': {'scanned': 2},
        };
      }
      if (action == 'list_by_date') {
        listed++;
        return {
          'ok': true,
          'groups': [
            {'year': 2024, 'month': 7, 'items': [_photo('a.jpg', '2024-07-01 00:00:00')]},
          ],
        };
      }
      if (action == 'list_folders') return {'ok': true, 'folders': <Object?>[]};
      return {'ok': true};
    }

    await tester.pumpWidget(MaterialApp(home: Scaffold(body: PhotoAlbumScreen(api: api))));
    await tester.pump(const Duration(milliseconds: 140));
    await tester.pump();
    expect(listed, 0);
    expect(find.byKey(const ValueKey('photoAlbumScanBanner')), findsOneWidget);
    expect(find.text('2026년 1월 (1장)'), findsOneWidget);
    expect(find.text('2025년 3월 (1장)'), findsOneWidget);
    expect(
      tester.getTopLeft(find.text('2026년 1월 (1장)')).dy,
      lessThan(tester.getTopLeft(find.text('2025년 3월 (1장)')).dy),
    );

    finished = true;
    await tester.pump(const Duration(milliseconds: 140));
    await tester.pumpAndSettle();
    expect(listed, 1);
    expect(find.byKey(const ValueKey('photoAlbumScanBanner')), findsNothing);
    expect(find.text('2024년 7월 (1장)'), findsOneWidget);
    expect(find.text('2026년 1월 (1장)'), findsNothing);
  });
}
