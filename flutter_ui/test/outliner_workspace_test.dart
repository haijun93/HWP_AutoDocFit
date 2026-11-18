import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:docfit_ui/features/outliner/outliner_workspace.dart';

class MemoryRecentPagesStore implements RecentPagesStore {
  MemoryRecentPagesStore([this.pages = const []]);
  List<String> pages;

  @override
  Future<List<String>> load() async => pages;

  @override
  Future<void> save(List<String> value) async => pages = value;
}

class DamagedRecentPagesStore implements RecentPagesStore {
  @override
  Future<List<String>> load() async => throw const FormatException('broken');

  @override
  Future<void> save(List<String> pages) async {}
}

void main() {
  test('file recent page store reads and writes only its supplied directory', () async {
    final directory = await Directory.systemTemp.createTemp('outliner-store-');
    addTearDown(() => directory.delete(recursive: true));
    final store = FileRecentPagesStore(() async => directory);
    await store.save(['Home']);
    expect(await store.load(), ['Home']);
  });

  testWidgets('renders page navigation, daily journal and backlinks panel', (
    tester,
  ) async {
    Future<Object?> api(String method, [List<Object?> args = const []]) async {
      if (method == 'outliner') {
        final payload = args.first as Map;
        switch (payload['action']) {
          case 'list':
            return {
              'ok': true,
              'data': {
                'pages': [
                  {'name': 'Home'},
                ],
              },
            };
          case 'daily':
          case 'load':
            return {
              'ok': true,
              'data': {
                'page': payload['action'] == 'daily'
                    ? '2026-11-18'
                    : payload['page'],
                'markdown': '- Hello\n',
                'version': 'v1',
                'document': {
                  'blocks': [
                    {'id': 1, 'text': 'Hello', 'indent': 0, 'folded': false},
                  ],
                },
              },
            };
          case 'backlinks':
            return {
              'ok': true,
              'data': {'block_ids': []},
            };
        }
      }
      return {'ok': true, 'data': {}};
    }

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(body: OutlinerWorkspace(api: api, recentPagesStore: MemoryRecentPagesStore())),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('오늘의 일지'), findsOneWidget);
    expect(find.text('Home'), findsOneWidget);
    expect(find.text('역링크'), findsOneWidget);
    expect(find.text('Hello'), findsOneWidget);
  });

  testWidgets('shows page management, recent pages and task toggles', (
    tester,
  ) async {
    Future<Object?> api(String method, [List<Object?> args = const []]) async {
      final payload = args.first as Map;
      switch (payload['action']) {
        case 'list':
          return {
            'ok': true,
            'data': {
              'pages': [
                {'name': 'Home'},
              ],
              'recent': ['Home'],
            },
          };
        case 'daily':
        case 'load':
          return {
            'ok': true,
            'data': {
              'page': payload['action'] == 'daily'
                  ? '2026-11-18'
                  : payload['page'],
              'markdown': '- TODO task\n',
              'version': 'v1',
              'document': {
                'blocks': [
                  {
                    'id': 1,
                    'text': 'TODO task',
                    'indent': 0,
                    'status': 'TODO',
                    'folded': false,
                  },
                ],
              },
            },
          };
        case 'backlinks':
          return {
            'ok': true,
            'data': {'block_ids': []},
          };
        default:
          return {
            'ok': true,
            'data': {
              'markdown': '- TODO task\n',
              'document': {
                'blocks': [
                  {
                    'id': 1,
                    'text': 'TODO task',
                    'indent': 0,
                    'status': 'TODO',
                    'folded': false,
                  },
                ],
              },
            },
          };
      }
    }

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(body: OutlinerWorkspace(api: api, recentPagesStore: MemoryRecentPagesStore())),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('최근 페이지'), findsOneWidget);
    expect(find.text('Home'), findsNWidgets(2));
    expect(find.byTooltip('새 페이지'), findsOneWidget);
    expect(find.byTooltip('이름 바꾸기'), findsOneWidget);
    expect(find.byTooltip('페이지 삭제'), findsOneWidget);
    expect(find.byTooltip('완료 표시'), findsOneWidget);
  });

  testWidgets('persists recent pages, ignores damaged and missing entries', (tester) async {
    final store = MemoryRecentPagesStore(['Home', 'Missing', 'Home']);
    Future<Object?> api(String method, [List<Object?> args = const []]) async {
      final p = args.first as Map;
      if (p['action'] == 'list') return {'ok': true, 'data': {'pages': [{'name': 'Home'}]}};
      if (p['action'] == 'daily' || p['action'] == 'load') return {'ok': true, 'data': {'page': p['action'] == 'daily' ? 'Today' : p['page'], 'markdown': '', 'version': '1', 'document': {'blocks': []}}};
      if (p['action'] == 'backlinks') return {'ok': true, 'data': {'block_ids': []}};
      return {'ok': true, 'data': {}};
    }
    await tester.pumpWidget(MaterialApp(home: Scaffold(body: OutlinerWorkspace(api: api, recentPagesStore: store))));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));
    expect(find.text('Today'), findsOneWidget);
    expect(store.pages, ['Today', 'Home']);
    await tester.pumpWidget(const SizedBox());
    await tester.pumpWidget(MaterialApp(home: Scaffold(body: OutlinerWorkspace(api: api, recentPagesStore: DamagedRecentPagesStore()))));
    await tester.pumpAndSettle();
    expect(find.text('Today'), findsOneWidget);
  });

  testWidgets('keyboard shortcuts edit outline blocks', (tester) async {
    final operations = <String>[];
    var blocks = [
      {'id': 1, 'text': 'First', 'indent': 0, 'folded': false},
      {'id': 2, 'text': 'Second', 'indent': 0, 'folded': false},
    ];
    Future<Object?> api(String method, [List<Object?> args = const []]) async {
      final p = args.first as Map;
      if (p['action'] == 'list') return {'ok': true, 'data': {'pages': [{'name': 'Home'}]}};
      if (p['action'] == 'daily' || p['action'] == 'load') return {'ok': true, 'data': {'page': 'Home', 'markdown': '', 'version': '1', 'document': {'blocks': blocks}}};
      if (p['action'] == 'backlinks') return {'ok': true, 'data': {'block_ids': []}};
      if (p['action'] == 'edit') {
        operations.add('${p['operation']}:${p['block_id']}');
        if (p['operation'] == 'insert') blocks = [...blocks, {'id': 3, 'text': '', 'indent': 0, 'folded': false}];
        if (p['operation'] == 'delete') blocks = blocks.where((b) => b['id'] != p['block_id']).toList();
        if (p['operation'] == 'set_text') {
          for (final block in blocks) {
            if (block['id'] == p['block_id']) block['text'] = p['text'];
          }
        }
        if (p['operation'] == 'indent') blocks = [blocks[0], {...blocks[1], 'indent': 1}];
        if (p['operation'] == 'outdent') blocks = [blocks[0], {...blocks[1], 'indent': 0}];
        if (p['operation'] == 'fold' || p['operation'] == 'unfold') {
          blocks = blocks.map((b) => b['id'] == p['block_id']
              ? {...b, 'folded': p['operation'] == 'fold'}
              : b).toList();
        }
        return {'ok': true, 'data': {'markdown': '', 'version': '1', 'document': {'blocks': blocks}}};
      }
      if (p['action'] == 'save') return {'ok': true, 'data': {'version': '2'}};
      return {'ok': true, 'data': {}};
    }
    await tester.pumpWidget(MaterialApp(home: Scaffold(body: OutlinerWorkspace(api: api, recentPagesStore: MemoryRecentPagesStore()))));
    await tester.pumpAndSettle();
    final first = find.byKey(const ValueKey('outliner-block-1'));
    final second = find.byKey(const ValueKey('outliner-block-2'));
    await tester.tap(first); await tester.pump();
    await tester.enterText(first, 'First edited');
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pump(const Duration(milliseconds: 100));
    expect(operations, contains('insert:1'));
    await tester.tap(second); await tester.pump();
    await tester.sendKeyEvent(LogicalKeyboardKey.tab); await tester.pump(const Duration(milliseconds: 100));
    expect(operations, contains('indent:2'));
    await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.tab);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft); await tester.pump(const Duration(milliseconds: 100));
    expect(operations, contains('outdent:2'));
    await tester.sendKeyDownEvent(LogicalKeyboardKey.altLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowUp);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.altLeft); await tester.pump(const Duration(milliseconds: 100));
    expect(operations, contains('up:2'));
    await tester.sendKeyDownEvent(LogicalKeyboardKey.altLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.altLeft); await tester.pump(const Duration(milliseconds: 100));
    expect(operations, contains('down:2'));
    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft); await tester.pump(const Duration(milliseconds: 100));
    expect(operations, contains('fold:2'));
    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft); await tester.pump(const Duration(milliseconds: 100));
    expect(operations, contains('unfold:2'));
    await tester.tap(second); await tester.enterText(second, 'Second');
    await tester.sendKeyEvent(LogicalKeyboardKey.home);
    await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
    await tester.pump(const Duration(milliseconds: 100));
    expect(operations, contains('set_text:1'));
    expect(operations, contains('delete:2'));
    await tester.tap(first); await tester.enterText(first, '');
    await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
    await tester.pump(const Duration(milliseconds: 100));
    expect(operations, contains('delete:1'));
  });
}
