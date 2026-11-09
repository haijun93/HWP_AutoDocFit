import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:docfit_ui/features/file_rename/file_rename_screen.dart';

void main() {
  final renameActions = <String>[];

  Future<dynamic> fakeApi(String method, [dynamic args]) async {
    if (method == 'file_rename') {
      final sub = args is List && args.isNotEmpty ? args[0] : (args is Map ? args['action'] : '');
      renameActions.add('$sub');
      if (sub == 'pick_files' || sub == 'list') {
        return {
          'ok': true,
          'added': sub == 'pick_files' ? 1 : 0,
          'entries': [
            {'id': 'e1', 'name': '보고서.txt', 'folder': r'C:\test', 'size': 1536, 'mtime': 1791507900.0},
          ],
        };
      }
      if (sub == 'preview') {
        return {
          'ok': true,
          'plan_id': 'plan1',
          'items': [
            {'id': 'e1', 'new_name': '보고서_완료.txt', 'status': 'ok'},
          ],
          'summary': {'ok': 1, 'conflict': 0, 'invalid': 0, 'executables': 0},
        };
      }
      if (sub == 'apply' || sub == 'undo') {
        return {
          'ok': true,
          'run_id': 'run1',
          'done': 1,
          'failed': [],
          'entries': [
            {
              'id': 'e1',
              'name': sub == 'apply' ? '보고서_완료.txt' : '보고서.txt',
              'folder': r'C:\test',
              'size': 1536,
              'mtime': 1791507900.0,
            },
          ],
        };
      }
      if (sub == 'snapshot') {
        return {
          'entries': [
            {'id': 'e1', 'name': 'test1.txt', 'path': r'C:\test\test1.txt', 'dir': r'C:\test'},
          ],
          'rules': [],
          'preview': {
            'e1': {'status': 'ok', 'new_name': 'test1.txt', 'changed': false},
          },
          'summary': {'total': 1, 'changed': 0, 'conflicts': 0, 'empty': 0},
        };
      }
      if (sub == 'mru') return {'list': []};
      return {'ok': true};
    }
    return {'ok': true};
  }

  testWidgets('backend_preview_row_shows_names_status_size_and_modified_time', (tester) async {
    renameActions.clear();
    tester.view.physicalSize = const Size(1100, 800);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);

    await tester.pumpWidget(MaterialApp(home: Scaffold(body: FileRenameScreen(api: fakeApi))));
    await tester.tap(find.byKey(const ValueKey('rename-add-files')));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 200));
    await tester.pumpAndSettle();

    expect(find.text('원래 이름'), findsOneWidget);
    expect(find.text('새 이름 (바뀐 부분 강조 · 두 번 눌러 직접 고치기)'), findsOneWidget);
    expect(find.text('상태'), findsOneWidget);
    expect(find.text('크기'), findsOneWidget);
    expect(find.text('수정 시각'), findsOneWidget);
    expect(find.text('보고서.txt'), findsOneWidget);
    expect(find.textContaining('보고서_완료.txt'), findsOneWidget);
    expect(find.text('바뀜'), findsOneWidget);
    expect(find.text('1.5 KB'), findsOneWidget);
    // 수정 시각은 실행 PC의 시간대로 표시되므로(CI는 UTC) 같은 방식으로 기대값을 만든다.
    final t = DateTime.fromMillisecondsSinceEpoch(1791507900000);
    String two(int v) => v.toString().padLeft(2, '0');
    expect(find.text('${t.month}/${t.day} ${two(t.hour)}:${two(t.minute)}'), findsOneWidget);
    expect(renameActions, contains('preview'));

    await tester.tap(find.byKey(const ValueKey('rename-apply')));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('rename-confirm')));
    await tester.pumpAndSettle();
    expect(renameActions, contains('apply'));
    expect(find.byKey(const ValueKey('rename-undo')), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('rename-undo')));
    await tester.pumpAndSettle();
    expect(renameActions, contains('undo'));
    expect(tester.takeException(), isNull);
  });

  testWidgets('splitter_drag_updates_layout_and_resize_preserves_ratio', (tester) async {
    tester.view.physicalSize = const Size(1000, 800);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: FileRenameScreen(api: fakeApi),
        ),
      ),
    );
    await tester.pumpAndSettle();

    // 720px 이상에서는 좌우 분할 핸들 (rename-split-side) 확인
    final splitSideFinder = find.byKey(const ValueKey('rename-split-side'));
    expect(splitSideFinder, findsOneWidget);

    final startCenter = tester.getCenter(splitSideFinder);

    // 드래그 수행 (좌측으로 50px 이동)
    await tester.drag(splitSideFinder, const Offset(-50, 0));
    await tester.pumpAndSettle();

    final draggedCenter = tester.getCenter(splitSideFinder);
    expect(draggedCenter.dx, isNot(equals(startCenter.dx)));

    // 과도한 드래그 시 최소 폭(280px) 제한(clamp) 확인
    await tester.drag(splitSideFinder, const Offset(600, 0));
    await tester.pumpAndSettle();

    final clampedCenter = tester.getCenter(splitSideFinder);
    expect(clampedCenter.dx, greaterThanOrEqualTo(280.0));
  });

  testWidgets('narrow_layout_uses_vertical_splitter_and_clamps_height', (tester) async {
    tester.view.physicalSize = const Size(600, 800);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: FileRenameScreen(api: fakeApi),
        ),
      ),
    );
    await tester.pumpAndSettle();

    // 720px 미만에서는 상하 분할 핸들 (rename-split-list) 확인
    final splitListFinder = find.byKey(const ValueKey('rename-split-list'));
    expect(splitListFinder, findsOneWidget);

    final startCenter = tester.getCenter(splitListFinder);

    // 상하 드래그 수행
    await tester.drag(splitListFinder, const Offset(0, 40));
    await tester.pumpAndSettle();

    final draggedCenter = tester.getCenter(splitListFinder);
    expect(draggedCenter.dy, isNot(equals(startCenter.dy)));

    // 과도한 상단 드래그 시 최소 높이(120px) 제한(clamp) 확인
    await tester.drag(splitListFinder, const Offset(0, -500));
    await tester.pumpAndSettle();

    final clampedCenter = tester.getCenter(splitListFinder);
    expect(clampedCenter.dy, greaterThanOrEqualTo(120.0));
    expect(tester.takeException(), isNull);
  });

  testWidgets('empty_state_scrolls_without_overflow_at_minimum_list_height', (tester) async {
    // 좁고 낮은 뷰포트(500x350)에서 최소 목록 영역(120px) 빈 상태 안내가 overflow 없이 스크롤되는지 검증
    tester.view.physicalSize = const Size(500, 350);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: FileRenameScreen(api: fakeApi),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(tester.takeException(), isNull);
    expect(find.textContaining('여기에 파일이나 폴더를 끌어다 놓거나'), findsOneWidget);

    // 스크롤 제스처 수행
    final emptyScrollFinder = find.byType(SingleChildScrollView).first;
    await tester.drag(emptyScrollFinder, const Offset(0, -50));
    await tester.pumpAndSettle();

    expect(tester.takeException(), isNull);
  });
}
