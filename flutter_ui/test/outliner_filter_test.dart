import 'dart:async';
import 'dart:ui';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:docfit_ui/features/outliner/outliner_workspace.dart';

// OUTFILTER1: 블록 검색으로 걸러진 상태의 편집이 화면 순번이 아니라 문서의 그 블록에 적용되는지.

class _Recent implements RecentPagesStore {
  @override
  Future<List<String>> load() async => [];
  @override
  Future<void> save(List<String> pages) async {}
}

typedef _Block = ({int indent, String text});

/// 문서 순번으로만 동작하는 가짜 엔진. 블록 번호는 읽을 때마다 1.. 이다.
/// 마크다운 한 줄 = `들여쓰기 2칸씩- 글`(글 안의 줄바꿈은 `\n` 두 글자).
class _Engine {
  _Engine(List<String> lines, this.folded)
    : markdown = lines
          .map((l) => '${l.substring(0, l.length - l.trimLeft().length)}- ${l.trimLeft()}\n')
          .join();
  String markdown;

  /// 처음 읽을 때 접혀 있는 문서 순번.
  final Set<int> folded;
  final List<Map<String, Object?>> keys = [];
  final List<Map<String, Object?>> edits = [];
  Completer<void>? hold;

  static List<_Block> _parse(String markdown) => [
    for (final line in markdown.split('\n'))
      if (line.isNotEmpty)
        (
          indent: (line.length - line.trimLeft().length) ~/ 2,
          text: line.trimLeft().substring(2).replaceAll(r'\n', '\n'),
        ),
  ];
  static String _write(List<_Block> blocks) => blocks
      .map((b) => '${'  ' * b.indent}- ${b.text.replaceAll('\n', r'\n')}\n')
      .join();
  List<String> get lines => [
    for (final b in _parse(markdown)) '${'  ' * b.indent}${b.text}',
  ];

  Map<String, Object?> _document(List<_Block> blocks, Set<int> folds) => {
    'blocks': [
      for (final (i, b) in blocks.indexed)
        {
          'id': i + 1,
          'text': b.text,
          'indent': b.indent,
          'folded': folds.contains(i),
        },
    ],
  };

  Future<Object?> call(String method, [List<Object?> args = const []]) async {
    final p = Map<String, Object?>.from(args.first as Map);
    Map<String, Object?> data = {};
    switch (p['action']) {
      case 'list':
        data = {'pages': [{'name': '합성'}]};
      case 'daily':
      case 'load':
        data = {
          'page': '합성',
          'markdown': markdown,
          'version': 'v0',
          'document': _document(_parse(markdown), folded),
        };
      case 'backlinks':
        data = {'block_ids': []};
      case 'search':
        data = {
          'block_ids': [
            for (final (i, b) in _parse(p['markdown'] as String).indexed)
              if (b.text.contains(p['query'] as String)) i + 1,
          ],
        };
      case 'save':
        markdown = p['markdown'] as String;
        data = {'version': 'v1'};
      case 'edit':
        edits.add(p);
        final blocks = _parse(p['markdown'] as String);
        final i = (p['block_id'] as int) - 1;
        if (p['operation'] == 'insert') {
          blocks.insert(i + 1, (indent: blocks[i].indent, text: '${p['text']}'));
        } else if (p['operation'] == 'set_text') {
          blocks[i] = (indent: blocks[i].indent, text: '${p['text']}');
        } // toggle_status 는 실제 core 처럼 글을 바꾸지 않는다(상태 칸만 바뀐다).
        data = {'markdown': _write(blocks), 'document': _document(blocks, {})};
      case 'key':
        keys.add(p);
        await hold?.future;
        data = _key(p);
    }
    return {'ok': true, 'data': data};
  }

  Map<String, Object?> _key(Map<String, Object?> p) {
    final blocks = _parse(p['markdown'] as String);
    final selection = (p['selection'] as List).cast<int>();
    final folds = (p['folded'] as List).cast<int>().toSet();
    final at = p['block'] as int?;
    final operation = p['operation'];
    var start = p['start'] as int;
    if (at != null) {
      blocks[at] = (indent: blocks[at].indent, text: p['text'] as String);
    }
    final targets = selection.isNotEmpty ? selection : [?at];
    var cursor = at;
    _Block shifted(int i, int by) =>
        (indent: blocks[i].indent + by, text: blocks[i].text);
    switch (operation) {
      case 'enter':
        final text = blocks[at!].text;
        blocks[at] = (indent: blocks[at].indent, text: text.substring(0, start));
        blocks.insert(at + 1, (
          indent: blocks[at].indent,
          text: text.substring(start),
        ));
        cursor = at + 1;
        start = 0;
      case 'newline':
        final text = blocks[at!].text;
        blocks[at] = (
          indent: blocks[at].indent,
          text: text.replaceRange(start, start, '\n'),
        );
        start++;
      case 'indent':
        for (final i in targets) {
          blocks[i] = shifted(i, 1);
        }
      case 'outdent':
        for (final i in targets) {
          if (blocks[i].indent > 0) blocks[i] = shifted(i, -1);
        }
      case 'cycle_todo':
        for (final i in targets) {
          blocks[i] = (indent: blocks[i].indent, text: 'TODO ${blocks[i].text}');
        }
      case 'delete_selection' || 'cut':
        for (final i in [...targets]..sort((a, b) => b - a)) {
          blocks.removeAt(i);
        }
        cursor = null;
      case 'backspace':
        if (at! > 0) {
          cursor = at - 1;
          start = blocks[cursor].text.length;
          blocks[cursor] = (
            indent: blocks[cursor].indent,
            text: blocks[cursor].text + blocks.removeAt(at).text,
          );
        }
      case 'delete':
        if (at! + 1 < blocks.length) {
          blocks[at] = (
            indent: blocks[at].indent,
            text: blocks[at].text + blocks.removeAt(at + 1).text,
          );
        }
    }
    return {
      'handled': true,
      'markdown': _write(blocks),
      'document': _document(blocks, folds),
      'selection': operation == 'delete_selection' || operation == 'cut'
          ? const <int>[]
          : selection,
      'cursor': cursor == null
          ? null
          : {'block': cursor, 'start': start, 'end': start},
      if (operation == 'copy' || operation == 'cut') 'clipboard': '- 합성',
    };
  }
}

/// 'apple' 로 거르면 1·3·6번(문서 순번 0·2·5)만 보인다. 3·6번은 글이 같고 3번은 하위가 있다.
const _doc = [
  'apple one',
  'banana',
  'apple two',
  '  child',
  'cherry',
  'apple two',
];

Future<_Engine> _pump(
  WidgetTester tester, {
  List<String> lines = _doc,
  Set<int> folded = const {},
  String? search = 'apple',
}) async {
  tester.view.physicalSize = const Size(1400, 900);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
    SystemChannels.platform,
    (call) async => null,
  );
  addTearDown(
    () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.platform,
      null,
    ),
  );
  final engine = _Engine(lines, folded);
  await tester.pumpWidget(
    MaterialApp(
      home: Scaffold(
        body: OutlinerWorkspace(api: engine.call, recentPagesStore: _Recent()),
      ),
    ),
  );
  await tester.pumpAndSettle();
  if (search != null) await _search(tester, search);
  return engine;
}

final _searchBox = find.widgetWithText(TextField, '페이지·블록 검색');

Future<void> _search(WidgetTester tester, String query) async {
  await tester.tap(_searchBox);
  await tester.enterText(_searchBox, query);
  await tester.testTextInput.receiveAction(TextInputAction.done);
  await tester.pumpAndSettle();
}

Finder _field(int id) => find.byKey(ValueKey('outliner-block-$id'));

/// 화면에 보이는 블록 번호.
List<int> _shown(WidgetTester tester) => [
  for (var id = 1; id <= 20; id++)
    if (_field(id).evaluate().isNotEmpty) id,
];

TextEditingController _controller(WidgetTester tester, int id) => tester
    .widget<EditableText>(
      find.descendant(of: _field(id), matching: find.byType(EditableText)),
    )
    .controller;

Future<void> _focus(WidgetTester tester, int id, [int? caret]) async {
  await tester.tap(_field(id));
  await tester.pumpAndSettle();
  final controller = _controller(tester, id);
  controller.selection = TextSelection.collapsed(
    offset: caret ?? controller.text.length,
  );
  await tester.pump();
}

Future<void> _press(
  WidgetTester tester,
  LogicalKeyboardKey key, {
  bool shift = false,
  bool ctrl = false,
  bool alt = false,
  bool settle = true,
}) async {
  final mods = [
    if (shift) LogicalKeyboardKey.shiftLeft,
    if (ctrl) LogicalKeyboardKey.controlLeft,
    if (alt) LogicalKeyboardKey.altLeft,
  ];
  for (final mod in mods) {
    await tester.sendKeyDownEvent(mod);
  }
  await tester.sendKeyEvent(key);
  for (final mod in mods.reversed) {
    await tester.sendKeyUpEvent(mod);
  }
  settle ? await tester.pumpAndSettle() : await tester.pump();
}

/// 편집 칸에서 Esc 로 블록 선택.
Future<void> _select(WidgetTester tester, int id) async {
  await _focus(tester, id);
  await _press(tester, LogicalKeyboardKey.escape);
  expect(find.text('1개 선택'), findsOneWidget);
}

List<String> _changed(Map<int, String> at) => [
  for (final (i, line) in _doc.indexed) at[i] ?? line,
];

final _windows = TargetPlatformVariant.only(TargetPlatform.windows);

void _test(String name, Future<void> Function(WidgetTester) body) =>
    testWidgets(name, body, variant: _windows);

void main() {
  // ---------- 편집 칸의 키 ----------

  for (final (name, key, shift, ctrl, operation, expected) in [
    (
      'Enter',
      LogicalKeyboardKey.enter,
      false,
      false,
      'enter',
      [..._doc.sublist(0, 3), '', ..._doc.sublist(3)],
    ),
    (
      'Shift+Enter',
      LogicalKeyboardKey.enter,
      true,
      false,
      'newline',
      _changed({2: 'apple two\n'}),
    ),
    (
      'Tab',
      LogicalKeyboardKey.tab,
      false,
      false,
      'indent',
      _changed({2: '  apple two'}),
    ),
    (
      'Shift+Tab',
      LogicalKeyboardKey.tab,
      true,
      false,
      'outdent',
      _changed({}),
    ),
    (
      'Ctrl+Enter',
      LogicalKeyboardKey.enter,
      false,
      true,
      'cycle_todo',
      _changed({2: 'TODO apple two'}),
    ),
  ]) {
    _test('거른 목록의 $name 는 그 블록의 문서 순번에 적용한다', (tester) async {
      final engine = await _pump(tester);
      expect(_shown(tester), [1, 3, 6]);
      await _focus(tester, 3);
      await _press(tester, key, shift: shift, ctrl: ctrl);
      expect(engine.keys.single['operation'], operation);
      expect(engine.keys.single['block'], 2);
      expect(engine.lines, expected);
    });
  }

  _test('글이 같은 블록은 누른 쪽만 바뀐다', (tester) async {
    final engine = await _pump(tester);
    await _focus(tester, 6);
    await _press(tester, LogicalKeyboardKey.tab);
    expect(engine.keys.single['block'], 5);
    expect(engine.lines, _changed({5: '  apple two'}));
  });

  for (final (name, key, caret) in [
    ('맨 앞 Backspace', LogicalKeyboardKey.backspace, 0),
    ('맨 끝 Delete', LogicalKeyboardKey.delete, null),
  ]) {
    _test('거른 목록의 $name 는 합치지 않고 이유를 알린다', (tester) async {
      final engine = await _pump(tester);
      await _focus(tester, 3, caret);
      await _press(tester, key);
      expect(engine.keys, isEmpty);
      expect(engine.lines, _doc);
      expect(find.textContaining('블록 검색'), findsWidgets);
      expect(find.textContaining('합치'), findsOneWidget);
    });
  }

  for (final (name, key) in [
    ('위', LogicalKeyboardKey.arrowUp),
    ('아래', LogicalKeyboardKey.arrowDown),
  ]) {
    _test('거른 목록에서는 블록을 $name로 옮기지 않는다', (tester) async {
      final engine = await _pump(tester);
      await _focus(tester, 3);
      await _press(tester, key, alt: true, shift: true);
      expect(engine.keys, isEmpty);
      await _press(tester, LogicalKeyboardKey.escape);
      await _press(tester, key, alt: true, shift: true);
      expect(engine.keys, isEmpty);
      expect(find.textContaining('옮기'), findsOneWidget);
    });
  }

  _test('거르지 않으면 합치기는 그대로다', (tester) async {
    final engine = await _pump(tester, search: null);
    await _focus(tester, 3, 0);
    await _press(tester, LogicalKeyboardKey.backspace);
    expect(engine.keys.single['block'], 2);
  });

  _test('모든 블록이 검색되면 합치기는 그대로다', (tester) async {
    final engine = await _pump(
      tester,
      lines: ['apple a', 'apple b'],
      search: 'apple',
    );
    await _focus(tester, 2, 0);
    await _press(tester, LogicalKeyboardKey.backspace);
    expect(engine.keys.single['block'], 1);
  });

  // ---------- 블록 선택 ----------

  for (final (name, key, shift, ctrl, operation) in [
    ('Tab', LogicalKeyboardKey.tab, false, false, 'indent'),
    ('Shift+Tab', LogicalKeyboardKey.tab, true, false, 'outdent'),
    ('Ctrl+Enter', LogicalKeyboardKey.enter, false, true, 'cycle_todo'),
    ('Delete', LogicalKeyboardKey.delete, false, false, 'delete_selection'),
    ('Backspace', LogicalKeyboardKey.backspace, false, false, 'delete_selection'),
  ]) {
    _test('거른 목록의 블록 선택 $name 는 선택한 블록의 문서 순번을 보낸다', (tester) async {
      final engine = await _pump(tester);
      await _select(tester, 6);
      await _press(tester, key, shift: shift, ctrl: ctrl);
      expect(engine.keys.single['operation'], operation);
      expect(engine.keys.single['selection'], [5]);
    });
  }

  for (final (tooltip, operation) in [
    ('선택 블록 삭제 (Delete)', 'delete_selection'),
    ('선택 블록 들여쓰기 (Tab)', 'indent'),
    ('선택 블록 내어쓰기 (Shift+Tab)', 'outdent'),
    ('선택 블록 접기', 'collapse'),
  ]) {
    _test('거른 목록의 일괄 단추 "$tooltip"', (tester) async {
      final engine = await _pump(tester);
      await _select(tester, 6);
      await tester.tap(find.byTooltip(tooltip));
      await tester.pumpAndSettle();
      expect(engine.keys.single['operation'], operation);
      expect(engine.keys.single['selection'], [5]);
    });
  }

  _test('범위 선택은 보이는 블록만 고른다', (tester) async {
    final engine = await _pump(tester);
    await _select(tester, 1);
    await _press(tester, LogicalKeyboardKey.arrowDown, shift: true);
    await _press(tester, LogicalKeyboardKey.tab);
    expect(engine.keys.single['selection'], [0, 2]);
  });

  for (final (name, key, ctrl) in [
    ('삭제', LogicalKeyboardKey.delete, false),
    ('잘라내기', LogicalKeyboardKey.keyX, true),
  ]) {
    _test('가려진 하위가 있는 블록의 $name 는 막고 이유를 알린다', (tester) async {
      final engine = await _pump(tester);
      await _select(tester, 3);
      await _press(tester, key, ctrl: ctrl);
      expect(engine.keys, isEmpty);
      expect(engine.lines, _doc);
      expect(find.textContaining('가려진 하위'), findsOneWidget);
      expect(find.text('1개 선택'), findsOneWidget);
    });
  }

  _test('가려진 하위가 있어도 복사는 한다', (tester) async {
    final engine = await _pump(tester);
    await _select(tester, 3);
    await _press(tester, LogicalKeyboardKey.keyC, ctrl: true);
    expect(engine.keys.single['operation'], 'copy');
    expect(engine.keys.single['selection'], [2]);
  });

  _test('하위가 모두 보이면 삭제한다', (tester) async {
    final engine = await _pump(
      tester,
      lines: ['other', 'apple p', '  apple c'],
    );
    await _select(tester, 2);
    await _press(tester, LogicalKeyboardKey.delete);
    expect(engine.keys.single['selection'], [1]);
  });

  _test('Ctrl+A 는 가려진 상위 블록을 선택하지 않는다', (tester) async {
    final engine = await _pump(tester, search: 'child');
    await _select(tester, 4);
    await _press(tester, LogicalKeyboardKey.keyA, ctrl: true);
    expect(find.textContaining('상위 블록'), findsOneWidget);
    await _press(tester, LogicalKeyboardKey.delete);
    expect(engine.keys.single['selection'], [3]);
  });

  _test('검색하면 가려질 수 있는 앞의 블록 선택은 푼다', (tester) async {
    await _pump(tester, search: null);
    await _select(tester, 2);
    await _search(tester, 'apple');
    expect(find.text('1개 선택'), findsNothing);
    expect(find.byTooltip('선택 블록 삭제 (Delete)'), findsNothing);
  });

  // ---------- 줄 단추 ----------

  for (final (tooltip, operation) in [
    ('들여쓰기', 'indent'),
    ('내어쓰기', 'outdent'),
    ('접기/펼치기', 'toggle_collapse'),
  ]) {
    _test('거른 목록의 줄 단추 "$tooltip"', (tester) async {
      final engine = await _pump(tester);
      await tester.tap(find.byTooltip(tooltip).at(1));
      await tester.pumpAndSettle();
      expect(engine.keys.single['operation'], operation);
      expect(engine.keys.single['block'], 2);
    });
  }

  _test('거른 목록의 완료 단추는 그 블록만 바꾼다', (tester) async {
    final engine = await _pump(tester);
    await tester.tap(find.byTooltip('완료 표시').at(1));
    await tester.pumpAndSettle();
    expect(
      [for (final e in engine.edits) (e['operation'], e['block_id'], e['text'])],
      [('set_text', 3, 'apple two'), ('toggle_status', 3, null)],
    );
    expect(engine.lines, _doc);
  });

  _test('거른 목록의 새 블록은 문서 끝에 넣는다', (tester) async {
    final engine = await _pump(tester, search: 'banana');
    await tester.tap(find.byTooltip('새 블록'));
    await tester.pumpAndSettle();
    expect(engine.edits.single['block_id'], 6);
    expect(engine.lines, [..._doc, '']);
  });

  // ---------- 확정 전 글·붙여넣기 ----------

  _test('거른 목록에서 칸을 벗어난 글은 그 블록에 저장한다', (tester) async {
    final engine = await _pump(tester);
    await _focus(tester, 6);
    await tester.enterText(_field(6), 'apple !');
    await tester.tapAt(const Offset(100, 860), kind: PointerDeviceKind.mouse);
    await tester.pumpAndSettle();
    expect(engine.keys.single['block'], 5);
    expect(engine.lines, _changed({5: 'apple !'}));
  });

  _test('거른 목록의 여러 줄 붙여넣기는 그 블록에 보낸다', (tester) async {
    final engine = await _pump(tester);
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.platform,
      (call) async =>
          call.method == 'Clipboard.getData' ? {'text': 'a\nb'} : null,
    );
    await _focus(tester, 3);
    await _press(tester, LogicalKeyboardKey.keyV, ctrl: true);
    expect(engine.keys.single['operation'], 'paste');
    expect(engine.keys.single['block'], 2);
  });

  // ---------- 예약 입력 ----------

  _test('처리 중에 예약한 Tab 도 같은 블록의 문서 순번에 적용한다', (tester) async {
    final engine = await _pump(tester);
    await _focus(tester, 3);
    engine.hold = Completer<void>();
    await _press(tester, LogicalKeyboardKey.tab, settle: false);
    await _press(tester, LogicalKeyboardKey.tab, settle: false);
    engine.hold!.complete();
    await tester.pumpAndSettle();
    expect([for (final k in engine.keys) k['block']], [2, 2]);
    expect(engine.lines, _changed({2: '    apple two'}));
  });

  for (final (name, key, caret) in [
    ('Backspace', LogicalKeyboardKey.backspace, 0),
    ('Delete', LogicalKeyboardKey.delete, null),
  ]) {
    _test('거른 목록에서 처리 중에 누른 $name 는 블록을 합치지 않는다', (tester) async {
      final engine = await _pump(tester);
      await _focus(tester, 3, caret);
      engine.hold = Completer<void>();
      await _press(tester, LogicalKeyboardKey.tab, settle: false);
      await _press(tester, key, settle: false);
      engine.hold!.complete();
      await tester.pumpAndSettle();
      expect([for (final k in engine.keys) k['operation']], ['indent']);
      expect(engine.lines, _changed({2: '  apple two'}));
      expect(find.textContaining('블록 검색 중'), findsOneWidget);
    });
  }

  // ---------- 접힌 블록 ----------

  _test('접힘 상태는 문서 순번으로 보낸다', (tester) async {
    // 3번(apple two)이 접혀 있다. 검색 결과에서는 두 번째 줄이다.
    final engine = await _pump(tester, folded: {2});
    await _focus(tester, 6);
    await _press(tester, LogicalKeyboardKey.tab);
    expect(engine.keys.single['folded'], [2]);
  });

  _test('검색에 없는 블록의 접힘도 잃지 않는다', (tester) async {
    final engine = await _pump(tester, folded: {2}, search: 'banana');
    await _focus(tester, 2);
    await _press(tester, LogicalKeyboardKey.tab);
    expect(engine.keys.single['folded'], [2]);
  });

  _test('거른 목록의 완료 단추는 다른 블록을 접지 않는다', (tester) async {
    await _pump(tester, folded: {2}, search: 'banana');
    await tester.tap(find.byTooltip('완료 표시'));
    await tester.pumpAndSettle();
    expect(_shown(tester), [1, 2, 3, 5, 6]); // 접힌 3번의 하위(4번)만 가려진다.
  });

  _test('접힌 블록은 자기 하위가 아닌 검색 결과를 가리지 않는다', (tester) async {
    await _pump(
      tester,
      lines: ['apple p', '  apple c', 'other', '  apple deep'],
      folded: {0},
    );
    expect(_shown(tester), [1, 4]);
  });

  // ---------- 검색 상태 ----------

  _test('다시 검색하면 문서 전체에서 새로 거른다', (tester) async {
    await _pump(tester);
    await _search(tester, 'banana');
    expect(_shown(tester), [2]);
  });

  _test('검색 칸을 비우면 거르기를 푼다', (tester) async {
    await _pump(tester);
    await tester.enterText(_searchBox, '');
    await tester.pumpAndSettle();
    expect(_shown(tester), [1, 2, 3, 4, 5, 6]);
  });

  _test('결과가 없으면 빈 문서로 안내하지 않는다', (tester) async {
    await _pump(tester, search: 'zzz');
    expect(_shown(tester), isEmpty);
    expect(find.textContaining('블록이 없습니다. +'), findsNothing);
    expect(find.textContaining('검색 결과가 없습니다'), findsOneWidget);
  });
}
