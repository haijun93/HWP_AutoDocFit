import 'package:docfit_ui/features/workspace/tab_navigation_shell.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

/// 햄버거 단추 순환(사용자 지시 2026-11-07):
/// 펼치기 ↔ 닫기. 3번째 누름(사이드메뉴만 표시)은 폐기(2026-11-08).
void main() {
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


  testWidgets('developer mode also only toggles (3rd stage removed) expand and collapse', (tester) async {
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
    await tester.ensureVisible(find.byKey(const ValueKey('sidebar-mode-toggle')));
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
    await tester.ensureVisible(find.byKey(const ValueKey('sidebar-mode-toggle')));
    await tester.tap(find.byKey(const ValueKey('sidebar-mode-toggle')));
    await tester.pumpAndSettle();
    expect(requests, [true, false]);
    expect(railWidth(), kSidebarExpandedWidth);
    expect(find.text('work-area'), findsOneWidget);
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

}
