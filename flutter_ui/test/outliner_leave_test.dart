import 'dart:async';
import 'dart:ui';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:docfit_ui/features/outliner/outliner_ui_gate.dart';
import 'package:docfit_ui/features/outliner/outliner_workspace.dart';

// OUTKEYS3: 확정 전 글이 있는 채 입력 칸 바깥을 마우스로 눌러도 글이 사라지지 않는지.

class _Recent implements RecentPagesStore {
  @override
  Future<List<String>> load() async => ['다른'];
  @override
  Future<void> save(List<String> pages) async {}
}

class _Panels implements OutlinerPanelStore {
  @override
  Future<Map<String, bool>> load() async => {};
  @override
  Future<void> save(Map<String, bool> collapsed) async {}
}

class _Engine {
  final Map<String, List<String>> pages = {
    '합성': ['one', 'two'],
    '다른': ['x'],
  };
  String page = '합성';

  /// 요청 순서. 저장은 `save:페이지:글|글`.
  final log = <String>[];
  bool failKey = false;
  Completer<void>? holdKey;

  String _markdown(List<String> texts) => texts.map((t) => '- $t\n').join();
  List<String> _parse(Object? markdown) => [
    for (final line in (markdown as String).split('\n'))
      if (line.isNotEmpty) line.substring(2),
  ];
  Map<String, Object?> _document(List<String> texts) => {
    'blocks': [
      for (final (i, t) in texts.indexed)
        {
          'id': i + 1,
          'text': t,
          'indent': 0,
          'folded': false,
          'page_links': [if (i == 1) '다른'],
        },
    ],
  };
  Map<String, Object?> _page(String name) => {
    'page': name,
    'markdown': _markdown(pages[name]!),
    'version': 'v0',
    'document': _document(pages[name]!),
  };

  Future<Object?> call(String method, [List<Object?> args = const []]) async {
    final p = Map<String, Object?>.from(args.first as Map);
    Map<String, Object?> data = {};
    switch (p['action']) {
      case 'ui_mode':
        data = {'mode': 'flutter'};
      case 'open_native':
        log.add('open_native');
      case 'list':
        data = {
          'pages': [for (final name in pages.keys) {'name': name}],
        };
      case 'daily':
        log.add('daily');
        data = _page(page = '합성');
      case 'load':
        log.add('load:${p['page']}');
        data = _page(page = p['page'] as String);
      case 'backlinks':
        data = {'block_ids': []};
      case 'search':
        data = {'block_ids': [1, 2]};
      case 'save':
        pages[p['page'] as String] = _parse(p['markdown']);
        log.add('save:${p['page']}:${pages[p['page']]!.join('|')}');
        data = {'version': 'v1'};
      case 'key':
        log.add('key:${p['operation']}');
        await holdKey?.future;
        if (failKey) {
          return {'ok': false, 'error': {'message': '합성 실패'}};
        }
        final texts = _parse(p['markdown']);
        final i = p['block'] as int;
        texts[i] = p['text'] as String;
        data = {
          'handled': true,
          'markdown': _markdown(texts),
          'document': _document(texts),
          'selection': const <int>[],
          'cursor': {'block': i, 'start': p['start'], 'end': p['end']},
        };
      case 'edit':
        log.add('edit:${p['operation']}');
        final texts = _parse(p['markdown']);
        if (p['operation'] == 'insert') texts.add('${p['text'] ?? ''}');
        if (p['operation'] == 'set_text') {
          texts[(p['block_id'] as int) - 1] = p['text'] as String;
        }
        data = {'markdown': _markdown(texts), 'document': _document(texts)};
    }
    return {'ok': true, 'data': data};
  }
}

void _view(WidgetTester tester) {
  tester.view.physicalSize = const Size(1400, 900);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
}

Future<_Engine> _pump(WidgetTester tester) async {
  _view(tester);
  final engine = _Engine();
  await tester.pumpWidget(
    MaterialApp(
      home: Scaffold(
        body: OutlinerWorkspace(
          api: engine.call,
          recentPagesStore: _Recent(),
          panelStore: _Panels(),
        ),
      ),
    ),
  );
  await tester.pumpAndSettle();
  return engine;
}

Finder _field(int id) => find.byKey(ValueKey('outliner-block-$id'));

EditableText _editable(WidgetTester tester, int id) => tester.widget(
  find.descendant(of: _field(id), matching: find.byType(EditableText)),
);

String _text(WidgetTester tester, int id) =>
    _editable(tester, id).controller.text;

Future<void> _click(WidgetTester tester, Finder finder) async {
  await tester.tap(finder, kind: PointerDeviceKind.mouse);
  await tester.pumpAndSettle();
}

/// 첫 블록에 확정하지 않은 글을 둔다.
Future<_Engine> _typed(WidgetTester tester, [String text = 'one!']) async {
  final engine = await _pump(tester);
  await _click(tester, _field(1));
  await tester.enterText(_field(1), text);
  await tester.pump();
  engine.log.clear();
  return engine;
}

void _expectSaved(WidgetTester tester, _Engine engine) {
  expect(engine.pages['합성']!.first, 'one!', reason: '${engine.log}');
}

final _windows = TargetPlatformVariant.only(TargetPlatform.windows);

void main() {
  final blank = <String, Offset Function(WidgetTester)>{
    '본문 빈 곳': (t) => t.getBottomLeft(find.byKey(const ValueKey('outliner-body'))) + const Offset(100, -40),
    '제목': (t) => t.getTopLeft(find.byKey(const ValueKey('outliner-body'))) + const Offset(40, 30),
    '왼쪽 패널 빈 곳': (t) => const Offset(100, 860),
    '오른쪽 패널 빈 곳': (t) => const Offset(1290, 860),
    '화면 밖 단추 줄의 도움말 아이콘': (t) => t.getCenter(find.byIcon(Icons.keyboard_outlined)),
  };
  for (final MapEntry(:key, :value) in blank.entries) {
    testWidgets('$key 을 눌러도 글이 남고 저장된다', (tester) async {
      final engine = await _typed(tester);
      await tester.tapAt(value(tester), kind: PointerDeviceKind.mouse);
      await tester.pumpAndSettle();
      expect(_text(tester, 1), 'one!');
      _expectSaved(tester, engine);
      expect(engine.log.where((l) => l.startsWith('key:')), ['key:commit']);
    }, variant: _windows);
  }

  testWidgets('검색 칸을 눌러도 글이 남고 저장된다', (tester) async {
    final engine = await _typed(tester);
    await _click(tester, find.widgetWithText(TextField, '페이지·블록 검색'));
    expect(_text(tester, 1), 'one!');
    _expectSaved(tester, engine);
  }, variant: _windows);

  final moves = <String, (Finder, String)>{
    '페이지 목록': (find.widgetWithText(ListTile, '다른').last, 'load:다른'),
    '최근 페이지': (find.widgetWithText(ListTile, '다른').first, 'load:다른'),
    '오늘의 일지': (find.widgetWithText(ListTile, '오늘의 일지'), 'daily'),
    '[[링크]] 칩': (find.widgetWithText(ActionChip, '[[다른]]'), 'load:다른'),
  };
  for (final MapEntry(:key, value: (finder, request)) in moves.entries) {
    testWidgets('$key 으로 옮기면 저장한 뒤 옮긴다', (tester) async {
      final engine = await _typed(tester);
      await _click(tester, finder);
      _expectSaved(tester, engine);
      expect(engine.log, contains(request));
      expect(
        engine.log.indexWhere((l) => l.startsWith('save:합성:one!')),
        lessThan(engine.log.indexOf(request)),
        reason: '${engine.log}',
      );
    }, variant: _windows);
  }

  testWidgets('저장이 느려도 누른 이동은 저장 뒤에 한 번만 한다', (tester) async {
    final engine = await _typed(tester);
    engine.holdKey = Completer<void>();
    final tile = find.widgetWithText(ListTile, '다른').last;
    final gesture = await tester.startGesture(
      tester.getCenter(tile),
      kind: PointerDeviceKind.mouse,
    );
    await tester.pump();
    await tester.pump(); // 누르고 있는 동안 저장이 시작돼 다시 그려진다.
    await gesture.up();
    await tester.pump();
    expect(engine.log, ['key:commit']);
    engine.holdKey!.complete();
    await tester.pumpAndSettle();
    expect(engine.log, ['key:commit', 'save:합성:one!|two', 'load:다른']);
    expect(_text(tester, 1), 'x');
  }, variant: _windows);

  testWidgets('저장에 실패하면 옮기지 않고 글을 남긴다', (tester) async {
    final engine = await _typed(tester);
    engine.failKey = true;
    await _click(tester, find.widgetWithText(ListTile, '다른').last);
    expect(_text(tester, 1), 'one!');
    expect(engine.log.where((l) => l.startsWith('load')), isEmpty);
    expect(find.textContaining('합성 실패'), findsOneWidget);
    // 빈 곳을 눌러도 남는다.
    await tester.tapAt(const Offset(100, 860), kind: PointerDeviceKind.mouse);
    await tester.pumpAndSettle();
    expect(_text(tester, 1), 'one!');
  }, variant: _windows);

  testWidgets('저장 못 한 글이 있으면 다른 블록의 Enter 가 덮지 않는다', (tester) async {
    final engine = await _typed(tester);
    engine.failKey = true;
    await _click(tester, _field(2));
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pumpAndSettle();
    expect(_text(tester, 1), 'one!');
    expect(engine.log, isNot(contains('key:enter')));
  }, variant: _windows);

  testWidgets('새 블록 단추는 글을 저장한 뒤 넣는다', (tester) async {
    final engine = await _typed(tester);
    await _click(tester, find.byTooltip('새 블록'));
    expect(engine.pages['합성'], ['one!', 'two', ''], reason: '${engine.log}');
  }, variant: _windows);

  for (final (name, finder) in [
    ('다른 블록의 들여쓰기 단추', () => find.byTooltip('들여쓰기').last),
    ('다른 블록의 접기 단추', () => find.byTooltip('접기/펼치기').last),
    ('다른 블록의 완료 단추', () => find.byTooltip('완료 표시').last),
  ]) {
    testWidgets('$name 를 눌러도 글이 남는다', (tester) async {
      final engine = await _typed(tester);
      await _click(tester, finder());
      expect(_text(tester, 1), 'one!');
      _expectSaved(tester, engine);
      expect(engine.log.length, greaterThan(2), reason: '${engine.log}');
    }, variant: _windows);
  }

  testWidgets('[[ 후보를 눌러 고르면 글이 남는다', (tester) async {
    await _typed(tester, 'a [[]]');
    _editable(tester, 1).controller.selection = const TextSelection.collapsed(
      offset: 4,
    );
    tester.widget<TextField>(_field(1)).onChanged!('');
    await tester.pump();
    await _click(tester, find.widgetWithText(ListTile, '합성').last);
    expect(_text(tester, 1), 'a [[합성]]');
  }, variant: _windows);

  testWidgets('실행 취소 단추는 확정 전 글만 되돌린다', (tester) async {
    final engine = await _typed(tester);
    await _click(tester, find.byKey(const ValueKey('outliner-undo')));
    expect(_text(tester, 1), 'one');
    expect(engine.log, isEmpty);
  }, variant: _windows);

  testWidgets('Esc 는 저장한 뒤 블록을 선택한다', (tester) async {
    final engine = await _typed(tester);
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pumpAndSettle();
    _expectSaved(tester, engine);
    expect(find.text('1개 선택'), findsOneWidget);
  }, variant: _windows);

  testWidgets('새 창으로 열기 전에 글을 저장한다', (tester) async {
    _view(tester);
    final engine = _Engine();
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: OutlinerUiGate(
            api: engine.call,
            recentPagesStore: _Recent(),
            panelStore: _Panels(),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    await _click(tester, _field(1));
    await tester.enterText(_field(1), 'one!');
    await tester.pump();
    await _click(tester, find.byKey(const ValueKey('outliner-open-native')));
    _expectSaved(tester, engine);
  }, variant: _windows);
}
