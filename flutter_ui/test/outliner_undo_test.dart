import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:docfit_ui/features/outliner/outliner_history.dart';
import 'package:docfit_ui/features/outliner/outliner_workspace.dart';

class _MemoryRecent implements RecentPagesStore {
  @override
  Future<List<String>> load() async => [];
  @override
  Future<void> save(List<String> pages) async {}
}

/// 마크다운 한 줄 = 블록 하나인 가짜 엔진(블록 번호는 매번 1.. 로 새로 매긴다. 실제 엔진과 같다).
class _FakeEngine {
  _FakeEngine(Map<String, String> pages) : pages = Map.of(pages);
  final Map<String, String> pages;
  final List<String> log = [];
  final List<Map> saves = [];
  String conflictPage = '';

  static List<String> _lines(String md) =>
      md.split('\n').where((l) => l.isNotEmpty).map((l) => l.substring(2)).toList();
  static String _md(List<String> lines) => lines.map((l) => '- $l\n').join();
  static Map doc(String md) => {
    'blocks': [
      for (final (i, t) in _lines(md).indexed)
        {'id': i + 1, 'text': t, 'indent': 0, 'folded': false},
    ],
  };

  Future<Object?> call(String method, [List<Object?> args = const []]) async {
    final p = Map<String, Object?>.from(args.first as Map);
    final page = (p['page'] as String?) ?? pages.keys.first;
    log.add('${p['action']}${p['operation'] != null ? ':${p['operation']}' : ''}');
    switch (p['action']) {
      case 'list':
        return {'ok': true, 'data': {'pages': [for (final n in pages.keys) {'name': n}]}};
      case 'daily':
      case 'load':
        final name = p['action'] == 'daily' ? pages.keys.first : page;
        return {'ok': true, 'data': {
          'page': name, 'markdown': pages[name], 'version': 'v0', 'document': doc(pages[name]!),
        }};
      case 'backlinks':
        return {'ok': true, 'data': {'block_ids': []}};
      case 'parse':
        return {'ok': true, 'data': doc(p['markdown'] as String)};
      case 'edit':
        final lines = _lines(p['markdown'] as String);
        final index = (p['block_id'] as int) - 1;
        switch (p['operation']) {
          case 'set_text':
            lines[index] = p['text'] as String;
          case 'insert':
            lines.insert(index + 1, (p['text'] as String?) ?? '');
          case 'delete':
            lines.removeAt(index);
        }
        final md = _md(lines);
        return {'ok': true, 'data': {'markdown': md, 'document': doc(md)}};
      case 'save':
        if (conflictPage == page) {
          return {'ok': false, 'error': {'message': '페이지가 다른 곳에서 변경되었습니다.'}};
        }
        pages[page] = p['markdown'] as String;
        saves.add(p);
        return {'ok': true, 'data': {'version': 'v${saves.length}'}};
    }
    return {'ok': true, 'data': {}};
  }
}

Future<void> _pump(WidgetTester tester, _FakeEngine engine) async {
  tester.view.physicalSize = const Size(1400, 900);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  await tester.pumpWidget(MaterialApp(
    home: Scaffold(body: OutlinerWorkspace(api: engine.call, recentPagesStore: _MemoryRecent())),
  ));
  await tester.pumpAndSettle();
}

Future<void> _ctrl(WidgetTester tester, LogicalKeyboardKey key, {bool shift = false}) async {
  await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
  if (shift) await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
  await tester.sendKeyEvent(key);
  if (shift) await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
  await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
  await tester.pump(const Duration(milliseconds: 100));
  await tester.pump(const Duration(milliseconds: 100));
}

Finder _field(int id) => find.byKey(ValueKey('outliner-block-$id'));
bool _enabled(WidgetTester tester, String key) =>
    tester.widget<IconButton>(find.byKey(ValueKey(key))).onPressed != null;

void main() {
  group('OutlinerHistory', () {
    test('undo/redo N steps and a new edit clears redo', () {
      final h = OutlinerHistory();
      var current = 'a';
      for (final next in ['b', 'c', 'd']) {
        h.record(current);
        current = next;
      }
      expect(h.undoCount, 3);
      current = h.undo(current)!;
      current = h.undo(current)!;
      expect(current, 'b');
      expect(h.canRedo, isTrue);
      current = h.redo(current)!;
      expect(current, 'c');
      h.record(current);
      expect(h.canRedo, isFalse);
    });

    test('depth limit 200 drops the oldest and byte limit is honoured', () {
      final h = OutlinerHistory();
      for (var i = 0; i < 250; i++) {
        h.record('- $i\n');
      }
      expect(h.undoCount, 200);
      expect(h.undo('now'), '- 249\n');
      final small = OutlinerHistory(maxChars: 100);
      for (var i = 0; i < 5; i++) {
        small.record('가' * 30);
      }
      expect(small.chars <= 100, isTrue);
      expect(small.undoCount, 3);
      expect(small.record('x' * 101), isFalse);
      expect(small.canUndo, isFalse);
    });

    test('clear empties both stacks', () {
      final h = OutlinerHistory()..record('a');
      h.undo('b');
      h.clear();
      expect(h.canUndo || h.canRedo, isFalse);
    });
  });

  testWidgets('Ctrl+Z undoes Enter (commit + insert) as one step and Ctrl+Y redoes it', (tester) async {
    final engine = _FakeEngine({'홈': '- 첫 항목\n'});
    await _pump(tester, engine);
    expect(_enabled(tester, 'outliner-undo'), isFalse);
    expect(_enabled(tester, 'outliner-redo'), isFalse);

    await tester.tap(_field(1));
    await tester.enterText(_field(1), '한글 수정 닭 앎');
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pump(const Duration(milliseconds: 100));
    await tester.pump(const Duration(milliseconds: 100));
    expect(engine.pages['홈'], '- 한글 수정 닭 앎\n- \n');
    expect(_field(2), findsOneWidget);
    expect(_enabled(tester, 'outliner-undo'), isTrue);

    await _ctrl(tester, LogicalKeyboardKey.keyZ);
    expect(engine.pages['홈'], '- 첫 항목\n', reason: 'set_text + insert 가 한 단계로 되돌려진다');
    expect(_field(2), findsNothing);
    expect(
      tester.widget<EditableText>(find.descendant(of: _field(1), matching: find.byType(EditableText))).controller.text,
      '첫 항목',
    );
    expect(_enabled(tester, 'outliner-undo'), isFalse);
    expect(_enabled(tester, 'outliner-redo'), isTrue);

    await _ctrl(tester, LogicalKeyboardKey.keyY);
    expect(engine.pages['홈'], '- 한글 수정 닭 앎\n- \n');
    expect(_field(2), findsOneWidget);
    expect(_enabled(tester, 'outliner-redo'), isFalse);

    // Ctrl+Shift+Z 도 다시 실행이다.
    await _ctrl(tester, LogicalKeyboardKey.keyZ);
    expect(engine.pages['홈'], '- 첫 항목\n');
    await _ctrl(tester, LogicalKeyboardKey.keyZ, shift: true);
    expect(engine.pages['홈'], '- 한글 수정 닭 앎\n- \n');
  });

  testWidgets('with unconfirmed typing the field keeps its own text undo and the document history is untouched', (tester) async {
    final engine = _FakeEngine({'홈': '- 하나\n'});
    await _pump(tester, engine);
    await tester.tap(_field(1));
    await tester.enterText(_field(1), '하나 둘');
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pump(const Duration(milliseconds: 200));
    expect(engine.pages['홈'], '- 하나 둘\n- \n');

    // 새 줄에 확정하지 않은 글을 쓴 뒤 Ctrl+Z: 문서는 그대로(칸의 글자 되돌리기에 맡긴다).
    await tester.enterText(_field(2), '미확정 입력');
    await tester.pump();
    final savesBefore = engine.saves.length;
    await _ctrl(tester, LogicalKeyboardKey.keyZ);
    expect(engine.saves.length, savesBefore);
    expect(engine.log.where((e) => e == 'parse'), isEmpty, reason: '문서 되돌리기는 일어나지 않았다');
    expect(engine.pages['홈'], '- 하나 둘\n- \n');
    // 단추는 확정하지 않은 입력만 버리고 문서는 건드리지 않는다.
    await tester.tap(find.byKey(const ValueKey('outliner-undo')));
    await tester.pump();
    expect(engine.saves.length, savesBefore);
    expect(
      tester.widget<EditableText>(find.descendant(of: _field(2), matching: find.byType(EditableText))).controller.text,
      '',
    );
    // 이제 확정하지 않은 글이 없으므로 같은 단추가 문서를 되돌린다.
    await tester.tap(find.byKey(const ValueKey('outliner-undo')));
    await tester.pump(const Duration(milliseconds: 100));
    await tester.pump(const Duration(milliseconds: 100));
    expect(engine.pages['홈'], '- 하나\n');
  });

  testWidgets('a new edit clears redo and switching pages clears the history', (tester) async {
    final engine = _FakeEngine({'가': '- a\n', '나': '- b\n'});
    await _pump(tester, engine);
    await tester.tap(_field(1));
    await tester.enterText(_field(1), 'a1');
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pump(const Duration(milliseconds: 200));
    await _ctrl(tester, LogicalKeyboardKey.keyZ);
    expect(_enabled(tester, 'outliner-redo'), isTrue);

    await tester.tap(_field(1));
    await tester.enterText(_field(1), 'a2');
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pump(const Duration(milliseconds: 200));
    expect(_enabled(tester, 'outliner-redo'), isFalse, reason: '새 편집은 다시 실행을 비운다');
    expect(_enabled(tester, 'outliner-undo'), isTrue);

    await tester.tap(find.widgetWithText(ListTile, '나').last);
    await tester.pumpAndSettle();
    expect(_enabled(tester, 'outliner-undo'), isFalse, reason: '페이지를 바꾸면 이력을 비운다');
    expect(_enabled(tester, 'outliner-redo'), isFalse);
  });

  testWidgets('a rejected save (external change) clears the history', (tester) async {
    final engine = _FakeEngine({'홈': '- x\n'});
    await _pump(tester, engine);
    await tester.tap(_field(1));
    await tester.enterText(_field(1), 'x1');
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pump(const Duration(milliseconds: 200));
    expect(_enabled(tester, 'outliner-undo'), isTrue);
    engine.conflictPage = '홈';
    await tester.tap(_field(2));
    await tester.enterText(_field(2), 'y');
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pump(const Duration(milliseconds: 200));
    expect(_enabled(tester, 'outliner-undo'), isFalse);
  });
}
