import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:docfit_ui/features/library/library_api.dart';
import 'package:docfit_ui/features/library/library_screen.dart';
import 'package:docfit_ui/features/library/library_view_prefs.dart';

void main() {
  group('LIBPANES: 자료함 자료 영역 개수 1~4개 설정', () {
    testWidgets('기본값은 4개 영역이고 상단 도구줄에 1·2·3·4 선택 단추가 표시된다', (tester) async {
      await tester.binding.setSurfaceSize(const Size(1200, 800));
      addTearDown(() => tester.binding.setSurfaceSize(null));

      final api = InMemoryLibraryApi(
        initialBoxes: const [LibraryBox(id: 'box_1', name: '기본자료함')],
      );

      await tester.pumpWidget(MaterialApp(
        home: Scaffold(
          body: LibraryScreen(api: api),
        ),
      ));
      await tester.pumpAndSettle();

      // 상단 도구줄에 영역 수 선택 토글 확인
      expect(find.byKey(const ValueKey('libraryPaneCountToggle')), findsOneWidget);
      expect(find.byKey(const ValueKey('libraryPaneCount-1')), findsOneWidget);
      expect(find.byKey(const ValueKey('libraryPaneCount-2')), findsOneWidget);
      expect(find.byKey(const ValueKey('libraryPaneCount-3')), findsOneWidget);
      expect(find.byKey(const ValueKey('libraryPaneCount-4')), findsOneWidget);

      // 기본 4개 영역 확인
      expect(find.byKey(const ValueKey('librarySpace-core')), findsOneWidget);
      expect(find.byKey(const ValueKey('librarySpace-basis')), findsOneWidget);
      expect(find.byKey(const ValueKey('librarySpace-reference')), findsOneWidget);
      expect(find.byKey(const ValueKey('librarySpace-temp')), findsOneWidget);
    });

    testWidgets('1~4 각각 영역 수 전환 시 해당 영역만 표시된다', (tester) async {
      await tester.binding.setSurfaceSize(const Size(1200, 800));
      addTearDown(() => tester.binding.setSurfaceSize(null));

      final api = InMemoryLibraryApi(
        initialBoxes: const [LibraryBox(id: 'box_1', name: '자료함')],
      );

      await tester.pumpWidget(MaterialApp(
        home: Scaffold(
          body: LibraryScreen(api: api),
        ),
      ));
      await tester.pumpAndSettle();

      // 1개 선택 (1=전체)
      await tester.tap(find.byKey(const ValueKey('libraryPaneCount-1')));
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('librarySpace-core')), findsOneWidget);
      expect(find.byKey(const ValueKey('librarySpace-basis')), findsNothing);
      expect(find.byKey(const ValueKey('librarySpace-reference')), findsNothing);
      expect(find.byKey(const ValueKey('librarySpace-temp')), findsNothing);

      // 2개 선택 (2=좌우)
      await tester.tap(find.byKey(const ValueKey('libraryPaneCount-2')));
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('librarySpace-core')), findsOneWidget);
      expect(find.byKey(const ValueKey('librarySpace-basis')), findsOneWidget);
      expect(find.byKey(const ValueKey('librarySpace-reference')), findsNothing);
      expect(find.byKey(const ValueKey('librarySpace-temp')), findsNothing);

      // 3개 선택 (3=좌1+우2)
      await tester.tap(find.byKey(const ValueKey('libraryPaneCount-3')));
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('librarySpace-core')), findsOneWidget);
      expect(find.byKey(const ValueKey('librarySpace-basis')), findsOneWidget);
      expect(find.byKey(const ValueKey('librarySpace-reference')), findsOneWidget);
      expect(find.byKey(const ValueKey('librarySpace-temp')), findsNothing);

      // 4개 선택 (4=2x2)
      await tester.tap(find.byKey(const ValueKey('libraryPaneCount-4')));
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('librarySpace-core')), findsOneWidget);
      expect(find.byKey(const ValueKey('librarySpace-basis')), findsOneWidget);
      expect(find.byKey(const ValueKey('librarySpace-reference')), findsOneWidget);
      expect(find.byKey(const ValueKey('librarySpace-temp')), findsOneWidget);
    });

    testWidgets('선택값 저장 및 다음 실행 때 복원된다', (tester) async {
      await tester.binding.setSurfaceSize(const Size(1200, 800));
      addTearDown(() => tester.binding.setSurfaceSize(null));

      final prefs = MemoryLibraryViewPrefs();
      final api = InMemoryLibraryApi(
        initialBoxes: const [LibraryBox(id: 'box_1', name: '자료함')],
      );

      // 첫 실행에서 2개로 변경
      await tester.pumpWidget(MaterialApp(
        home: Scaffold(
          body: LibraryScreen(api: api, viewPrefs: prefs),
        ),
      ));
      await tester.pumpAndSettle();

      await tester.tap(find.byKey(const ValueKey('libraryPaneCount-2')));
      await tester.pumpAndSettle();

      expect(await prefs.read(libraryPaneCountKey), '2');

      // 새 인스턴스로 다시 실행 (복원 확인)
      await tester.pumpWidget(MaterialApp(
        home: Scaffold(
          body: LibraryScreen(api: api, viewPrefs: prefs),
        ),
      ));
      await tester.pumpAndSettle();

      expect(find.byKey(const ValueKey('librarySpace-core')), findsOneWidget);
      expect(find.byKey(const ValueKey('librarySpace-basis')), findsOneWidget);
      expect(find.byKey(const ValueKey('librarySpace-reference')), findsNothing);
      expect(find.byKey(const ValueKey('librarySpace-temp')), findsNothing);
    });

    testWidgets('개수를 줄여도 내용은 삭제되지 않고 숨겨지며, 늘리면 다시 보인다', (tester) async {
      await tester.binding.setSurfaceSize(const Size(1200, 800));
      addTearDown(() => tester.binding.setSurfaceSize(null));

      final api = InMemoryLibraryApi(
        initialBoxes: const [LibraryBox(id: 'box_1', name: '자료함')],
        initialItems: const [
          LibraryItem(id: 'item_1', boxId: 'box_1', kind: LibraryItemKind.text, title: '핵심메모', space: LibrarySpace.core, x: 10, y: 10, text: '핵심'),
          LibraryItem(id: 'item_2', boxId: 'box_1', kind: LibraryItemKind.text, title: '기초메모', space: LibrarySpace.basis, x: 10, y: 10, text: '기초'),
          LibraryItem(id: 'item_3', boxId: 'box_1', kind: LibraryItemKind.text, title: '참고메모', space: LibrarySpace.reference, x: 10, y: 10, text: '참고'),
          LibraryItem(id: 'item_4', boxId: 'box_1', kind: LibraryItemKind.text, title: '임시메모', space: LibrarySpace.temp, x: 10, y: 10, text: '임시'),
        ],
      );

      await tester.pumpWidget(MaterialApp(
        home: Scaffold(
          body: LibraryScreen(api: api),
        ),
      ));
      await tester.pumpAndSettle();

      // 초기 4개 모두 표시
      expect(find.text('핵심메모'), findsOneWidget);
      expect(find.text('기초메모'), findsOneWidget);
      expect(find.text('참고메모'), findsOneWidget);
      expect(find.text('임시메모'), findsOneWidget);

      // 1개로 축소 -> 핵심메모만 보이고 나머지는 숨김
      await tester.tap(find.byKey(const ValueKey('libraryPaneCount-1')));
      await tester.pumpAndSettle();

      expect(find.text('핵심메모'), findsOneWidget);
      expect(find.text('기초메모'), findsNothing);
      expect(find.text('참고메모'), findsNothing);
      expect(find.text('임시메모'), findsNothing);

      // 백엔드 API에는 여전히 4개 아이템 모두 존재
      final itemsAfterShrink = await api.listItems('box_1');
      expect(itemsAfterShrink.length, 4);

      // 4개로 복원 -> 숨겨졌던 기초·참고·임시메모 다시 표시
      await tester.tap(find.byKey(const ValueKey('libraryPaneCount-4')));
      await tester.pumpAndSettle();

      expect(find.text('핵심메모'), findsOneWidget);
      expect(find.text('기초메모'), findsOneWidget);
      expect(find.text('참고메모'), findsOneWidget);
      expect(find.text('임시메모'), findsOneWidget);
    });

    testWidgets('영역 수가 변경되어도 탐색기 형식 및 선택 기능이 정상 동작한다', (tester) async {
      await tester.binding.setSurfaceSize(const Size(1200, 800));
      addTearDown(() => tester.binding.setSurfaceSize(null));

      final api = InMemoryLibraryApi(
        initialBoxes: const [LibraryBox(id: 'box_1', name: '자료함')],
        initialItems: const [
          LibraryItem(id: 'item_1', boxId: 'box_1', kind: LibraryItemKind.text, title: '문서1', space: LibrarySpace.core, x: 10, y: 10, text: '내용1'),
          LibraryItem(id: 'item_2', boxId: 'box_1', kind: LibraryItemKind.text, title: '문서2', space: LibrarySpace.temp, x: 10, y: 10, text: '내용2'),
        ],
      );

      await tester.pumpWidget(MaterialApp(
        home: Scaffold(
          body: LibraryScreen(api: api),
        ),
      ));
      await tester.pumpAndSettle();

      // 1개 영역으로 설정
      await tester.tap(find.byKey(const ValueKey('libraryPaneCount-1')));
      await tester.pumpAndSettle();

      // 탐색기 보기로 전환
      await tester.tap(find.byKey(const ValueKey('libraryViewExplorer')));
      await tester.pumpAndSettle();

      // 탐색기에서는 모든 폴더가 보이므로 임시저장소의 문서2도 확인 가능
      expect(find.byKey(const ValueKey('libraryExplorerView')), findsOneWidget);
      expect(find.byKey(const ValueKey('libraryExplorerFolder-temp')), findsOneWidget);

      // 다시 캔버스로 전환 시 여전히 1개 영역
      await tester.tap(find.byKey(const ValueKey('libraryViewCanvas')));
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('librarySpace-core')), findsOneWidget);
      expect(find.byKey(const ValueKey('librarySpace-temp')), findsNothing);
    });
  });
}
