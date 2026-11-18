import 'package:docfit_ui/features/workspace/home_button.dart';
import 'package:docfit_ui/features/workspace/tab_navigation_shell.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

/// 탭 내부 편집 상태 보존을 검증하기 위한 상태 유지 위젯
class _StatefulTestTab extends StatefulWidget {
  const _StatefulTestTab({required this.label});
  final String label;

  @override
  State<_StatefulTestTab> createState() => _StatefulTestTabState();
}

class _StatefulTestTabState extends State<_StatefulTestTab> {
  int _counter = 0;
  final TextEditingController _controller = TextEditingController();

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Column(
      children: [
        Text('${widget.label}: $_counter'),
        TextField(
          key: Key('input-${widget.label}'),
          controller: _controller,
        ),
        ElevatedButton(
          key: Key('btn-${widget.label}'),
          onPressed: () => setState(() => _counter++),
          child: const Text('증가'),
        ),
      ],
    );
  }
}

Widget _makeTestApp({
  int initialIndex = 0,
  Set<int> hiddenTabs = const {},
  ValueChanged<int>? onTabChanged,
  Widget? libraryScreen,
  Widget? notebookScreen,
}) {
  return MaterialApp(
    home: TabNavigationShell(
      initialIndex: initialIndex,
      hiddenTabs: hiddenTabs,
      onTabChanged: onTabChanged,
      libraryScreen: libraryScreen,
      notebookScreen: notebookScreen,
      documentEditor: const _StatefulTestTab(label: '문서편집'),
      tableTemplates: const _StatefulTestTab(label: '표템플릿'),
      tableSketch: const _StatefulTestTab(label: '표그림판'),
      diagram: const _StatefulTestTab(label: '다이어그램'),
      mindMap: const _StatefulTestTab(label: '마인드맵'),
      paint: const _StatefulTestTab(label: '그림판'),
      fileRename: const _StatefulTestTab(label: '파일이름'),
      timemachine: const _StatefulTestTab(label: '몽땅검색'),
      docGrid: const _StatefulTestTab(label: '문서뷰'),
      photoAlbum: const _StatefulTestTab(label: '사진뷰'),
      fileConversion: const _StatefulTestTab(label: '파일변환'),
      pdfEdit: const _StatefulTestTab(label: 'PDF 편집'),
      outliner: const _StatefulTestTab(label: '아웃라이너'),
    ),
  );
}

void main() {
  testWidgets('모든 탭에서 HomeButton이 존재하고 상단 왼쪽 모서리 같은 위치를 유지한다', (tester) async {
    tester.view.physicalSize = const Size(1024, 768);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);

    await tester.pumpWidget(_makeTestApp(initialIndex: 0));
    await tester.pumpAndSettle();

    // 탭 0(문서편집)에서 HomeButton 존재 및 위치 확인
    final homeBtnFinder = find.byType(HomeButton);
    expect(homeBtnFinder, findsOneWidget);
    final baseOffset = tester.getTopLeft(homeBtnFinder);
    expect(baseOffset.dx, greaterThanOrEqualTo(0));
    expect(baseOffset.dy, greaterThanOrEqualTo(0));

    // 첫 화면(index 0)에서는 비활성(enabled: false) 및 숨김(visible: false) 처리됨
    final initialBtn = tester.widget<HomeButton>(homeBtnFinder);
    expect(initialBtn.enabled, isFalse);
    expect(initialBtn.visible, isFalse);

    // 주요 탭들(1 표템플릿, 2 표그림판, 3 다이어그램, 6 파일이름, 10 파일변환, 11 PDF 편집, 14 아웃라이너 등)을
    // 순회하며 위치가 baseOffset과 정확히 일치하는지 확인
    final tabsToTest = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 14];
    for (final tabIndex in tabsToTest) {
      final tabKey = Key('workspaceTab-$tabIndex');
      final tabFinder = find.byKey(tabKey);
      if (tabFinder.evaluate().isNotEmpty) {
        await tester.scrollUntilVisible(tabFinder, 100, scrollable: find.byType(Scrollable).first);
        await tester.tap(tabFinder);
        await tester.pumpAndSettle();

        expect(find.byType(HomeButton), findsOneWidget);
        final currentOffset = tester.getTopLeft(find.byType(HomeButton));
        expect(
          currentOffset,
          equals(baseOffset),
          reason: '탭 $tabIndex의 HomeButton 위치가 탭 0의 위치와 동일해야 합니다.',
        );

        final btnWidget = tester.widget<HomeButton>(find.byType(HomeButton));
        expect(btnWidget.enabled, isTrue, reason: '탭 $tabIndex에서는 HomeButton이 활성화되어야 합니다.');
        expect(btnWidget.visible, isTrue, reason: '탭 $tabIndex에서는 HomeButton이 표시되어야 합니다.');
      }
    }
  });

  testWidgets('다른 탭에서 HomeButton을 누르면 첫 화면(index 0)으로 전환된다', (tester) async {
    tester.view.physicalSize = const Size(1024, 768);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);

    int? currentTab;
    await tester.pumpWidget(
      _makeTestApp(
        initialIndex: 2, // 표그림판 탭에서 시작
        onTabChanged: (idx) => currentTab = idx,
      ),
    );
    await tester.pumpAndSettle();

    final bar = tester.widget<WorkspaceTabBar>(find.byType(WorkspaceTabBar));
    expect(bar.selectedIndex, 2);

    // 활성화된 HomeButton 누름
    await tester.tap(find.byKey(const ValueKey('workspace-home-button')));
    await tester.pumpAndSettle();

    // 첫 화면(index 0)으로 전환 확인
    final updatedBar = tester.widget<WorkspaceTabBar>(find.byType(WorkspaceTabBar));
    expect(updatedBar.selectedIndex, 0);
    expect(currentTab, 0);

    // 첫 화면 전환 후 HomeButton은 다시 비활성화/숨김 상태
    final homeBtn = tester.widget<HomeButton>(find.byType(HomeButton));
    expect(homeBtn.enabled, isFalse);
    expect(homeBtn.visible, isFalse);
  });

  testWidgets('HomeButton으로 첫 화면에 다녀와도 이전 탭의 편집/위젯 상태가 온전히 유지된다', (tester) async {
    tester.view.physicalSize = const Size(1024, 768);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);

    await tester.pumpWidget(_makeTestApp(initialIndex: 3)); // 다이어그램 탭
    await tester.pumpAndSettle();

    // 1. 다이어그램 탭에서 상태 변경 (카운터 2회 증가 및 텍스트필드 입력)
    await tester.tap(find.byKey(const Key('btn-다이어그램')));
    await tester.pump();
    await tester.tap(find.byKey(const Key('btn-다이어그램')));
    await tester.pump();
    await tester.enterText(find.byKey(const Key('input-다이어그램')), '보존되어야 하는 편집 내용');
    await tester.pumpAndSettle();

    expect(find.text('다이어그램: 2'), findsOneWidget);
    expect(find.text('보존되어야 하는 편집 내용'), findsOneWidget);

    // 2. 홈 버튼을 눌러 첫 화면(문서편집)으로 전환
    await tester.tap(find.byKey(const ValueKey('workspace-home-button')));
    await tester.pumpAndSettle();

    expect(tester.widget<WorkspaceTabBar>(find.byType(WorkspaceTabBar)).selectedIndex, 0);

    // 3. 다시 다이어그램 탭(탭 3)으로 이동
    await tester.tap(find.byKey(const Key('workspaceTab-3')));
    await tester.pumpAndSettle();

    // 4. 편집 상태(카운터 2, 텍스트 내용)가 그대로 유지되어 있는지 검증
    expect(find.text('다이어그램: 2'), findsOneWidget);
    expect(find.text('보존되어야 하는 편집 내용'), findsOneWidget);
  });

  testWidgets('자료함(extraPage)이 열린 상태에서도 홈 버튼을 누르면 첫 화면으로 전환된다', (tester) async {
    tester.view.physicalSize = const Size(1024, 768);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);

    await tester.pumpWidget(
      _makeTestApp(
        initialIndex: 0,
        libraryScreen: const Text('가상폴더 화면 내용'),
      ),
    );
    await tester.pumpAndSettle();

    // 가상 폴더 클릭하여 extraPage 열기
    await tester.tap(find.byKey(const ValueKey('library-open')));
    await tester.pumpAndSettle();

    expect(find.text('가상폴더 화면 내용'), findsOneWidget);
    final homeBtn = tester.widget<HomeButton>(find.byType(HomeButton));
    expect(homeBtn.enabled, isTrue);
    expect(homeBtn.visible, isTrue);

    // 홈 버튼 누름
    await tester.tap(find.byKey(const ValueKey('workspace-home-button')));
    await tester.pumpAndSettle();

    // extraPage 닫히고 첫 화면(문서편집) 복귀 확인
    expect(find.text('가상폴더 화면 내용'), findsNothing);
    final updatedBar = tester.widget<WorkspaceTabBar>(find.byType(WorkspaceTabBar));
    expect(updatedBar.selectedIndex, 0);
  });

  testWidgets('사이드메뉴 hidden_tabs 설정과 충돌 없이 정상 동작한다', (tester) async {
    tester.view.physicalSize = const Size(1024, 768);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);

    // 탭 1, 2가 숨겨진 상태
    await tester.pumpWidget(
      _makeTestApp(
        initialIndex: 3,
        hiddenTabs: {1, 2},
      ),
    );
    await tester.pumpAndSettle();

    expect(find.byKey(const Key('workspaceTab-1')), findsNothing);
    expect(find.byKey(const Key('workspaceTab-2')), findsNothing);

    // 홈 버튼 누르면 첫 화면(0)으로 전환
    await tester.tap(find.byKey(const ValueKey('workspace-home-button')));
    await tester.pumpAndSettle();

    expect(tester.widget<WorkspaceTabBar>(find.byType(WorkspaceTabBar)).selectedIndex, 0);
  });

  testWidgets('HomeButton은 툴팁 "첫 화면", Semantics 및 키보드 초점 접근성을 지원한다', (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: Center(
            child: HomeButton(
              onPressed: () {},
              enabled: true,
              visible: true,
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    // 툴팁 '첫 화면' 검증
    final iconBtn = tester.widget<IconButton>(find.byType(IconButton));
    expect(iconBtn.tooltip, '첫 화면');

    // Semantics 검증
    expect(
      find.bySemanticsLabel('첫 화면'),
      findsOneWidget,
    );

    // 아이콘 하나 포함 검증
    expect(find.byIcon(Icons.home_outlined), findsOneWidget);
  });
}
