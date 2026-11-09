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
  });


  testWidgets('developer mode also only toggles (3rd stage removed) expand and collapse', (tester) async {
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
          developerMode: true,
        ),
      ));
      await tester.pumpAndSettle();
      final toggle = find.byKey(const ValueKey('workspaceSidebarToggle'));
      for (var i = 0; i < taps; i++) {
        await tester.tap(toggle);
        await tester.pumpAndSettle();
      }
      final burger = tester.getCenter(find.descendant(of: toggle, matching: find.byType(Icon))).dx;
      final firstMenuIcon = tester.getCenter(find.descendant(of: find.byType(InkWell), matching: find.byType(Icon)).first).dx;
      expect((burger - firstMenuIcon).abs(), lessThan(1.0), reason: 'size=$size taps=$taps');
    }
    addTearDown(tester.view.reset);
    await check(const Size(1040, 700), 0); // 펼침
    await check(const Size(1040, 700), 1); // 접힘
    await check(const Size(1040, 700), 2); // 펼침으로 복귀
  });

}
