import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:docfit_ui/features/doc_grid/doc_grid_screen.dart';
import 'package:docfit_ui/features/photo_album/photo_album_screen.dart';

void main() {
  for (final photo in [false, true]) {
    testWidgets('${photo ? 'photo' : 'doc'}: 저장 목록·증분 반영과 구버전 안내가 함께 보인다', (tester) async {
      tester.view.physicalSize = const Size(1400, 1000);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      var finished = false;
      var polls = 0;
      var fullLists = 0;
      var restarts = 0;
      final ext = photo ? 'jpg' : 'pdf';
      final first = <String, Object?>{
        'path': '/synthetic/a.$ext', 'name': 'a.$ext', 'filename': 'a.$ext', 'ext': ext,
        'date': '2026-01-01 00:00:00', 'date_taken': '2026-01-01 00:00:00',
        'size': 10, 'cloud': false, 'tags': <String>[], 'favorite': false,
      };
      final second = <String, Object?>{
        ...first, 'path': '/synthetic/b.$ext', 'name': 'b.$ext', 'filename': 'b.$ext',
        'date': '2025-02-01 00:00:00', 'date_taken': '2025-02-01 00:00:00',
      };
      Future<Object?> api(String method, List<Object?> args) async {
        final action = args.first as String;
        if (action == 'scan_default' || action == 'list_default') {
          return {'ok': true, 'task_id': 'synthetic', 'state': 'running', 'from_db': true};
        }
        if (action == 'scan_progress') {
          polls++;
          final since = (args[1] as Map)['since'] as int;
          final all = [first, if (finished) {'op': 'remove', 'path': first['path']}, if (finished) second];
          return {
            'ok': true, 'state': finished ? 'complete' : 'running',
            'rows': all.skip(since).toList(), 'next': all.length, 'total': all.length,
            'service_status': {'status': 'outdated', 'outdated': true},
            if (finished) 'result': {'exif': {'total': 0}},
          };
        }
        if (action == 'list_by_date') fullLists++;
        if (action == 'service_restart') restarts++;
        if (action == 'list_folders') return {'ok': true, 'folders': <Object?>[]};
        return {'ok': true};
      }

      await tester.pumpWidget(MaterialApp(home: Scaffold(
        body: photo ? PhotoAlbumScreen(api: api) : DocGridScreen(api: api),
      )));
      await tester.pump(const Duration(milliseconds: 150));
      await tester.pump();
      final prefix = photo ? 'photoAlbum' : 'docGrid';
      expect(polls, greaterThan(0));
      expect(find.byKey(ValueKey('${prefix}ServiceOutdatedBanner')), findsOneWidget);
      expect(find.textContaining('저장된 목록 표시'), findsOneWidget);
      expect(photo ? find.text('2026년 1월 (1장)') : find.text('a.pdf'), findsOneWidget);

      finished = true;
      await tester.pump(const Duration(milliseconds: 150));
      await tester.pumpAndSettle();
      expect(find.byKey(ValueKey('${prefix}ServiceOutdatedBanner')), findsOneWidget);
      expect(photo ? find.text('2026년 1월 (1장)') : find.text('a.pdf'), findsNothing);
      expect(photo ? find.text('2025년 2월 (1장)') : find.text('b.pdf'), findsOneWidget);
      expect(fullLists, 0);
      expect(restarts, 0);
    });
  }
}
