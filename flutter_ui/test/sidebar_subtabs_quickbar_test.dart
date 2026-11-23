import 'package:docfit_ui/features/workspace/quick_action_bar.dart';
import 'package:docfit_ui/features/workspace/tab_navigation_shell.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

Widget _app({Key? k, int initial = 2, Set<int> hidden = const {}, List<QuickAction>? actions}) => MaterialApp(
      home: TabNavigationShell(
        key: k,
        documentEditor: const Text('doc'),
        tableTemplates: const Text('tpl'),
        tableSketch: const Text('sketch'),
        diagram: const Text('diagram'),
        mindMap: const Text('mind'),
        fileConversion: const Text('conv'),
        paint: const Text('paint'),
        fileRename: const Text('rename'),
        timemachine: const Text('tm'),
        initialIndex: initial,
        hiddenTabs: hidden,
        quickActions: actions ?? const [],
      ),
    );

void _size(WidgetTester t, double w) {
  t.view.physicalSize = Size(w, 900);
  t.view.devicePixelRatio = 1;
  addTearDown(t.view.reset);
}

void main() {
  setUp(() => quickBarVisibleNotifier.value = true);

  testWidgets('위쪽 가로 탭: 그리기 묶음에서 나오고 누르면 화면이 바뀐다', (t) async {
    _size(t, 1400);
    await t.pumpWidget(_app());
    expect(find.byKey(const ValueKey('workspaceSubTabs')), findsOneWidget);
    for (final i in [2, 3, 4]) {
      expect(find.byKey(ValueKey('workspaceSubTab-$i')), findsOneWidget);
    }
    await t.tap(find.byKey(const ValueKey('workspaceSubTab-3')));
    await t.pump();
    expect(find.text('diagram'), findsOneWidget);
    // 왼쪽 메뉴 항목은 그대로 있다.
    expect(find.byKey(const Key('workspaceTab-3')), findsOneWidget);
  });

  testWidgets('위쪽 가로 탭 글자는 한글 글꼴(NotoSansKR)로 그린다 — RichText 가 앱 글꼴을 잃어 한글이 네모로 깨지던 문제', (t) async {
    _size(t, 1400);
    await t.pumpWidget(_app());
    for (final i in [2, 3, 4]) {
      final rich = t.widget<RichText>(find.descendant(
        of: find.byKey(ValueKey('workspaceSubTab-$i')),
        matching: find.byType(RichText),
      ));
      final style = rich.text.style!;
      // 시험 앱은 기본 테마(Roboto)라 fontFamily 는 테마를 따른다. 실제 앱 테마는 NotoSansKR 이고, 어느 쪽이든 한글 글꼴이 대체 목록에 있어야 한글이 깨지지 않는다.
      expect(style.fontFamily, isNotNull, reason: '탭 $i 글꼴');
      expect(style.fontFamilyFallback, contains('NotoSansKR'), reason: '탭 $i 대체 글꼴');
    }
  });

  testWidgets('위쪽 가로 탭: 묶음 밖 화면·보이는 화면이 하나뿐이면 나오지 않는다', (t) async {
    _size(t, 1400);
    await t.pumpWidget(_app(k: UniqueKey(), initial: 0));
    expect(find.byKey(const ValueKey('workspaceSubTabs')), findsNothing);
    await t.pumpWidget(_app(k: UniqueKey(), initial: 2, hidden: {3, 4}));
    expect(find.byKey(const ValueKey('workspaceSubTabs')), findsNothing);
    await t.pumpWidget(_app(k: UniqueKey(), initial: 2, hidden: {3}));
    expect(find.byKey(const ValueKey('workspaceSubTab-3')), findsNothing);
    expect(find.byKey(const ValueKey('workspaceSubTab-4')), findsOneWidget);
  });

  testWidgets('빠른 실행 막대: 아이콘·이름 도움말·배지·눌러 실행', (t) async {
    _size(t, 1400);
    var hits = 0;
    await t.pumpWidget(_app(initial: 0, actions: [
      QuickAction(id: 'a', label: '새 문서', icon: Icons.note_add_outlined, badge: 120, onPressed: (_) => hits++),
      QuickAction(id: 'b', label: '작업 로그', icon: Icons.receipt_long_outlined, onPressed: (_) {}),
    ]));
    expect(find.byKey(const ValueKey('quickAction-a')), findsOneWidget);
    expect(find.byTooltip('작업 로그'), findsOneWidget);
    expect(find.text('99+'), findsOneWidget);
    expect(find.byKey(const ValueKey('quickBadge-b')), findsNothing);
    await t.tap(find.byKey(const ValueKey('quickAction-a')));
    expect(hits, 1);
  });

  testWidgets('빠른 실행 막대: 숨기기·다시 보이기, 좁은 창에서는 접힘, 항목 없으면 없음', (t) async {
    _size(t, 1400);
    final actions = [QuickAction(id: 'a', label: '새 문서', icon: Icons.add, onPressed: (_) {})];
    await t.pumpWidget(_app(initial: 0, actions: actions));
    await t.tap(find.byKey(const ValueKey('quickBarHide')));
    await t.pump();
    expect(quickBarVisibleNotifier.value, isFalse);
    expect(find.byKey(const ValueKey('quickAction-a')), findsNothing);
    await t.tap(find.byKey(const ValueKey('quickBarShow')));
    await t.pump();
    expect(find.byKey(const ValueKey('quickAction-a')), findsOneWidget);

    _size(t, 1000);
    await t.pumpWidget(_app(initial: 0, actions: actions));
    await t.pump();
    expect(find.byKey(const ValueKey('quickAction-a')), findsNothing);
    expect(find.byKey(const ValueKey('quickBarShow')), findsNothing);

    _size(t, 1400);
    await t.pumpWidget(_app(initial: 0));
    await t.pump();
    expect(find.byKey(const ValueKey('quickBarHide')), findsNothing);
  });
}
