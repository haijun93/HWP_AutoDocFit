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
  testWidgets('Alt arrows select blocks and Alt Shift moves selection in one edit', (tester) async {
    final requests = <Map>[];
    const blocks = [
      {'id': 1, 'text': 'one', 'indent': 0, 'folded': false},
      {'id': 2, 'text': 'two', 'indent': 0, 'folded': false},
      {'id': 3, 'text': 'three', 'indent': 0, 'folded': false},
    ];
    Future<Object?> api(String method, [List<Object?> args = const []]) async {
      final p = Map<String, Object?>.from(args.first as Map);
      requests.add(p);
      if (p['action'] == 'list') return {'ok': true, 'data': {'pages': []}};
      if (p['action'] == 'daily') return {'ok': true, 'data': {'page': 'Today', 'markdown': '- one\n- two\n- three\n', 'version': 'v1', 'document': {'blocks': blocks}}};
      if (p['action'] == 'backlinks') return {'ok': true, 'data': {'block_ids': []}};
      if (p['action'] == 'key') return {'ok': true, 'data': {'handled': true, 'markdown': '- three\n- one\n- two\n', 'document': {'blocks': [blocks[2], blocks[0], blocks[1]]}, 'selection': [1, 2]}};
      if (p['action'] == 'save') return {'ok': true, 'data': {'version': 'v2'}};
      return {'ok': true, 'data': {}};
    }
    await tester.pumpWidget(MaterialApp(home: Scaffold(body: OutlinerWorkspace(api: api, recentPagesStore: MemoryRecentPagesStore()))));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('outliner-block-1')));
    await tester.pump();
    await tester.sendKeyDownEvent(LogicalKeyboardKey.altLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await tester.pumpAndSettle();
    // Logseq 처럼 첫 Alt+↓ 는 지금 블록만, 다음부터 늘어난다.
    expect(find.text('1개 선택'), findsOneWidget);
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.altLeft);
    await tester.pumpAndSettle();
    expect(find.text('2개 선택'), findsOneWidget);
    await tester.sendKeyDownEvent(LogicalKeyboardKey.altLeft);
    await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.altLeft);
    await tester.pumpAndSettle();
    final edit = requests.lastWhere((p) => p['action'] == 'key' && p['operation'] != 'commit');
    expect(edit['operation'], 'move_down');
    expect(edit['selection'], [0, 1]);
  });

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

  testWidgets('synthetic Korean input is committed before Enter creates the next block', (tester) async {
    final requests = <Map>[];
    List<Map<String, Object?>> blocks = [
      {'id': 1, 'text': '첫 항목', 'indent': 0, 'folded': false},
    ];
    Future<Object?> api(String method, [List<Object?> args = const []]) async {
      final payload = Map<String, Object?>.from(args.first as Map);
      if (payload['action'] == 'list') {
        return {'ok': true, 'data': {'pages': [{'name': '합성'}]}};
      }
      if (payload['action'] == 'daily' || payload['action'] == 'load') {
        return {'ok': true, 'data': {
          'page': '합성',
          'markdown': '- 첫 항목\n',
          'version': '1',
          'document': {'blocks': blocks},
        }};
      }
      if (payload['action'] == 'backlinks') {
        return {'ok': true, 'data': {'block_ids': []}};
      }
      if (payload['action'] == 'key') {
        requests.add(payload);
        blocks = [
          {...blocks[0], 'text': payload['text']},
          {'id': 2, 'text': '', 'indent': 0, 'folded': false},
        ];
        return {'ok': true, 'data': {
          'handled': true,
          'markdown': blocks.map((block) => '- ${block['text']}\n').join(),
          'document': {'blocks': blocks},
          'selection': [],
          'cursor': {'block': 1, 'start': 0, 'end': 0},
        }};
      }
      if (payload['action'] == 'save') return {'ok': true, 'data': {'version': '2'}};
      return {'ok': true, 'data': {}};
    }

    await tester.pumpWidget(MaterialApp(
      home: Scaffold(body: OutlinerWorkspace(
        api: api,
        recentPagesStore: MemoryRecentPagesStore(),
      )),
    ));
    await tester.pumpAndSettle();
    final first = find.byKey(const ValueKey('outliner-block-1'));
    await tester.tap(first);
    await tester.enterText(first, '한글 입력 시험');
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pump(const Duration(milliseconds: 100));
    await tester.pump(const Duration(milliseconds: 100));

    expect(requests.map((request) => request['operation']), ['enter']);
    expect(requests.first['text'], '한글 입력 시험');
    expect(find.byKey(const ValueKey('outliner-block-2')), findsOneWidget);
    final nextField = tester.widget<EditableText>(find.descendant(
      of: find.byKey(const ValueKey('outliner-block-2')),
      matching: find.byType(EditableText),
    ));
    expect(nextField.focusNode.hasFocus, isTrue);
  });
}
