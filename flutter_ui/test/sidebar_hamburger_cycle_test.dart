import 'package:docfit_ui/features/workspace/tab_navigation_shell.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

/// 햄버거 단추 순환(사용자 지시 2026-11-07):
/// 펼치기 → 닫기 → 닫은 채로 오른쪽 작업 화면 닫기 → 다시 펼치기.
void main() {
  testWidgets('hamburger cycles expand, collapse, collapse and close work area', (tester) async {
    tester.view.physicalSize = const Size(600, 900);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    final requests = <bool>[];
    await tester.pumpWidget(MaterialApp(
      home: TabNavigationShell(
        documentEditor: const Text('work-area'),
        tableTemplates: const SizedBox(),
        tableSketch: const SizedBox(),
        diagram: const SizedBox(),
        mindMap: const SizedBox(),
        fileConversion: const SizedBox(),
        paint: const SizedBox(),
        fileRename: const SizedBox(),
        timemachine: const SizedBox(),
        developerMode: true,
        onSidebarModeRequest: (enabled) async {
          requests.add(enabled);
          return true;
        },
      ),
    ));
    await tester.pumpAndSettle();
    final toggle = find.byKey(const ValueKey('workspaceSidebarToggle'));
    double railWidth() => tester.getSize(find.ancestor(of: toggle, matching: find.byType(Container)).first).width;

    // 좁은 창이라 처음엔 닫힌 상태.
    expect(railWidth(), kSidebarCollapsedWidth);

    // (1) 펼치기
    await tester.tap(toggle);
    await tester.pumpAndSettle();
    expect(railWidth(), kSidebarExpandedWidth);
    expect(find.text('work-area'), findsOneWidget);

    // (2) 닫기
    await tester.tap(toggle);
    await tester.pumpAndSettle();
    expect(railWidth(), kSidebarCollapsedWidth);
    expect(find.text('work-area'), findsOneWidget);

    // (3) 닫은 채로 작업 화면 닫기(앱 창도 사이드메뉴 모드로 요청)
    await tester.tap(toggle);
    await tester.pumpAndSettle();
    // 사이드메뉴 모드에서는 아이콘 줄이 창 폭을 채운다.
    expect(railWidth(), tester.view.physicalSize.width);
    expect(find.text('work-area'), findsNothing);
    expect(requests, [true]);

    // 다시 (1) 펼치기 + 작업 화면 복귀
    await tester.tap(toggle);
    await tester.pumpAndSettle();
    expect(railWidth(), kSidebarExpandedWidth);
    expect(find.text('work-area'), findsOneWidget);
    expect(requests, [true, false]);
  });

  testWidgets('hamburger falls back to expand when sidebar mode is refused', (tester) async {
    tester.view.physicalSize = const Size(600, 900);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    await tester.pumpWidget(MaterialApp(
      home: TabNavigationShell(
        documentEditor: const Text('work-area'),
        tableTemplates: const SizedBox(),
        tableSketch: const SizedBox(),
        diagram: const SizedBox(),
        mindMap: const SizedBox(),
        fileConversion: const SizedBox(),
        paint: const SizedBox(),
        fileRename: const SizedBox(),
        timemachine: const SizedBox(),
        developerMode: true,
        onSidebarModeRequest: (enabled) async => false,
      ),
    ));
    await tester.pumpAndSettle();
    final toggle = find.byKey(const ValueKey('workspaceSidebarToggle'));
    double railWidth() => tester.getSize(find.ancestor(of: toggle, matching: find.byType(Container)).first).width;
    await tester.tap(toggle); await tester.pumpAndSettle();
    expect(railWidth(), kSidebarExpandedWidth);
    await tester.tap(toggle); await tester.pumpAndSettle();
    expect(railWidth(), kSidebarCollapsedWidth);
    await tester.tap(toggle); await tester.pumpAndSettle();
    expect(railWidth(), kSidebarExpandedWidth);
    expect(find.text('work-area'), findsOneWidget);
  });

  testWidgets('normal mode only toggles expand and collapse', (tester) async {
    tester.view.physicalSize = const Size(600, 900);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    final requests = <bool>[];
    await tester.pumpWidget(MaterialApp(
      home: TabNavigationShell(
        documentEditor: const Text('work-area'),
        tableTemplates: const SizedBox(),
        tableSketch: const SizedBox(),
        diagram: const SizedBox(),
        mindMap: const SizedBox(),
        fileConversion: const SizedBox(),
        paint: const SizedBox(),
        fileRename: const SizedBox(),
        timemachine: const SizedBox(),
        onSidebarModeRequest: (enabled) async {
          requests.add(enabled);
          return true;
        },
      ),
    ));
    await tester.pumpAndSettle();
    final toggle = find.byKey(const ValueKey('workspaceSidebarToggle'));
    double railWidth() => tester.getSize(find.ancestor(of: toggle, matching: find.byType(Container)).first).width;
    for (var i = 0; i < 3; i++) {
      await tester.tap(toggle); await tester.pumpAndSettle();
      expect(railWidth(), kSidebarExpandedWidth);
      await tester.tap(toggle); await tester.pumpAndSettle();
      expect(railWidth(), kSidebarCollapsedWidth);
      expect(find.text('work-area'), findsOneWidget);
    }
    expect(requests, isEmpty);
  });

  testWidgets('sidebar mode always shows only the icon rail and fits a rail-wide window', (tester) async {
    tester.view.physicalSize = const Size(600, 900);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    final requests = <bool>[];
    await tester.pumpWidget(MaterialApp(
      home: TabNavigationShell(
        documentEditor: const Text('work-area'),
        tableTemplates: const SizedBox(),
        tableSketch: const SizedBox(),
        diagram: const SizedBox(),
        mindMap: const SizedBox(),
        fileConversion: const SizedBox(),
        paint: const SizedBox(),
        fileRename: const SizedBox(),
        timemachine: const SizedBox(),
        developerMode: true,
        onSidebarModeRequest: (enabled) async {
          requests.add(enabled);
          return true;
        },
      ),
    ));
    await tester.pumpAndSettle();
    final toggle = find.byKey(const ValueKey('workspaceSidebarToggle'));
    double railWidth() => tester.getSize(find.ancestor(of: toggle, matching: find.byType(Container)).first).width;

    // 펼친 상태에서 '사이드메뉴 모드' 단추로 바로 켜도 글자 라벨 없이 아이콘 줄만 남는다.
    await tester.tap(toggle);
    await tester.pumpAndSettle();
    expect(railWidth(), kSidebarExpandedWidth);
    await tester.tap(find.byKey(const ValueKey('sidebar-mode-toggle')));
    await tester.pumpAndSettle();
    expect(requests, [true]);
    expect(railWidth(), tester.view.physicalSize.width);
    expect(find.text('work-area'), findsNothing);
    expect(find.text('문서편집'), findsNothing);
    expect(find.textContaining('사이드메뉴 모드'), findsNothing);
    expect(find.text('왼쪽↔오른쪽 고정'), findsNothing);
    expect(find.byKey(const ValueKey('sidebar-mode-switch-side')), findsOneWidget);
    expect(find.byKey(const ValueKey('always-on-top-toggle')), findsOneWidget);

    // 앱 창이 아이콘 줄 폭으로 줄어도 넘침 오류 없이 모든 단추가 그 폭 안에 그려진다.
    tester.view.physicalSize = const Size(kSidebarCollapsedWidth, 900);
    await tester.pumpAndSettle();
    expect(tester.takeException(), isNull);
    expect(railWidth(), kSidebarCollapsedWidth);
    for (final key in ['workspaceSidebarToggle', 'sidebar-mode-switch-side', 'always-on-top-toggle', 'sidebar-mode-toggle']) {
      final rect = tester.getRect(find.byKey(ValueKey(key)));
      expect(rect.left, greaterThanOrEqualTo(0), reason: key);
      expect(rect.right, lessThanOrEqualTo(kSidebarCollapsedWidth), reason: key);
    }

    // 끄면 앱 창과 함께 원래(펼친) 메뉴와 작업 화면으로 돌아간다.
    tester.view.physicalSize = const Size(600, 900);
    await tester.tap(find.byKey(const ValueKey('sidebar-mode-toggle')));
    await tester.pumpAndSettle();
    expect(requests, [true, false]);
    expect(railWidth(), kSidebarExpandedWidth);
    expect(find.text('work-area'), findsOneWidget);
  });

  testWidgets('sidebar-only mode fills the whole window width with the menu (no blank area)', (tester) async {
    tester.view.physicalSize = const Size(1040, 700);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    await tester.pumpWidget(MaterialApp(
      home: TabNavigationShell(
        documentEditor: const Text('work-area'),
        tableTemplates: const SizedBox(),
        tableSketch: const SizedBox(),
        diagram: const SizedBox(),
        mindMap: const SizedBox(),
        fileConversion: const SizedBox(),
        paint: const SizedBox(),
        fileRename: const SizedBox(),
        timemachine: const SizedBox(),
        developerMode: true,
        onSidebarModeRequest: (enabled) async => true,
      ),
    ));
    await tester.pumpAndSettle();
    final toggle = find.byKey(const ValueKey('workspaceSidebarToggle'));
    // 넓은 창은 펼친 상태로 시작: 닫기 → 사이드메뉴 모드.
    for (var i = 0; i < 2; i++) {
      await tester.tap(toggle);
      await tester.pumpAndSettle();
    }
    expect(find.text('work-area'), findsNothing);
    // 앱이 창을 아이콘 줄 폭(Windows 최소 136px)으로 줄인 상태.
    tester.view.physicalSize = const Size(136, 700);
    await tester.pumpAndSettle();
    final rail = tester.getSize(find.ancestor(of: toggle, matching: find.byType(Container)).first);
    expect(rail.width, 136);
    final err = tester.takeException(); expect(err, isNull, reason: err?.toString());
  });

  testWidgets('sidebar drag handle calls API during drag and when released', (tester) async {
    tester.view.physicalSize = const Size(600, 900);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    final drags = <(double, bool)>[];
    await tester.pumpWidget(MaterialApp(home: TabNavigationShell(
      documentEditor: const SizedBox(), tableTemplates: const SizedBox(), tableSketch: const SizedBox(),
      diagram: const SizedBox(), mindMap: const SizedBox(), fileConversion: const SizedBox(),
      paint: const SizedBox(), fileRename: const SizedBox(), timemachine: const SizedBox(),
      developerMode: true,
      onSidebarModeRequest: (enabled) async => true,
      onSidebarDrag: (delta, save) async => drags.add((delta, save)),
    )));
    await tester.pumpAndSettle();
    final toggle = find.byKey(const ValueKey('workspaceSidebarToggle'));
    await tester.tap(toggle); await tester.pumpAndSettle();
    await tester.tap(toggle); await tester.pumpAndSettle();
    await tester.tap(toggle); await tester.pumpAndSettle();
    final handle = find.byKey(const ValueKey('sidebar-drag-handle'));
    expect(handle, findsOneWidget);
    expect(tester.getSize(handle).height, 14);
    await tester.drag(handle, const Offset(0, 42));
    await tester.pumpAndSettle();
    expect(drags.any((entry) => entry.$1 > 0 && !entry.$2), isTrue);
    expect(drags.last, (0, true));
  });

  testWidgets('hamburger icon lines up vertically with the menu icons', (tester) async {
    Future<void> check(Size size, int taps) async {
      tester.view.physicalSize = size;
      tester.view.devicePixelRatio = 1;
      await tester.pumpWidget(MaterialApp(
        home: TabNavigationShell(
          documentEditor: const SizedBox(), tableTemplates: const SizedBox(), tableSketch: const SizedBox(),
          diagram: const SizedBox(), mindMap: const SizedBox(), fileConversion: const SizedBox(),
          paint: const SizedBox(), fileRename: const SizedBox(), timemachine: const SizedBox(),
          developerMode: true, onSidebarModeRequest: (enabled) async => true,
        ),
      ));
      await tester.pumpAndSettle();
      final toggle = find.byKey(const ValueKey('workspaceSidebarToggle'));
      for (var i = 0; i < taps; i++) {
        await tester.tap(toggle);
        await tester.pumpAndSettle();
      }
      final burger = tester.getCenter(find.byIcon(Icons.menu)).dx;
      final firstMenuIcon = tester.getCenter(find.descendant(of: find.byType(InkWell), matching: find.byType(Icon)).first).dx;
      expect((burger - firstMenuIcon).abs(), lessThan(1.0), reason: 'size=$size taps=$taps');
    }
    addTearDown(tester.view.reset);
    await check(const Size(1040, 700), 0); // 펼침
    await check(const Size(1040, 700), 1); // 접힘
    await check(const Size(1040, 700), 2); // 사이드메뉴 모드
  });

  testWidgets('sidebar-only mode hides notices so they never cover the menu', (tester) async {
    tester.view.physicalSize = const Size(1040, 700);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    late BuildContext pageContext;
    await tester.pumpWidget(MaterialApp(
      home: Scaffold(body: TabNavigationShell(
        documentEditor: Builder(builder: (c) { pageContext = c; return const Text('work-area'); }),
        tableTemplates: const SizedBox(), tableSketch: const SizedBox(), diagram: const SizedBox(),
        mindMap: const SizedBox(), fileConversion: const SizedBox(), paint: const SizedBox(),
        fileRename: const SizedBox(), timemachine: const SizedBox(),
        developerMode: true, onSidebarModeRequest: (enabled) async => true,
      )),
    ));
    await tester.pumpAndSettle();
    final messenger = ScaffoldMessenger.of(pageContext);
    final toggle = find.byKey(const ValueKey('workspaceSidebarToggle'));
    messenger.showSnackBar(const SnackBar(content: Text('알림-전')));
    await tester.pump();
    for (var i = 0; i < 3 && find.text('work-area').evaluate().isNotEmpty; i++) { await tester.tap(toggle); await tester.pump(const Duration(milliseconds: 300)); }
    expect(find.text('work-area'), findsNothing);
    await tester.pump(const Duration(milliseconds: 300));
    expect(find.text('알림-전'), findsNothing);
    messenger.showSnackBar(const SnackBar(content: Text('알림-중')));
    for (var i = 0; i < 5; i++) { await tester.pump(const Duration(milliseconds: 60)); }
    expect(find.text('알림-중'), findsNothing);
    // 사이드메뉴 모드를 끄면 알림이 다시 보인다.
    await tester.tap(toggle);
    await tester.pumpAndSettle();
    messenger.showSnackBar(const SnackBar(content: Text('알림-후')));
    await tester.pump(const Duration(milliseconds: 500));
    expect(find.text('알림-후'), findsOneWidget);
    await tester.pumpAndSettle(const Duration(seconds: 5));
  });
}
