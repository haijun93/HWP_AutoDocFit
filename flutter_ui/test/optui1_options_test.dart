import 'package:docfit_ui/main.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'workspace_test.dart' show initialState;

/// OPTUI1: 카드 '고급 설정' 접기, 세부 작업 묶음 접기·전체 켜기/끄기, 같은 설정이 카드와 세부 작업에서
/// 항상 같은 값으로 보이는지(단일 원천) 확인한다.
const _tableKeys = ['table_style', 'precise_table'];

Map<String, dynamic> _allState() {
  final state = initialState();
  state['files'] = [
    {'name': '문서.hwp', 'folder': r'C:\문서'},
  ];
  state['mode'] = 'all';
  state['include_spacing'] = true;
  state['card_options'] = <String, Object?>{
    'all_exclude_tables': false,
    'all_exclude_pagefit': false,
    'all_exclude_linespacing': false,
    'all_cell_image_to_bg': false,
  };
  Map<String, Object?> stage(String key, String label, bool on) => {
    'key': key,
    'label': label,
    'example': '예: $label',
    'on': on,
    'default': on,
  };
  state['stages'] = [
    stage('style_unify', '서식통일', true),
    stage('table_style', '기본 표 서식', true),
    stage('precise_table', '일반 표 정밀 서식 복제', true),
    stage('page_fit', '문단 아래 간격 페이지 맞춤', true),
  ];
  return state;
}

/// 앱의 연동 규칙(표 제외 ↔ 표 관련 세부 작업)을 흉내 내는 가짜 백엔드. 값은 stages 한 곳에서만 나온다.
class _FakeBackend {
  _FakeBackend(this.state);
  final Map<String, dynamic> state;
  final calls = <List<Object?>>[];

  Map get _options => state['card_options'] as Map;
  List get _stages => state['stages'] as List;

  void _sync() {
    final anyTable = _stages.any(
      (s) => _tableKeys.contains(s['key']) && s['on'] == true,
    );
    state['exclude_tables'] = !anyTable;
    _options['all_exclude_tables'] = !anyTable;
  }

  Future<Object?> call(String name, List<Object?> args) async {
    if (name != 'get_state') calls.add([name, ...args]);
    if (name == 'set_stage') {
      (_stages.firstWhere((s) => s['key'] == args[0]) as Map)['on'] = args[1];
      _sync();
    } else if (name == 'set_exclude_tables') {
      for (final s in _stages) {
        if (_tableKeys.contains(s['key'])) s['on'] = args.first != true;
      }
      _sync();
    } else if (name == 'set_card_option') {
      _options[args[0] as String] = args[1];
    } else if (name == 'set_mode') {
      state['mode'] = args.first == 'format' ? 'all' : args.first;
    }
    return Map<String, dynamic>.from(state);
  }
}

Future<_FakeBackend> _open(WidgetTester tester, Map<String, dynamic> state) async {
  tester.view.physicalSize = const Size(1280, 2400);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
  final backend = _FakeBackend(state).._sync();
  await tester.pumpWidget(DocFitApp(api: backend.call));
  await tester.pumpAndSettle();
  await tester.tap(find.text('작업 방식 선택'));
  await tester.pumpAndSettle();
  return backend;
}

bool _switchValue(WidgetTester tester, String key) =>
    tester.widget<Switch>(find.byKey(Key(key))).value;

void main() {
  testWidgets('card shows only core options; advanced options are folded', (
    tester,
  ) async {
    await _open(tester, _allState());
    expect(find.byKey(const Key('includeSpacingSwitch')), findsOneWidget);
    expect(find.byKey(const Key('excludeTablesSwitch')), findsOneWidget);
    for (final key in [
      'allCellImageToBgSwitch',
      'allLinespacingSwitch',
      'allExcludePagefitSwitch',
    ]) {
      expect(find.byKey(Key(key)), findsNothing, reason: key);
    }
    await tester.tap(find.byKey(const Key('advancedOptionsTitle-all')));
    await tester.pumpAndSettle();
    for (final key in [
      'allCellImageToBgSwitch',
      'allLinespacingSwitch',
      'allExcludePagefitSwitch',
    ]) {
      expect(find.byKey(Key(key)), findsOneWidget, reason: key);
    }
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('advanced fold remembers its state and counts options kept on', (
    tester,
  ) async {
    final state = _allState();
    (state['card_options'] as Map)['all_exclude_linespacing'] = true;
    await _open(tester, state);
    // 접혀 있어도 켜 둔 개수를 제목에 보여 준다.
    expect(find.text('고급 설정 · 1개 켜 둠'), findsOneWidget);
    await tester.tap(find.byKey(const Key('advancedOptionsTitle-all')));
    await tester.pumpAndSettle();
    expect(find.byKey(const Key('allLinespacingSwitch')), findsOneWidget);
    // 다른 카드를 골랐다가 돌아와도 펼침 상태를 기억한다.
    await tester.tap(find.text('자간 정리').first);
    await tester.pumpAndSettle();
    await tester.tap(find.text('한 번에 적용').first);
    await tester.pumpAndSettle();
    expect(find.byKey(const Key('allLinespacingSwitch')), findsOneWidget);
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('keyboard can reach and toggle the advanced fold', (tester) async {
    await _open(tester, _allState());
    final title = find.byKey(const Key('advancedOptionsTitle-all'));
    var reached = false;
    for (var i = 0; i < 80 && !reached; i++) {
      await tester.sendKeyEvent(LogicalKeyboardKey.tab);
      await tester.pump();
      final focus = FocusManager.instance.primaryFocus?.context;
      final tiles = find
          .ancestor(of: title, matching: find.byType(ListTile))
          .evaluate();
      if (focus == null || tiles.isEmpty) continue;
      final tile = tiles.first;
      if (identical(focus, tile)) reached = true;
      focus.visitAncestorElements((a) {
        if (identical(a, tile)) reached = true;
        return !reached;
      });
    }
    expect(reached, isTrue, reason: '키보드 Tab으로 고급 설정 제목에 닿아야 한다');
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pumpAndSettle();
    expect(find.byKey(const Key('allLinespacingSwitch')), findsOneWidget);
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('stage groups fold and show used counts', (tester) async {
    await _open(tester, _allState());
    expect(find.byKey(const Key('stageSummary')), findsOneWidget);
    expect(find.text('4개 중 4개 사용'), findsOneWidget);
    // 접혀 있으면 단계 카드는 보이지 않고 묶음 제목만 보인다.
    expect(find.text('3. 일반 표 정밀 서식 복제'), findsNothing);
    expect(find.text('표  2/2'), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('stage-section-title-table')));
    await tester.pumpAndSettle();
    expect(find.text('3. 일반 표 정밀 서식 복제'), findsOneWidget);
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('group on/off sends only the changed stages and card follows', (
    tester,
  ) async {
    final backend = await _open(tester, _allState());
    await tester.tap(find.byKey(const ValueKey('stage-section-title-table')));
    await tester.pumpAndSettle();
    // 이미 모두 켜져 있으면 '묶음 전체 켜기'는 눌리지 않는다.
    expect(
      tester
          .widget<TextButton>(
            find.byKey(const ValueKey('stage-section-on-table')),
          )
          .onPressed,
      isNull,
    );
    expect(_switchValue(tester, 'excludeTablesSwitch'), isFalse);
    await tester.tap(find.byKey(const ValueKey('stage-section-off-table')));
    await tester.pumpAndSettle();
    final sets = backend.calls.where((c) => c.first == 'set_stage').toList();
    expect(sets, [
      ['set_stage', 'table_style', false],
      ['set_stage', 'precise_table', false],
    ]);
    // 단일 원천: 카드의 '표 제외'가 세부 작업과 같이 바뀌고 요약 개수도 맞다.
    expect(_switchValue(tester, 'excludeTablesSwitch'), isTrue);
    expect(find.text('4개 중 2개 사용'), findsOneWidget);
    expect(find.text('표  0/2'), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('stage-section-on-table')));
    await tester.pumpAndSettle();
    expect(_switchValue(tester, 'excludeTablesSwitch'), isFalse);
    expect(find.text('4개 중 4개 사용'), findsOneWidget);
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('card option and stage switches always show the same value', (
    tester,
  ) async {
    await _open(tester, _allState());
    await tester.tap(find.byKey(const ValueKey('stage-section-title-table')));
    await tester.pumpAndSettle();
    bool stageOn(String key) =>
        tester.widget<Switch>(find.byKey(ValueKey('stage-switch-$key'))).value;
    // 카드에서 바꾸면 세부 작업이 따라 바뀐다.
    await tester.tap(find.byKey(const Key('excludeTablesSwitch')));
    await tester.pumpAndSettle();
    expect(_switchValue(tester, 'excludeTablesSwitch'), isTrue);
    expect([for (final k in _tableKeys) stageOn(k)], [false, false]);
    expect(find.text('표  0/2'), findsOneWidget);
    // 세부 작업 하나를 켜면 카드가 따라 바뀐다.
    await tester.tap(find.byKey(const ValueKey('stage-switch-table_style')));
    await tester.pumpAndSettle();
    expect(_switchValue(tester, 'excludeTablesSwitch'), isFalse);
    expect(stageOn('table_style'), isTrue);
    expect(find.text('표  1/2'), findsOneWidget);
    expect(find.text('4개 중 3개 사용'), findsOneWidget);
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox());
  });
}
