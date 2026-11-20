import 'package:docfit_ui/features/workspace/tab_navigation_shell.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

Widget _shell({bool? expanded, Set<int> hidden = const {}}) => MaterialApp(
      home: Scaffold(
        body: Row(children: [
          WorkspaceTabBar(
            labels: const ['문서편집', '표템플릿', '표그림판', '다이어그램', '마인드맵', '붙임사진 편집', '파일명 일괄변경', '몽땅검색', '문서뷰', '사진뷰', '파일변환', 'PDF 편집', '설정', '사용 방법', '아웃라이너', '검색 벤치마크'],
            icons: List.filled(16, Icons.circle),
            selectedIndex: 0,
            onSelected: (_) {},
            scrollController: ScrollController(),
            hiddenTabs: hidden,
            expanded: expanded ?? true,
          ),
        ]),
      ),
    );

void main() {
  testWidgets('펼침: 묶음 제목이 보이고 숨긴 탭의 묶음은 제목도 사라진다', (tester) async {
    tester.view.physicalSize = const Size(1400, 1200);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    await tester.pumpWidget(_shell());
    for (final t in ['문서 작업', '그리기', '찾기·자료', '변환·도구']) {
      expect(find.byKey(ValueKey('workspaceGroup-$t')), findsOneWidget, reason: t);
    }
    await tester.pumpWidget(_shell(hidden: {2, 3, 4}));
    expect(find.byKey(const ValueKey('workspaceGroup-그리기')), findsNothing);
    expect(find.byKey(const Key('workspaceTab-2')), findsNothing);
  });

  testWidgets('접힘: 제목 글자 없이 구분선, 항목 이름은 툴팁', (tester) async {
    tester.view.physicalSize = const Size(1400, 1200);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    await tester.pumpWidget(_shell(expanded: false));
    expect(find.byKey(const ValueKey('workspaceGroup-그리기')), findsNothing);
    expect(find.byType(Divider), findsNWidgets(3));
    expect(find.byTooltip('몽땅검색'), findsOneWidget);
  });
}
