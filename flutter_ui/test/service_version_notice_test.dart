import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:docfit_ui/features/doc_grid/doc_grid_screen.dart';
import 'package:docfit_ui/features/photo_album/photo_album_screen.dart';

void main() {
  for (final photo in [false, true]) {
    final prefix = photo ? 'photoAlbum' : 'docGrid';
    testWidgets('$prefix: 이전 버전 안내와 수동 방법, 취소·실패·재확인', (tester) async {
      tester.view.physicalSize = const Size(1200, 900);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      var outdated = true;
      var succeed = false;
      var restarts = 0;
      final calls = <String>[];
      Map<String, Object?> status() => {
        'status': outdated ? 'outdated' : 'ok',
        'outdated': outdated,
        'message': '색인 서비스가 이전 버전입니다.',
      };
      Future<Object?> api(String method, List<Object?> args) async {
        final action = args.first as String;
        calls.add(action);
        if (action == 'service_restart') {
          restarts++;
          expect((args[1] as Map)['confirmed'], true);
          if (!succeed) return {'ok': false, 'message': '재시작 실패'};
          outdated = false;
          return {'ok': true};
        }
        if (action == 'service_status_get') return {'ok': true, 'service_status': status()};
        if (action == 'list_default' || action == 'scan_default') {
          return {'ok': true, 'documents': <Object?>[], 'service_status': status()};
        }
        if (action == 'list_folders') return {'ok': true, 'folders': <Object?>[]};
        if (action == 'list_by_date') return {'ok': true, 'groups': <Object?>[]};
        return {'ok': true};
      }
      await tester.pumpWidget(MaterialApp(home: photo ? PhotoAlbumScreen(api: api) : DocGridScreen(api: api)));
      await tester.pumpAndSettle();
      final banner = find.byKey(ValueKey('${prefix}ServiceOutdatedBanner'));
      final button = find.byKey(ValueKey('${prefix}RestartServiceButton'));
      expect(banner, findsOneWidget);
      expect(restarts, 0);
      await tester.tap(button);
      await tester.pumpAndSettle();
      expect(find.textContaining('services.msc'), findsOneWidget);
      await tester.tap(find.text('취소'));
      await tester.pumpAndSettle();
      expect(restarts, 0);
      expect(banner, findsOneWidget);
      await tester.tap(button);
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(ValueKey('${prefix}ConfirmRestartButton')));
      await tester.pumpAndSettle();
      expect(restarts, 1);
      expect(banner, findsOneWidget);
      expect(find.text('재시작 실패'), findsOneWidget);
      succeed = true;
      await tester.tap(button);
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(ValueKey('${prefix}ConfirmRestartButton')));
      await tester.pumpAndSettle();
      expect(restarts, 2);
      expect(calls, contains('service_status_get'));
      expect(banner, findsNothing);
    });

    testWidgets('$prefix: 검색 중 구버전 감지 후 화면을 떠나도 안전하다', (tester) async {
      tester.view.physicalSize = const Size(1200, 900);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      var polled = false;
      Future<Object?> api(String method, List<Object?> args) async {
        final action = args.first as String;
        if (action == 'list_default' || action == 'scan_default') return {'ok': true, 'task_id': 't', 'state': 'running'};
        if (action == 'scan_progress') {
          polled = true;
          return {'ok': true, 'state': 'complete', 'rows': <Object?>[], 'next': 0, 'total': 0,
                  'result': {'documents': <Object?>[]},
                  'service_status': {'status': 'outdated', 'outdated': true}};
        }
        return {'ok': true};
      }
      await tester.pumpWidget(MaterialApp(home: photo ? PhotoAlbumScreen(api: api) : DocGridScreen(api: api)));
      await tester.pump(const Duration(milliseconds: 150));
      await tester.pumpAndSettle();
      expect(polled, true);
      expect(find.byKey(ValueKey('${prefix}ServiceOutdatedBanner')), findsOneWidget);
      await tester.pumpWidget(const MaterialApp(home: SizedBox()));
      await tester.pump(const Duration(seconds: 1));
      expect(tester.takeException(), isNull);
    });
  }
}
