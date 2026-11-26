import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:docfit_ui/features/outliner/outliner_typing.dart';
import 'package:docfit_ui/features/outliner/outliner_workspace.dart';

class _MemoryRecent implements RecentPagesStore {
  @override
  Future<List<String>> load() async => [];
  @override
  Future<void> save(List<String> pages) async {}
}

/// 화면이 보내는 `key` 요청을 기록하는 가짜 엔진. 규칙 자체는 Rust 시험(outliner_core/tests/keys.rs)이 맡고,
/// 여기서는 칸의 글을 반영한 뒤 정해 준 커서·선택·클립보드를 돌려준다.
class _Engine {
  _Engine(List<String> texts)
    : blocks = [
        for (final (i, t) in texts.indexed)
          {'id': i + 1, 'text': t, 'indent': 0, 'folded': false},
      ];
  List<Map<String, Object?>> blocks;
  final List<Map<String, Object?>> keys = [];
  Map<String, Object?> Function(Map<String, Object?> request)? reply;
  int saves = 0;

  String get markdown => blocks.map((b) => '- ${b['text']}\n').join();

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
          'document': {'blocks': blocks},
        };
      case 'backlinks':
        data = {'block_ids': []};
      case 'save':
        saves++;
        data = {'version': 'v$saves'};
      case 'key':
        keys.add(p);
        final index = p['block'] as int?;
        if (index != null && p['text'] != null) {
          blocks = [
            for (final (i, b) in blocks.indexed)
              i == index ? {...b, 'text': p['text']} : b,
          ];
        }
        final extra = reply?.call(p) ?? {};
        data = {
          'handled': true,
          'markdown': markdown,
          'document': {'blocks': blocks},
          'selection': p['selection'],
          'cursor': index == null
              ? null
              : {'block': index, 'start': p['start'], 'end': p['start']},
          ...extra,
        };
    }
    return {'ok': true, 'data': data};
  }

  List<Object?> get operations => [for (final k in keys) k['operation']];
}

Future<_Engine> _pump(WidgetTester tester, List<String> texts) async {
  tester.view.physicalSize = const Size(1400, 900);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  final engine = _Engine(texts);
  await tester.pumpWidget(MaterialApp(
    home: Scaffold(
      body: OutlinerWorkspace(
        api: engine.call,
        recentPagesStore: _MemoryRecent(),
      ),
    ),
  ));
  await tester.pumpAndSettle();
  return engine;
}

Finder _field(int id) => find.byKey(ValueKey('outliner-block-$id'));

EditableText _editable(WidgetTester tester, int id) => tester.widget(
  find.descendant(of: _field(id), matching: find.byType(EditableText)),
);

Future<void> _focus(WidgetTester tester, int id, [int? caret]) async {
  await tester.tap(_field(id));
  await tester.pumpAndSettle();
  final controller = _editable(tester, id).controller;
  controller.selection = TextSelection.collapsed(
    offset: caret ?? controller.text.length,
  );
  await tester.pump();
}

Future<void> _press(
  WidgetTester tester,
  LogicalKeyboardKey key, {
  bool ctrl = false,
  bool alt = false,
  bool shift = false,
}) async {
  final mods = [
    if (ctrl) LogicalKeyboardKey.controlLeft,
    if (alt) LogicalKeyboardKey.altLeft,
    if (shift) LogicalKeyboardKey.shiftLeft,
  ];
  for (final m in mods) {
    await tester.sendKeyDownEvent(m);
  }
  await tester.sendKeyEvent(key);
  for (final m in mods.reversed) {
    await tester.sendKeyUpEvent(m);
  }
  await tester.pumpAndSettle();
}

void main() {
  testWidgets('editing keys send the matching rule with block, caret and text', (
    tester,
  ) async {
    final engine = await _pump(tester, ['abcd', 'ef']);
    await _focus(tester, 1, 2);
    final cases = <(LogicalKeyboardKey, bool, bool, bool, int, String)>[
      (LogicalKeyboardKey.enter, false, false, false, 2, 'enter'),
      (LogicalKeyboardKey.enter, false, false, true, 2, 'newline'),
      (LogicalKeyboardKey.enter, true, false, false, 2, 'cycle_todo'),
      (LogicalKeyboardKey.tab, false, false, false, 2, 'indent'),
      (LogicalKeyboardKey.tab, false, false, true, 2, 'outdent'),
      (LogicalKeyboardKey.arrowUp, false, true, true, 2, 'move_up'),
      (LogicalKeyboardKey.arrowDown, false, true, true, 2, 'move_down'),
      (LogicalKeyboardKey.arrowUp, true, false, false, 2, 'collapse'),
      (LogicalKeyboardKey.arrowDown, true, false, false, 2, 'expand'),
      (LogicalKeyboardKey.semicolon, true, false, false, 2, 'toggle_collapse'),
      (LogicalKeyboardKey.arrowUp, false, false, false, 2, 'up'),
      (LogicalKeyboardKey.arrowDown, false, false, false, 2, 'down'),
      (LogicalKeyboardKey.backspace, false, false, false, 0, 'backspace'),
      (LogicalKeyboardKey.arrowLeft, false, false, false, 0, 'left'),
      (LogicalKeyboardKey.delete, false, false, false, 4, 'delete'),
      (LogicalKeyboardKey.arrowRight, false, false, false, 4, 'right'),
    ];
    for (final (key, ctrl, alt, shift, caret, operation) in cases) {
      await _focus(tester, 1, caret);
      engine.keys.clear();
      await _press(tester, key, ctrl: ctrl, alt: alt, shift: shift);
      expect(engine.operations, [operation], reason: operation);
      expect(engine.keys.single['block'], 0, reason: operation);
      expect(engine.keys.single['start'], caret, reason: operation);
      expect(engine.keys.single['text'], 'abcd', reason: operation);
    }
  });

  testWidgets('keys in the middle of the text stay with the field', (
    tester,
  ) async {
    final engine = await _pump(tester, ['abcd', '첫\n둘']);
    await _focus(tester, 1, 2);
    for (final key in [
      LogicalKeyboardKey.backspace,
      LogicalKeyboardKey.delete,
      LogicalKeyboardKey.arrowLeft,
      LogicalKeyboardKey.arrowRight,
    ]) {
      await _press(tester, key);
    }
    expect(engine.keys, isEmpty);
    // 여러 줄 블록: 첫 줄의 ↓, 둘째 줄의 ↑ 는 블록 안에서 움직인다.
    await _focus(tester, 2, 0);
    engine.keys.clear(); // 앞 칸에서 지운 글의 저장(commit)
    await _press(tester, LogicalKeyboardKey.arrowDown);
    expect(engine.keys, isEmpty);
    expect(_editable(tester, 2).controller.selection.baseOffset, greaterThan(1));
    await _press(tester, LogicalKeyboardKey.arrowUp);
    expect(engine.keys, isEmpty);
    await _press(tester, LogicalKeyboardKey.arrowUp);
    expect(engine.operations, ['up']);
  });

  testWidgets('cursor and focus follow the engine result', (tester) async {
    final engine = await _pump(tester, ['abcd']);
    engine.reply = (p) {
      engine.blocks = [
        {'id': 1, 'text': 'ab', 'indent': 0, 'folded': false},
        {'id': 2, 'text': 'cd', 'indent': 1, 'folded': false},
      ];
      return {
        'markdown': engine.markdown,
        'document': {'blocks': engine.blocks},
        'cursor': {'block': 1, 'start': 1, 'end': 1},
      };
    };
    await _focus(tester, 1, 2);
    await _press(tester, LogicalKeyboardKey.enter);
    expect(_editable(tester, 1).controller.text, 'ab');
    expect(_editable(tester, 2).focusNode.hasFocus, isTrue);
    expect(_editable(tester, 2).controller.selection.baseOffset, 1);
    expect(engine.saves, 1, reason: '명령 하나 = 저장 한 번 = 실행 취소 한 단계');
  });

  for (final (name, key, operation) in [
    ('Enter', LogicalKeyboardKey.enter, 'enter'),
    ('Tab', LogicalKeyboardKey.tab, 'indent'),
  ]) {
    testWidgets('$name during Korean composition commits once then runs', (
      tester,
    ) async {
      final engine = await _pump(tester, ['']);
      await _focus(tester, 1);
      Future<void> compose() async {
        tester.testTextInput.updateEditingValue(
          const TextEditingValue(
            text: '한',
            selection: TextSelection.collapsed(offset: 1),
            composing: TextRange(start: 0, end: 1),
          ),
        );
        await tester.pump();
      }

      await compose();
      // 조합 중 Backspace 는 IME 몫이라 블록 명령을 보내지 않는다(시험 틀은 글자를 직접 지운다).
      await _press(tester, LogicalKeyboardKey.backspace);
      expect(engine.keys, isEmpty);
      await compose();
      await _press(tester, key);
      expect(engine.operations, [operation]);
      expect(engine.keys.single['text'], '한', reason: '유실·중복 없음');
      expect(_editable(tester, 1).controller.text, '한');
    });
  }

  testWidgets('block selection: Esc, extend, batch keys and clipboard', (
    tester,
  ) async {
    String? clipboard;
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.platform,
      (call) async {
        if (call.method == 'Clipboard.setData') {
          clipboard = (call.arguments as Map)['text'] as String?;
        }
        if (call.method == 'Clipboard.getData') return {'text': clipboard};
        return null;
      },
    );
    addTearDown(
      () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        null,
      ),
    );
    final engine = await _pump(tester, ['one', 'two', 'three']);
    engine.reply = (p) =>
        p['operation'] == 'copy' ? {'clipboard': '- one\n- two'} : {};
    await _focus(tester, 1);
    await _press(tester, LogicalKeyboardKey.escape);
    expect(find.text('1개 선택'), findsOneWidget);
    await _press(tester, LogicalKeyboardKey.arrowDown, shift: true);
    expect(find.text('2개 선택'), findsOneWidget);
    for (final (key, ctrl, shift, operation) in [
      (LogicalKeyboardKey.tab, false, false, 'indent'),
      (LogicalKeyboardKey.tab, false, true, 'outdent'),
      (LogicalKeyboardKey.enter, true, false, 'cycle_todo'),
      (LogicalKeyboardKey.keyC, true, false, 'copy'),
      (LogicalKeyboardKey.keyV, true, false, 'paste'),
      (LogicalKeyboardKey.delete, false, false, 'delete_selection'),
    ]) {
      engine.keys.clear();
      await _press(tester, key, ctrl: ctrl, shift: shift);
      expect(engine.operations, [operation], reason: operation);
      expect(engine.keys.single['selection'], [0, 1], reason: operation);
      expect(engine.keys.single.containsKey('block'), isFalse);
    }
    expect(clipboard, '- one\n- two');
    await _press(tester, LogicalKeyboardKey.escape);
    expect(find.textContaining('개 선택'), findsNothing);
  });

  testWidgets('multi line paste builds blocks, single line stays in the field', (
    tester,
  ) async {
    var data = '둘째\n셋째';
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.platform,
      (call) async =>
          call.method == 'Clipboard.getData' ? {'text': data} : null,
    );
    addTearDown(
      () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        null,
      ),
    );
    final engine = await _pump(tester, ['첫']);
    await _focus(tester, 1);
    await _press(tester, LogicalKeyboardKey.keyV, ctrl: true);
    expect(engine.operations, ['paste']);
    expect(engine.keys.single['paste'], '둘째\n셋째');
    engine.keys.clear();
    data = ' 한 줄\n';
    await _focus(tester, 1);
    await _press(tester, LogicalKeyboardKey.keyV, ctrl: true);
    expect(engine.keys, isEmpty);
    expect(_editable(tester, 1).controller.text, '첫 한 줄');
    // Ctrl+Shift+V 는 블록을 나누지 않고 칸에 넣은 뒤 확정한다.
    data = 'x\ny';
    await _press(tester, LogicalKeyboardKey.keyV, ctrl: true, shift: true);
    expect(engine.operations, ['commit']);
    expect(engine.keys.single['text'], '첫 한 줄x\ny');
  });

  testWidgets('bracket pairs and keyboard completion of page links', (
    tester,
  ) async {
    final engine = await _pump(tester, ['']);
    await _focus(tester, 1);
    Future<void> type(String text, int caret) async {
      tester.testTextInput.updateEditingValue(
        TextEditingValue(
          text: text,
          selection: TextSelection.collapsed(offset: caret),
        ),
      );
      await tester.pumpAndSettle();
    }

    await type('[', 1);
    expect(_editable(tester, 1).controller.text, '[]');
    await type('[[]', 2);
    expect(_editable(tester, 1).controller.text, '[[]]');
    expect(find.widgetWithText(ListTile, '합성'), findsWidgets);
    await _press(tester, LogicalKeyboardKey.enter);
    expect(_editable(tester, 1).controller.text, '[[합성]]');
    expect(_editable(tester, 1).controller.selection.baseOffset, 6);
    expect(engine.keys, isEmpty, reason: '후보 선택 Enter 는 새 블록을 만들지 않는다');
  });

  testWidgets('clicking another block saves the unconfirmed text', (
    tester,
  ) async {
    final engine = await _pump(tester, ['one', 'two']);
    await _focus(tester, 1);
    await tester.enterText(_field(1), 'one 고침');
    await tester.tap(_field(2));
    await tester.pumpAndSettle();
    expect(engine.operations, ['commit']);
    expect(engine.keys.single['block'], 0);
    expect(engine.keys.single['text'], 'one 고침');
    expect(engine.saves, 1);
  });

  test('Dart pair rules match the Rust test vectors', () {
    final vectors =
        jsonDecode(
              File(
                '../outliner_core/testdata/typing_rules.json',
              ).readAsStringSync(),
            )
            as Map;
    for (final v in vectors['type'] as List) {
      final r = typeChar(
        v['text'] as String,
        v['start'] as int,
        v['end'] as int,
        v['char'] as String,
      );
      expect(
        r == null ? null : [r.text, r.start, r.end],
        v['expect'],
        reason: '$v',
      );
    }
    for (final v in vectors['backspace'] as List) {
      final r = pairBackspace(v['text'] as String, v['caret'] as int);
      expect(r == null ? null : [r.text, r.caret], v['expect'], reason: '$v');
    }
  });
}
