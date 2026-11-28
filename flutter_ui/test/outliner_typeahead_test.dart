import 'dart:async';
import 'dart:ui';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:docfit_ui/features/outliner/outliner_typeahead.dart';
import 'package:docfit_ui/features/outliner/outliner_workspace.dart';

// OUTKEYS2: 느린 엔진이 처리하는 동안 누른 Enter·친 글의 순서가 보존되는지.

class _MemoryRecent implements RecentPagesStore {
  @override
  Future<List<String>> load() async => [];
  @override
  Future<void> save(List<String> pages) async {}
}

/// 블록 번호를 매번 1.. 로 다시 매기는 가짜 엔진(실제 엔진과 같다).
/// `key`·`save` 응답은 [slowKeys]·[slowSaves] 이면 시험이 [release] 할 때까지 멈춘다.
class _Engine {
  _Engine(this.texts);
  List<String> texts;
  final List<Map<String, Object?>> keys = [];
  final List<String> loads = [];
  bool slowKeys = false;
  bool slowSaves = false;
  bool slowParse = false;
  bool failKey = false;
  List<int> selectResult = [];
  final List<Completer<void>> _held = [];

  String get markdown => texts.map((t) => '- $t\n').join();
  Map<String, Object?> get document => {
    'blocks': [
      for (final (i, t) in texts.indexed)
        {'id': i + 1, 'text': t, 'indent': 0, 'folded': false},
    ],
  };
  List<Object?> get operations => [for (final k in keys) k['operation']];

  Future<void> _hold() {
    final c = Completer<void>();
    _held.add(c);
    return c.future;
  }

  void release() => _held.removeAt(0).complete();

  Future<Object?> call(String method, [List<Object?> args = const []]) async {
    final p = Map<String, Object?>.from(args.first as Map);
    Map<String, Object?> data = {};
    switch (p['action']) {
      case 'list':
        data = {'pages': [{'name': '합성'}, {'name': '다른'}]};
      case 'daily':
      case 'load':
        loads.add('${p['page'] ?? '합성'}');
        data = {
          'page': p['page'] ?? '합성',
          'markdown': markdown,
          'version': 'v0',
          'document': document,
        };
      case 'backlinks':
        data = {'block_ids': []};
      case 'parse':
        if (slowParse) await _hold();
        texts = [
          for (final line in (p['markdown'] as String).split('\n'))
            if (line.isNotEmpty) line.substring(2),
        ];
        data = document;
      case 'save':
        if (slowSaves) await _hold();
        data = {'version': 'v1'};
      case 'key':
        keys.add(p);
        if (slowKeys) await _hold();
        if (failKey) {
          return {'ok': false, 'error': {'message': '합성 실패'}};
        }
        final i = p['block'] as int;
        final text = p['text'] as String;
        var start = p['start'] as int;
        var at = i;
        texts = [...texts]..[i] = text;
        if (p['operation'] == 'enter') {
          texts[i] = text.substring(0, start);
          texts.insert(i + 1, text.substring(p['end'] as int));
          at = i + 1;
          start = 0;
        } else if (p['operation'] == 'backspace' && i > 0) {
          at = i - 1;
          start = texts[at].length;
          texts[at] += texts.removeAt(i);
        } else if (p['operation'] == 'delete' && i + 1 < texts.length) {
          texts[i] += texts.removeAt(i + 1);
        }
        data = {
          'handled': true,
          'markdown': markdown,
          'document': document,
          'selection': selectResult,
          'cursor': {'block': at, 'start': start, 'end': start},
        };
    }
    return {'ok': true, 'data': data};
  }
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

List<String> _texts(WidgetTester tester) => [
  for (final e in tester.widgetList<EditableText>(
    find.descendant(
      of: find.byWidgetPredicate(
        (w) =>
            w is TextField &&
            w.key is ValueKey<String> &&
            (w.key as ValueKey<String>).value.startsWith('outliner-block-'),
      ),
      matching: find.byType(EditableText),
    ),
  ))
    e.controller.text,
];

int _focused(WidgetTester tester) {
  for (var id = 1; _field(id).evaluate().isNotEmpty; id++) {
    if (_editable(tester, id).focusNode.hasFocus) return id;
  }
  return 0;
}

Future<void> _focus(WidgetTester tester, int id, [int? caret]) async {
  await tester.tap(_field(id));
  await tester.pumpAndSettle();
  final controller = _editable(tester, id).controller;
  controller.selection = TextSelection.collapsed(
    offset: caret ?? controller.text.length,
  );
  await tester.pump();
}

/// 지금 입력을 받는 칸의 커서 자리에 글을 친다(IME 가 보내는 값과 같은 모양).
Future<void> _type(WidgetTester tester, String s, {bool composing = false}) async {
  final value = _editable(tester, _focused(tester)).controller.value;
  final from = composing && value.composing.isValid
      ? value.composing.start
      : value.selection.start;
  final to = composing && value.composing.isValid
      ? value.composing.end
      : value.selection.end;
  tester.testTextInput.updateEditingValue(
    TextEditingValue(
      text: value.text.replaceRange(from, to, s),
      selection: TextSelection.collapsed(offset: from + s.length),
      composing: composing
          ? TextRange(start: from, end: from + s.length)
          : TextRange.empty,
    ),
  );
  await tester.pump();
}

Future<bool> _key(WidgetTester tester, LogicalKeyboardKey key) async {
  final handled = await tester.sendKeyEvent(key);
  await tester.pump();
  return handled;
}

Future<void> _release(WidgetTester tester, _Engine engine) async {
  engine.release();
  await tester.pumpAndSettle();
}

Future<_Engine> _slowEnter(
  WidgetTester tester, [
  List<String> texts = const ['abcd'],
  int caret = 2,
]) async {
  final engine = await _pump(tester, [...texts]);
  await _focus(tester, 1, caret);
  engine.slowKeys = true;
  await _key(tester, LogicalKeyboardKey.enter);
  return engine;
}

void main() {
  testWidgets('repeated Enter while busy is applied in order', (tester) async {
    final engine = await _slowEnter(tester);
    await _key(tester, LogicalKeyboardKey.enter);
    await _key(tester, LogicalKeyboardKey.enter);
    expect(engine.operations, ['enter'], reason: '겹쳐 보내지 않는다');
    for (var i = 0; i < 3; i++) {
      await _release(tester, engine);
    }
    expect(engine.operations, ['enter', 'enter', 'enter']);
    expect([for (final k in engine.keys) k['block']], [0, 1, 2]);
    expect(engine.texts, ['ab', '', '', 'cd']);
    expect(_focused(tester), 4);
  });

  testWidgets('typing right after Enter goes to the new block', (tester) async {
    final engine = await _slowEnter(tester);
    await _type(tester, 'x');
    await _type(tester, 'y');
    await _release(tester, engine);
    expect(_texts(tester), ['ab', 'xycd']);
    expect(_focused(tester), 2);
    expect(_editable(tester, 2).controller.selection.baseOffset, 2);
  });

  testWidgets('character keys are left to the text input while busy', (
    tester,
  ) async {
    final engine = await _slowEnter(tester);
    expect(await _key(tester, LogicalKeyboardKey.keyA), isFalse);
    expect(await _key(tester, LogicalKeyboardKey.space), isFalse);
    // 칸에는 바로 손대지 않는다(Backspace 는 예약, 화살표는 받지 않는다).
    expect(await _key(tester, LogicalKeyboardKey.backspace), isTrue);
    expect(await _key(tester, LogicalKeyboardKey.arrowLeft), isTrue);
    expect(_texts(tester), ['abcd']);
    expect(_editable(tester, 1).controller.selection.baseOffset, 2);
    await _type(tester, 'zq');
    // 친 글 안의 Backspace 는 지워지고, 그보다 앞의 글은 바로 지워지지 않는다.
    for (var i = 0; i < 3; i++) {
      await _key(tester, LogicalKeyboardKey.backspace);
    }
    expect(_texts(tester), ['abcd']);
    await _type(tester, 'k');
    await _release(tester, engine);
    await _release(tester, engine);
    // 예약한 첫 Backspace 는 새 블록 맨 앞이라 합치고, 둘째는 그 자리의 'b' 를 지운다.
    expect(engine.operations, ['enter', 'backspace']);
    expect(_texts(tester), ['akcd']);
  });

  testWidgets('Enter, text, Enter, text keeps every piece in its block', (
    tester,
  ) async {
    final engine = await _slowEnter(tester);
    await _type(tester, 'one');
    await _key(tester, LogicalKeyboardKey.enter);
    await _type(tester, 'two');
    await _key(tester, LogicalKeyboardKey.enter);
    await _type(tester, '3');
    await _release(tester, engine);
    expect(engine.keys[1]['text'], 'onecd');
    expect(engine.keys[1]['start'], 3);
    await _type(tester, '4'); // 두 번째 Enter 처리 중에 더 친 글
    await _release(tester, engine);
    await _release(tester, engine);
    expect(engine.operations, ['enter', 'enter', 'enter']);
    expect(_texts(tester), ['ab', 'one', 'two', '34cd']);
    expect(_focused(tester), 4);
    expect(_editable(tester, 4).controller.selection.baseOffset, 2);
  });

  testWidgets('Korean composition typed while busy arrives once', (tester) async {
    final engine = await _slowEnter(tester, [''], 0);
    await _type(tester, 'ㅎ', composing: true);
    await _type(tester, '하', composing: true);
    await _type(tester, '한', composing: true);
    await _release(tester, engine);
    expect(_texts(tester), ['', '한']);
    await _type(tester, '글');
    expect(_texts(tester), ['', '한글']);
  });

  testWidgets('Enter while composing during a command takes the syllable', (
    tester,
  ) async {
    final engine = await _slowEnter(tester, [''], 0);
    await _type(tester, '글', composing: true);
    await _key(tester, LogicalKeyboardKey.enter);
    await _release(tester, engine);
    expect(engine.keys[1]['text'], '글');
    expect(engine.keys[1]['start'], 1);
    await _release(tester, engine);
    expect(_texts(tester), ['', '글', '']);
  });

  testWidgets('Enter and Tab keep the order they were pressed in', (
    tester,
  ) async {
    final engine = await _slowEnter(tester);
    await _type(tester, 'a');
    await _key(tester, LogicalKeyboardKey.tab);
    await _type(tester, 'b');
    await _key(tester, LogicalKeyboardKey.enter);
    await _type(tester, 'c');
    for (var i = 0; i < 3; i++) {
      await _release(tester, engine);
    }
    expect(engine.operations, ['enter', 'indent', 'enter']);
    expect(engine.keys[1]['text'], 'acd');
    expect(engine.keys[1]['start'], 1);
    expect(engine.keys[2]['text'], 'abcd');
    expect(engine.keys[2]['start'], 2);
    expect(_texts(tester), ['ab', 'ab', 'ccd']);
  });

  testWidgets('typing and Enter while the save is still running', (
    tester,
  ) async {
    final engine = await _pump(tester, ['abcd']);
    await _focus(tester, 1, 2);
    engine.slowSaves = true;
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pumpAndSettle();
    expect(_focused(tester), 2);
    await _type(tester, 'x');
    await _key(tester, LogicalKeyboardKey.enter);
    await _type(tester, 'y');
    engine.slowSaves = false;
    await _release(tester, engine);
    expect(engine.operations, ['enter', 'enter']);
    expect(_texts(tester), ['ab', 'x', 'ycd']);
  });

  testWidgets('input arriving before focus has moved follows the cursor', (
    tester,
  ) async {
    final engine = await _slowEnter(tester);
    engine.release();
    await tester.idle(); // 응답은 왔지만 아직 다시 그리기 전
    tester.testTextInput.updateEditingValue(
      const TextEditingValue(
        text: 'abz',
        selection: TextSelection.collapsed(offset: 3),
      ),
    );
    await tester.pumpAndSettle();
    expect(_texts(tester), ['ab', 'zcd']);
  });

  testWidgets('Shift+Enter and Ctrl+Enter are queued as their own rules', (
    tester,
  ) async {
    final engine = await _slowEnter(tester);
    for (final mod in [
      LogicalKeyboardKey.shiftLeft,
      LogicalKeyboardKey.controlLeft,
    ]) {
      await tester.sendKeyDownEvent(mod);
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.sendKeyUpEvent(mod);
    }
    await tester.pump();
    for (var i = 0; i < 3; i++) {
      await _release(tester, engine);
    }
    expect(engine.operations, ['enter', 'newline', 'cycle_todo']);
  });

  testWidgets('bracket pairs and link completion typed while busy', (
    tester,
  ) async {
    final engine = await _slowEnter(tester);
    await _type(tester, '[');
    await _type(tester, '[');
    expect(_texts(tester), ['ab[[]]cd']);
    // 후보가 열려 있으면 Enter 는 처리 중이 아닐 때처럼 후보를 고른다.
    await _key(tester, LogicalKeyboardKey.enter);
    expect(_texts(tester), ['ab[[합성]]cd']);
    await _release(tester, engine);
    expect(engine.operations, ['enter']);
    expect(_texts(tester), ['ab', '[[합성]]cd']);
    expect(_editable(tester, 2).controller.selection.baseOffset, 6);
  });

  testWidgets('text typed while undo runs follows the block', (tester) async {
    final engine = await _pump(tester, ['abcd']);
    await _focus(tester, 1, 2);
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pumpAndSettle();
    expect(_texts(tester), ['ab', 'cd']);
    engine.slowParse = true;
    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.keyZ);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await tester.pump();
    await _type(tester, 'u');
    await _release(tester, engine);
    // 커서가 있던 둘째 블록이 사라지므로 마지막 블록으로 옮긴다.
    expect(_texts(tester), ['abcdu']);
    expect(_focused(tester), 1);
  });

  testWidgets('a block selection result does not wipe typed text', (
    tester,
  ) async {
    final engine = await _slowEnter(tester);
    engine.selectResult = [1];
    await _type(tester, 'x');
    await _release(tester, engine);
    expect(_texts(tester), ['ab', 'xcd']);
    expect(find.textContaining('개 선택'), findsNothing);
  });

  testWidgets('a failed command keeps the typed text and says what was skipped', (
    tester,
  ) async {
    final engine = await _slowEnter(tester);
    await _type(tester, 'x');
    await _key(tester, LogicalKeyboardKey.enter);
    await _type(tester, 'y');
    engine.failKey = true;
    await _release(tester, engine);
    expect(engine.operations, ['enter'], reason: '실패 뒤의 예약은 보내지 않는다');
    expect(_texts(tester), ['abxycd']);
    expect(_focused(tester), 1);
    expect(find.textContaining('합성 실패'), findsOneWidget);
    expect(find.textContaining('적용하지 않았습니다'), findsOneWidget);
  });

  testWidgets('a failure of a queued command keeps the later text', (
    tester,
  ) async {
    final engine = await _slowEnter(tester);
    await _type(tester, 'x');
    await _key(tester, LogicalKeyboardKey.enter);
    await _type(tester, 'y');
    await _key(tester, LogicalKeyboardKey.enter);
    await _type(tester, 'z');
    await _release(tester, engine);
    engine.failKey = true;
    await _release(tester, engine);
    expect(engine.operations, ['enter', 'enter']);
    expect(_texts(tester), ['ab', 'xyzcd']);
  });

  testWidgets('clicks cannot switch page or block while a command runs', (
    tester,
  ) async {
    final engine = await _slowEnter(tester, ['abcd', 'ef']);
    await _type(tester, 'x');
    await tester.tap(_field(2), warnIfMissed: false, kind: PointerDeviceKind.mouse);
    await tester.pump();
    expect(_focused(tester), 1);
    await _type(tester, 'y');
    // 처리 중에는 페이지 목록이 눌리지 않는다(바깥 클릭이라 포커스는 풀린다).
    await tester.tap(
      find.widgetWithText(ListTile, '다른').last,
      kind: PointerDeviceKind.mouse,
    );
    await tester.pump();
    await _release(tester, engine);
    expect(_focused(tester), 2);
    expect(engine.loads, ['합성']);
    expect(_texts(tester), ['ab', 'xycd', 'ef']);
    // 끝난 뒤에는 누를 수 있다.
    await tester.tap(_field(3));
    await tester.pump();
    expect(_focused(tester), 3);
  }, variant: TargetPlatformVariant.only(TargetPlatform.windows));

  testWidgets('focus moved to the search box keeps the typed text', (
    tester,
  ) async {
    final engine = await _slowEnter(tester);
    await _type(tester, 'x');
    await tester.tap(find.byType(TextField).first);
    await tester.pump();
    expect(_focused(tester), 0);
    await _release(tester, engine);
    expect(_texts(tester), ['ab', 'xcd']);
    expect(_focused(tester), 2);
  });

  testWidgets('failure after focus left the block still keeps the text', (
    tester,
  ) async {
    final engine = await _slowEnter(tester);
    await _type(tester, 'x');
    await tester.tap(find.byType(TextField).first);
    await tester.pump();
    engine.failKey = true;
    await _release(tester, engine);
    expect(_texts(tester), ['abxcd']);
  });

  testWidgets('Enter after clicking another block waits for its save', (
    tester,
  ) async {
    final engine = await _pump(tester, ['one', 'two']);
    await _focus(tester, 1);
    await _type(tester, '!');
    engine.slowKeys = true;
    await tester.tap(_field(2));
    await tester.pump();
    await tester.pump();
    expect(engine.operations, ['commit']);
    _editable(tester, 2).controller.selection = const TextSelection.collapsed(
      offset: 3,
    );
    await _type(tester, '?');
    await _key(tester, LogicalKeyboardKey.enter);
    await _type(tester, 'z');
    await _release(tester, engine);
    await _release(tester, engine);
    expect(engine.operations, ['commit', 'enter']);
    expect(_texts(tester), ['one!', 'two?', 'z']);
  });

  // ---------- OUTKEYS3: 처리 중의 Backspace·Delete ----------

  testWidgets('Backspace right after Enter merges the blocks again', (
    tester,
  ) async {
    final engine = await _slowEnter(tester);
    expect(await _key(tester, LogicalKeyboardKey.backspace), isTrue);
    expect(engine.operations, ['enter']);
    await _release(tester, engine);
    await _release(tester, engine);
    expect(engine.operations, ['enter', 'backspace']);
    expect(_texts(tester), ['abcd']);
    expect(_focused(tester), 1);
    expect(_editable(tester, 1).controller.selection.baseOffset, 2);
  });

  testWidgets('consecutive Backspace undoes consecutive Enter in order', (
    tester,
  ) async {
    final engine = await _slowEnter(tester);
    await _key(tester, LogicalKeyboardKey.enter);
    await _key(tester, LogicalKeyboardKey.backspace);
    await _key(tester, LogicalKeyboardKey.backspace);
    for (var i = 0; i < 4; i++) {
      await _release(tester, engine);
    }
    expect(engine.operations, ['enter', 'enter', 'backspace', 'backspace']);
    expect(_texts(tester), ['abcd']);
  });

  testWidgets('typed text is erased first, then the blocks are merged', (
    tester,
  ) async {
    final engine = await _slowEnter(tester);
    await _type(tester, 'x');
    await _key(tester, LogicalKeyboardKey.backspace); // 친 글
    await _key(tester, LogicalKeyboardKey.backspace); // 블록 합치기
    await _type(tester, 'z');
    await _release(tester, engine);
    await _release(tester, engine);
    expect(engine.operations, ['enter', 'backspace']);
    expect(_texts(tester), ['abzcd']);
    expect(_editable(tester, 1).controller.selection.baseOffset, 3);
  });

  testWidgets('Backspace is judged by the cursor of the result, not the press', (
    tester,
  ) async {
    // 누를 때는 블록 중간. Tab 결과의 커서도 중간이므로 글자를 지우고 합치지 않는다.
    final engine = await _pump(tester, ['ab', 'cdef']);
    await _focus(tester, 2, 2);
    engine.slowKeys = true;
    await _key(tester, LogicalKeyboardKey.tab);
    await _key(tester, LogicalKeyboardKey.backspace);
    await _key(tester, LogicalKeyboardKey.backspace);
    await _key(tester, LogicalKeyboardKey.backspace); // 이때는 맨 앞 → 합치기
    await _release(tester, engine);
    await _release(tester, engine);
    expect(engine.operations, ['indent', 'backspace']);
    expect(engine.keys[1]['text'], 'ef');
    expect(engine.keys[1]['start'], 0);
    expect(_texts(tester), ['abef']);
  });

  testWidgets('Delete erases the next character or merges the next block', (
    tester,
  ) async {
    final engine = await _pump(tester, ['ab', 'ef']);
    await _focus(tester, 1, 2);
    engine.slowKeys = true;
    await _key(tester, LogicalKeyboardKey.enter); // 빈 블록이 생긴다
    await _key(tester, LogicalKeyboardKey.delete); // 다음 블록 합치기
    await _key(tester, LogicalKeyboardKey.delete); // 합친 뒤에는 글자 지우기
    await _release(tester, engine);
    await _release(tester, engine);
    expect(engine.operations, ['enter', 'delete']);
    expect(_texts(tester), ['ab', 'f']);
    expect(_focused(tester), 2);
  });

  testWidgets('Delete after typing keeps the typed text before it', (
    tester,
  ) async {
    final engine = await _slowEnter(tester);
    await _type(tester, 'x');
    await _key(tester, LogicalKeyboardKey.delete);
    await _release(tester, engine);
    expect(engine.operations, ['enter']);
    expect(_texts(tester), ['ab', 'xd']);
    expect(_editable(tester, 2).controller.selection.baseOffset, 1);
  });

  testWidgets('Backspace keeps its place between Tab and Enter', (tester) async {
    final engine = await _slowEnter(tester);
    await _key(tester, LogicalKeyboardKey.tab);
    await _key(tester, LogicalKeyboardKey.backspace);
    await _key(tester, LogicalKeyboardKey.enter);
    for (var i = 0; i < 4; i++) {
      await _release(tester, engine);
    }
    expect(engine.operations, ['enter', 'indent', 'backspace', 'enter']);
    expect(_texts(tester), ['ab', 'cd']);
    expect(_focused(tester), 2);
  });

  testWidgets('Backspace during the save of Enter is applied after it', (
    tester,
  ) async {
    final engine = await _pump(tester, ['abcd']);
    await _focus(tester, 1, 2);
    engine.slowSaves = true;
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pumpAndSettle();
    await _key(tester, LogicalKeyboardKey.backspace);
    engine.slowSaves = false;
    await _release(tester, engine);
    expect(engine.operations, ['enter', 'backspace']);
    expect(_texts(tester), ['abcd']);
  });

  testWidgets('Backspace at the start of a clicked block waits for the save', (
    tester,
  ) async {
    final engine = await _pump(tester, ['one', 'two']);
    await _focus(tester, 1);
    await _type(tester, '!');
    engine.slowKeys = true;
    await tester.tap(_field(2));
    await tester.pump();
    await tester.pump();
    _editable(tester, 2).controller.selection = const TextSelection.collapsed(
      offset: 0,
    );
    await _key(tester, LogicalKeyboardKey.backspace);
    expect(engine.operations, ['commit']);
    await _release(tester, engine);
    await _release(tester, engine);
    expect(engine.operations, ['commit', 'backspace']);
    expect(_texts(tester), ['one!two']);
  });

  testWidgets('a held key does not merge several blocks', (tester) async {
    final engine = await _slowEnter(tester, ['ab', 'cd', 'ef'], 2);
    await tester.sendKeyDownEvent(LogicalKeyboardKey.backspace);
    for (var i = 0; i < 5; i++) {
      await tester.sendKeyRepeatEvent(LogicalKeyboardKey.backspace);
    }
    await _release(tester, engine);
    await tester.sendKeyRepeatEvent(LogicalKeyboardKey.backspace);
    await _release(tester, engine);
    // 처리가 끝난 뒤에도 누르고 있는 키는 맨 앞에서 합치지 않는다.
    _editable(tester, _focused(tester)).controller.selection =
        const TextSelection.collapsed(offset: 0);
    await tester.sendKeyRepeatEvent(LogicalKeyboardKey.backspace);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.backspace);
    await tester.pump();
    expect(engine.operations, ['enter', 'backspace']);
    expect(_texts(tester), ['ab', 'cd', 'ef']);
  });

  testWidgets('a held Delete does not merge several blocks', (tester) async {
    final engine = await _pump(tester, ['ab', 'cd', 'ef']);
    await _focus(tester, 1);
    await tester.sendKeyDownEvent(LogicalKeyboardKey.delete);
    await tester.pumpAndSettle();
    for (var i = 0; i < 3; i++) {
      _editable(tester, 1).controller.selection = TextSelection.collapsed(
        offset: _editable(tester, 1).controller.text.length,
      );
      await tester.sendKeyRepeatEvent(LogicalKeyboardKey.delete);
      await tester.pumpAndSettle();
    }
    await tester.sendKeyUpEvent(LogicalKeyboardKey.delete);
    expect(engine.operations, ['delete']);
    expect(_texts(tester), ['abcd', 'ef']);
  });

  testWidgets('a failed command drops the queued Backspace', (tester) async {
    final engine = await _slowEnter(tester);
    await _key(tester, LogicalKeyboardKey.backspace);
    engine.failKey = true;
    await _release(tester, engine);
    expect(engine.operations, ['enter']);
    expect(_texts(tester), ['abcd']);
    expect(find.textContaining('Backspace'), findsOneWidget);
    expect(find.textContaining('적용하지 않았습니다'), findsOneWidget);
  });

  testWidgets('page clicks are ignored while a Backspace is queued', (
    tester,
  ) async {
    final engine = await _slowEnter(tester);
    await _key(tester, LogicalKeyboardKey.backspace);
    await tester.tap(
      find.widgetWithText(ListTile, '다른').last,
      kind: PointerDeviceKind.mouse,
    );
    await tester.pump();
    await _release(tester, engine);
    await _release(tester, engine);
    expect(engine.loads, ['합성']);
    expect(engine.operations, ['enter', 'backspace']);
    expect(_texts(tester), ['abcd']);
  }, variant: TargetPlatformVariant.only(TargetPlatform.windows));

  testWidgets('without a cursor from the result Backspace is not applied', (
    tester,
  ) async {
    final engine = await _pump(tester, ['abcd']);
    await _focus(tester, 1, 2);
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pumpAndSettle();
    engine.slowParse = true;
    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.keyZ);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await tester.pump();
    await _key(tester, LogicalKeyboardKey.backspace);
    await _release(tester, engine);
    expect(engine.operations, ['enter']);
    expect(_texts(tester), ['abcd']);
    expect(find.textContaining('커서 위치를 알 수 없어'), findsOneWidget);
  });

  testWidgets('a character erased after the command is saved on leaving', (
    tester,
  ) async {
    final engine = await _slowEnter(tester);
    await _key(tester, LogicalKeyboardKey.delete);
    await _release(tester, engine);
    expect(_texts(tester), ['ab', 'd']);
    await tester.tap(find.byType(TextField).first);
    await tester.pump();
    expect(engine.operations, ['enter', 'commit']);
    await _release(tester, engine);
    expect(engine.texts, ['ab', 'd']);
    expect(_texts(tester), ['ab', 'd']);
  });

  test('Backspace and Delete inside a field', () {
    TextEditingValue at(String text, int start, [int? end]) => TextEditingValue(
      text: text,
      selection: TextSelection(baseOffset: start, extentOffset: end ?? start),
    );
    String? erase(TextEditingValue v, {bool forward = false}) {
      final r = eraseInField(v, forward: forward);
      return r == null ? null : '${r.text}@${r.selection.baseOffset}';
    }

    expect(erase(at('ab', 0)), isNull);
    expect(erase(at('ab', 2), forward: true), isNull);
    expect(erase(at('', 0)), isNull);
    expect(erase(at('가나', 2)), '가@1');
    expect(erase(at('a👨‍👩‍👧b', 9)), 'ab@1');
    expect(erase(at('a😀b', 1), forward: true), 'ab@1');
    expect(erase(at('x[]y', 2)), 'xy@1');
    expect(erase(at('abcd', 1, 3)), 'ad@1');
    expect(erase(at('abcd', 0, 2), forward: true), 'cd@0');
  });
}
