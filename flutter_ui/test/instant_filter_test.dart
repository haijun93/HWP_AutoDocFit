import 'package:docfit_ui/core/instant_search.dart';
import 'package:docfit_ui/features/abbreviations/abbreviation_manager.dart';
import 'package:docfit_ui/features/settings/idiom_panel.dart';
import 'package:docfit_ui/features/settings/settings_dialog.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

/// ISUI3(사용자 결정 2026-11-12): 목록 거르기 검색창 4곳(표 단위·준말본말·상용구·설정)도
/// 2글자부터 Enter 없이 실시간, 1글자는 Enter로 거르고, 1글자만 입력하면 거르지 않고 전체 목록.
/// 표 단위 화면 시험은 table_units_screen_test.dart에 있다.
Map<String, Object?> _ok(Map data) => {'api_version': 1, 'ok': true, 'data': data, 'error': null};

void main() {
  testWidgets('InstantFilterController: 1글자는 거르지 않고, Enter·2글자부터 거른다', (tester) async {
    final c = InstantFilterController();
    addTearDown(c.dispose);
    expect(c.delay, Duration.zero);
    expect(c.filterQuery, '');

    c.text.text = '회';
    await tester.pump();
    expect(c.phase, InstantSearchPhase.empty);
    expect(c.filterQuery, '');

    c.submit(); // Enter: 1글자도 거른다
    await tester.pump();
    expect(c.phase, InstantSearchPhase.results);
    expect(c.filterQuery, '회');

    c.text.text = '회의';
    await tester.pump(); // 지연 0: 타이머를 기다리지 않는다
    expect(c.filterQuery, '회의');
    expect(c.lastRun?.trigger, 'input');

    c.text.text = '회'; // 다시 1글자: 전체 목록
    await tester.pump();
    expect(c.filterQuery, '');

    c.text.text = '  회의  ';
    await tester.pump();
    expect(c.filterQuery, '회의');

    c.text.clear();
    await tester.pump();
    expect(c.filterQuery, '');
    expect(c.phase, InstantSearchPhase.empty);
  });

  testWidgets('준말·본말 검색: 1글자는 전체, Enter·2글자는 거르기', (tester) async {
    Future<Object?> api(String name, List<Object?> args) async => _ok({
          'entries': [
            {'key': '요약', 'type': 'text', 'value': '원래 문구', 'label': '원래 문구', 'builtin': false},
            {'key': '제목1', 'type': 'format', 'value': 'title1', 'label': '제목 서식1', 'builtin': true},
          ],
          'format_kinds': {'title1': '제목 서식1'},
          'use_defaults': true,
          'running': false,
        });
    await tester.pumpWidget(MaterialApp(home: Scaffold(body: AbbreviationManager(api: api))));
    await tester.pumpAndSettle();
    final field = find.byKey(const Key('abbreviationSearchField'));

    await tester.enterText(field, '요');
    await tester.pump();
    expect(find.text('요약'), findsOneWidget);
    expect(find.text('제목1 · 기본'), findsOneWidget); // 1글자는 거르지 않는다

    await tester.testTextInput.receiveAction(TextInputAction.done); // Enter
    await tester.pump();
    expect(find.text('요약'), findsOneWidget);
    expect(find.text('제목1 · 기본'), findsNothing);

    await tester.enterText(field, '제목');
    await tester.pump();
    expect(find.text('요약'), findsNothing);
    expect(find.text('제목1 · 기본'), findsOneWidget);

    await tester.enterText(field, '제');
    await tester.pump();
    expect(find.text('요약'), findsOneWidget);
    expect(find.text('제목1 · 기본'), findsOneWidget);
  });

  testWidgets('상용구 찾기: 1글자는 전체, Enter·2글자는 거르기', (tester) async {
    tester.view.physicalSize = const Size(1000, 900);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    Future<Object?> api(String method, List<Object?> args) async => _ok({
          'available': true,
          'text': [
            {'key': 'ㅁ', 'value': '□', 'attr': 0},
            {'key': 'ㅇ123', 'value': '①②③', 'attr': 22},
          ],
          'body': [],
        });
    await tester.pumpWidget(MaterialApp(home: Scaffold(body: IdiomPanel(api: api, body: false, locked: false))));
    await tester.pumpAndSettle();
    final field = find.byKey(const Key('idiom-search-false'));

    await tester.enterText(field, 'ㅇ');
    await tester.pump();
    expect(find.text('□'), findsOneWidget);
    expect(find.text('①②③'), findsOneWidget);

    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pump();
    expect(find.text('□'), findsNothing);
    expect(find.text('①②③'), findsOneWidget);

    await tester.enterText(field, 'ㅁ □'); // 거르기 대상은 '준말 본말'
    await tester.pump();
    expect(find.text('□'), findsOneWidget);
    expect(find.text('①②③'), findsNothing);

    await tester.enterText(field, '');
    await tester.pump();
    expect(find.text('□'), findsOneWidget);
    expect(find.text('①②③'), findsOneWidget);
  });

  testWidgets('설정 검색: 1글자는 지금 분류 그대로, Enter·2글자는 전체 설정에서 찾기', (tester) async {
    tester.view.physicalSize = const Size(1280, 900);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    Future<Object?> api(String method, List<Object?> args) async => _ok({
          'revision': 'r1',
          'running': false,
          'fields': [
            {'key': 'std_hanging_indent', 'label': '내어쓰기', 'type': 'boolean', 'default': true, 'group': '표준서식'},
            {'key': 'check_updates_on_start', 'label': '시작할 때 업데이트 확인', 'type': 'boolean', 'default': true, 'group': '네트워크'},
          ],
          'values': {'std_hanging_indent': true, 'check_updates_on_start': true},
        });
    await tester.pumpWidget(MaterialApp(home: Scaffold(body: SettingsDialog(api: api))));
    await tester.pumpAndSettle();
    final field = find.byKey(const Key('settingsSearchField'));
    expect(find.text('시작할 때 업데이트 확인'), findsNothing); // 첫 분류(보고서 모양)에는 없다

    await tester.enterText(field, '업');
    await tester.pumpAndSettle();
    expect(find.text('시작할 때 업데이트 확인'), findsNothing); // 1글자는 거르지 않는다

    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pumpAndSettle();
    expect(find.text('시작할 때 업데이트 확인'), findsWidgets);

    await tester.enterText(field, '');
    await tester.pumpAndSettle();
    expect(find.text('시작할 때 업데이트 확인'), findsNothing);

    await tester.enterText(field, '업데');
    await tester.pumpAndSettle();
    expect(find.text('시작할 때 업데이트 확인'), findsWidgets);
  });
}
