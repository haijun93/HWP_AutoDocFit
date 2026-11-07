import 'package:flutter/gestures.dart' show kSecondaryButton;
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:docfit_ui/features/library/library_api.dart';
import 'package:docfit_ui/features/library/library_explorer_view.dart';
import 'package:docfit_ui/features/library/library_screen.dart';
import 'package:docfit_ui/features/library/library_view_prefs.dart';

/// 실제로 파일을 열지 않는 가짜 실행기.
class _FakeOpener implements LibraryOpenApi {
  final opened = <String>[];

  @override
  Future<LibraryOpenResult> openItem({String? itemId, required LibraryItemKind kind, required String source, bool confirmed = false}) async {
    opened.add(source);
    return const LibraryOpenResult(ok: true);
  }
}

/// 크기·수정일을 정해 두고 어떤 자료를 물었는지 기록하는 가짜 조회기.
class _FakeInfo implements LibraryFileInfoApi {
  _FakeInfo(this.values);
  final Map<String, LibraryFileInfo> values;
  final asked = <List<String>>[];

  @override
  Future<Map<String, LibraryFileInfo>> fileInfo(List<String> itemIds) async {
    asked.add(itemIds);
    return {for (final id in itemIds) if (values.containsKey(id)) id: values[id]!};
  }

  Set<String> get askedIds => {for (final batch in asked) ...batch};
}

Future<void> _pump(
  WidgetTester tester,
  LibraryApi api, {
  LibraryViewPrefs? prefs,
  _FakeOpener? opener,
  LibraryFileInfoApi? info,
}) async {
  tester.view.physicalSize = const Size(1500, 900);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  await tester.pumpWidget(MaterialApp(
    home: LibraryScreen(api: api, openApi: opener, viewPrefs: prefs ?? MemoryLibraryViewPrefs(), fileInfoApi: info),
  ));
  await tester.pumpAndSettle();
}

Future<void> _toExplorer(WidgetTester tester) async {
  await tester.tap(find.text('탐색기 형식'));
  await tester.pumpAndSettle();
}

Finder _row(String id) => find.byKey(ValueKey('libraryExplorerRow-$id'));
Finder _folderRow(LibrarySpace s) => find.byKey(ValueKey('libraryExplorerFolder-${s.name}'));
Finder _tree(LibrarySpace s) => find.byKey(ValueKey('libraryExplorerTree-${s.name}'));
String _status(WidgetTester tester) => tester.widget<Text>(find.byKey(const ValueKey('libraryExplorerStatus'))).data!;

Future<void> _click(WidgetTester tester, Finder finder, {bool ctrl = false, bool shift = false}) async {
  if (ctrl) await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
  if (shift) await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
  await tester.tap(finder);
  await tester.pump();
  if (shift) await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
  if (ctrl) await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
  await tester.pumpAndSettle();
  // 다음 클릭이 더블클릭으로 묶이지 않게 실제 시간을 조금 보낸다.
  await tester.runAsync(() => Future<void>.delayed(const Duration(milliseconds: 520)));
}

Future<void> _doubleClick(WidgetTester tester, Finder finder) async {
  await tester.tap(finder);
  await tester.pump(const Duration(milliseconds: 50));
  await tester.tap(finder);
  await tester.pumpAndSettle();
}

/// 자료함 하나와 핵심자료 파일 3개·기초자료 링크·메모를 만든다.
Future<(InMemoryLibraryApi, LibraryBox, List<LibraryItem>)> _sample() async {
  final api = InMemoryLibraryApi();
  final box = await api.createBox('업무 자료');
  final items = <LibraryItem>[
    await api.addItem(boxId: box.id, kind: LibraryItemKind.file, title: '나 보고서.hwpx', space: LibrarySpace.core, x: 16, y: 16, source: 'C:/자료/나 보고서.hwpx'),
    await api.addItem(boxId: box.id, kind: LibraryItemKind.file, title: '가 계획.pdf', space: LibrarySpace.core, x: 166, y: 16, source: 'C:/자료/가 계획.pdf'),
    await api.addItem(boxId: box.id, kind: LibraryItemKind.file, title: '다 붙임.xlsx', space: LibrarySpace.core, x: 316, y: 16, source: 'C:/자료/다 붙임.xlsx'),
    await api.addItem(boxId: box.id, kind: LibraryItemKind.link, title: '누리집', space: LibrarySpace.basis, x: 16, y: 16, source: 'https://example.com'),
    await api.addItem(boxId: box.id, kind: LibraryItemKind.text, title: '메모', space: LibrarySpace.basis, x: 166, y: 16, text: '내용'),
  ];
  return (api, box, items);
}

void main() {
  group('도움 함수', () {
    test('크기는 KB 올림·천 단위 쉼표, 값이 없으면 빈 칸', () {
      expect(formatLibrarySize(null), '');
      expect(formatLibrarySize(0), '0 KB');
      expect(formatLibrarySize(1), '1 KB');
      expect(formatLibrarySize(1024), '1 KB');
      expect(formatLibrarySize(1025), '2 KB');
      expect(formatLibrarySize(5 * 1024 * 1024), '5,120 KB');
      expect(formatLibraryDate(DateTime(2026, 3, 4, 5, 6)), '2026-03-04 05:06');
    });

    test('종류 글자와 정렬(빈 칸은 방향과 관계없이 뒤로)', () {
      const a = LibraryItem(id: '1', boxId: 'b', kind: LibraryItemKind.file, title: 'B', space: LibrarySpace.core, x: 0, y: 0, source: 'C:/x/b.hwpx');
      const b = LibraryItem(id: '2', boxId: 'b', kind: LibraryItemKind.file, title: 'a', space: LibrarySpace.core, x: 0, y: 0, source: 'C:/x/a.pdf');
      const c = LibraryItem(id: '3', boxId: 'b', kind: LibraryItemKind.note, title: 'c', space: LibrarySpace.core, x: 0, y: 0);
      expect(libraryKindLabel(a), 'HWPX 파일');
      expect(libraryKindLabel(c), '메모');
      final info = {'1': const LibraryFileInfo(size: 10), '2': const LibraryFileInfo(size: 5)};
      List<String> ids(LibraryExplorerColumn col, bool asc) =>
          sortLibraryExplorerItems([a, b, c], col, asc, info: info).map((i) => i.id).toList();
      expect(ids(LibraryExplorerColumn.name, true), ['2', '1', '3']);
      expect(ids(LibraryExplorerColumn.name, false), ['3', '1', '2']);
      expect(ids(LibraryExplorerColumn.size, true), ['2', '1', '3']);
      expect(ids(LibraryExplorerColumn.size, false), ['1', '2', '3']);
    });

    test('LibraryFileInfo는 klib 브리지 응답을 읽는다', () {
      final info = LibraryFileInfo.fromJson({'exists': false, 'is_dir': false, 'size': 12, 'mtime': 1700000000.5});
      expect(info.missing, isTrue);
      expect(info.size, 12);
      expect(info.modified, DateTime.fromMillisecondsSinceEpoch(1700000000500));
      expect(const LibraryFileInfo().missing, isFalse);
    });
  });

  group('보기 전환', () {
    testWidgets('기본은 캔버스, 탐색기 형식으로 바꾸면 저장되고 다시 열 때 복원된다', (tester) async {
      final (api, _, _) = await _sample();
      final prefs = MemoryLibraryViewPrefs();
      await _pump(tester, api, prefs: prefs);
      expect(find.byKey(const ValueKey('librarySpace-core')), findsOneWidget);
      expect(find.byKey(const ValueKey('libraryExplorer')), findsNothing);

      await _toExplorer(tester);
      expect(find.byKey(const ValueKey('libraryExplorer')), findsOneWidget);
      expect(find.byKey(const ValueKey('librarySpace-core')), findsNothing);
      expect(prefs.values[libraryViewModeKey], 'explorer');

      await tester.pumpWidget(const SizedBox());
      await _pump(tester, api, prefs: prefs);
      expect(find.byKey(const ValueKey('libraryExplorer')), findsOneWidget);

      await tester.tap(find.text('캔버스'));
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('librarySpace-core')), findsOneWidget);
      expect(prefs.values[libraryViewModeKey], 'canvas');
    });

    testWidgets('저장된 값이 이상하면 캔버스로 연다', (tester) async {
      final (api, _, _) = await _sample();
      await _pump(tester, api, prefs: MemoryLibraryViewPrefs({libraryViewModeKey: '???'}));
      expect(find.byKey(const ValueKey('librarySpace-core')), findsOneWidget);
    });

    testWidgets('보기를 오가도 선택과 자료는 그대로이고 좌표는 바뀌지 않는다', (tester) async {
      final (api, box, items) = await _sample();
      await _pump(tester, api);
      await tester.tap(find.byKey(ValueKey('libraryItem-${items[0].id}')));
      await tester.pump(const Duration(milliseconds: 400));
      await tester.pumpAndSettle();
      expect(find.text('1개 선택'), findsOneWidget);

      await _toExplorer(tester);
      expect(find.text('1개 선택'), findsOneWidget);
      await tester.tap(_tree(LibrarySpace.core)); // 트리로 열면 선택이 그대로다.
      await tester.pumpAndSettle();
      expect(_status(tester), contains('1개 항목 선택함'));

      await tester.tap(find.text('캔버스'));
      await tester.pumpAndSettle();
      expect(find.text('1개 선택'), findsOneWidget);
      final stored = await api.listItems(box.id);
      for (final item in items) {
        final now = stored.firstWhere((i) => i.id == item.id);
        expect((now.space, now.x, now.y), (item.space, item.x, item.y));
      }
    });
  });

  group('탐색기 형식 구성', () {
    testWidgets('트리·주소 표시줄·뒤로/앞으로/위로로 영역 폴더를 오간다', (tester) async {
      final (api, _, items) = await _sample();
      await _pump(tester, api, prefs: MemoryLibraryViewPrefs({libraryViewModeKey: 'explorer'}));
      // 맨 위: 4개 영역 폴더와 항목 수.
      for (final s in LibrarySpace.values) {
        expect(_folderRow(s), findsOneWidget);
      }
      expect(find.byKey(const ValueKey('libraryExplorerCell-x-kind')), findsNothing);
      expect(_status(tester), '항목 4개');

      await _doubleClick(tester, _folderRow(LibrarySpace.core));
      expect(_row(items[0].id), findsOneWidget);
      expect(_row(items[3].id), findsNothing);
      expect(find.byKey(const ValueKey('libraryExplorerCrumb-core')), findsOneWidget);
      expect(_status(tester), '항목 3개');

      await tester.tap(_tree(LibrarySpace.basis));
      await tester.pumpAndSettle();
      expect(_row(items[3].id), findsOneWidget);
      expect(_row(items[0].id), findsNothing);

      await tester.tap(find.byKey(const ValueKey('libraryExplorerBack')));
      await tester.pumpAndSettle();
      expect(_row(items[0].id), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('libraryExplorerForward')));
      await tester.pumpAndSettle();
      expect(_row(items[3].id), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('libraryExplorerUp')));
      await tester.pumpAndSettle();
      expect(_folderRow(LibrarySpace.core), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('libraryExplorerTree-root')));
      await tester.pumpAndSettle();
      expect(_folderRow(LibrarySpace.temp), findsOneWidget);
    });

    testWidgets('자세히 보기 열 값: 종류·크기·수정한 날짜·원본 경로·영역, 원본 없음 표시, 보이는 파일만 조회', (tester) async {
      final (api, _, items) = await _sample();
      final info = _FakeInfo({
        items[0].id: LibraryFileInfo(exists: true, size: 2048, modified: DateTime(2026, 11, 1, 9, 30)),
        items[1].id: const LibraryFileInfo(exists: false),
        items[2].id: LibraryFileInfo(exists: true, size: 100, modified: DateTime(2026, 1, 2, 3, 4)),
      });
      await _pump(tester, api, prefs: MemoryLibraryViewPrefs({libraryViewModeKey: 'explorer'}), info: info);
      expect(info.asked, isEmpty); // 맨 위(폴더만)에서는 묻지 않는다.
      await _doubleClick(tester, _folderRow(LibrarySpace.core));
      String cell(String id, String col) =>
          tester.widget<Text>(find.byKey(ValueKey('libraryExplorerCell-$id-$col'))).data!;
      expect(cell(items[0].id, 'kind'), 'HWPX 파일');
      expect(cell(items[0].id, 'size'), '2 KB');
      expect(cell(items[0].id, 'modified'), '2026-11-01 09:30');
      expect(cell(items[0].id, 'source'), 'C:/자료/나 보고서.hwpx');
      expect(cell(items[0].id, 'space'), '핵심자료');
      expect(cell(items[1].id, 'size'), '');
      expect(find.byKey(ValueKey('libraryExplorerMissing-${items[1].id}')), findsOneWidget);
      expect(find.byKey(ValueKey('libraryExplorerMissing-${items[0].id}')), findsNothing);

      await tester.tap(_tree(LibrarySpace.basis));
      await tester.pumpAndSettle();
      expect(cell(items[3].id, 'kind'), '웹 주소');
      expect(cell(items[4].id, 'kind'), '텍스트');
      // 웹 주소·메모는 파일 정보를 묻지 않는다.
      expect(info.askedIds, {items[0].id, items[1].id, items[2].id});
    });

    testWidgets('열 머리글을 누르면 정렬하고 다시 누르면 반대로, 열 폭을 끌어 바꾼다', (tester) async {
      final (api, _, items) = await _sample();
      final info = _FakeInfo({
        items[0].id: const LibraryFileInfo(exists: true, size: 300),
        items[1].id: const LibraryFileInfo(exists: true, size: 100),
        items[2].id: const LibraryFileInfo(exists: true, size: 200),
      });
      await _pump(tester, api, prefs: MemoryLibraryViewPrefs({libraryViewModeKey: 'explorer'}), info: info);
      await _doubleClick(tester, _folderRow(LibrarySpace.core));
      List<String> order() {
        final ids = [items[0].id, items[1].id, items[2].id];
        ids.sort((a, b) => tester.getTopLeft(_row(a)).dy.compareTo(tester.getTopLeft(_row(b)).dy));
        return ids;
      }

      expect(order(), [items[1].id, items[0].id, items[2].id]); // 이름: 가·나·다
      await tester.tap(find.byKey(const ValueKey('libraryExplorerHeader-name')));
      await tester.pumpAndSettle();
      expect(order(), [items[2].id, items[0].id, items[1].id]);
      await tester.tap(find.byKey(const ValueKey('libraryExplorerHeader-size')));
      await tester.pumpAndSettle();
      expect(order(), [items[1].id, items[2].id, items[0].id]);
      await tester.tap(find.byKey(const ValueKey('libraryExplorerHeader-size')));
      await tester.pumpAndSettle();
      expect(order(), [items[0].id, items[2].id, items[1].id]);

      final before = tester.getSize(find.byKey(ValueKey('libraryExplorerCell-${items[0].id}-kind'))).width;
      await tester.drag(find.byKey(const ValueKey('libraryExplorerResize-kind')), const Offset(60, 0));
      await tester.pumpAndSettle();
      final after = tester.getSize(find.byKey(ValueKey('libraryExplorerCell-${items[0].id}-kind'))).width;
      expect(after, greaterThan(before + 40));
    });

    testWidgets('보기 방식(목록·아이콘)을 바꾸고 그 선택도 저장한다', (tester) async {
      final (api, _, items) = await _sample();
      final prefs = MemoryLibraryViewPrefs({libraryViewModeKey: 'explorer'});
      await _pump(tester, api, prefs: prefs);
      await _doubleClick(tester, _folderRow(LibrarySpace.core));
      for (final style in [LibraryExplorerStyle.list, LibraryExplorerStyle.largeIcons, LibraryExplorerStyle.mediumIcons, LibraryExplorerStyle.smallIcons]) {
        await tester.tap(find.byKey(const ValueKey('libraryExplorerStyle')));
        await tester.pumpAndSettle();
        await tester.tap(find.byKey(ValueKey('libraryExplorerStyle-${style.name}')));
        await tester.pumpAndSettle();
        expect(_row(items[0].id), findsOneWidget, reason: style.name);
        expect(find.byKey(const ValueKey('libraryExplorerHeader-name')), findsNothing);
        expect(prefs.values[libraryExplorerStyleKey], style.name);
      }
      await tester.pumpWidget(const SizedBox());
      await _pump(tester, api, prefs: prefs);
      expect(find.text(libraryExplorerStyleNames[LibraryExplorerStyle.smallIcons]!), findsOneWidget);
    });
  });

  group('조작', () {
    testWidgets('클릭·Ctrl+클릭·Shift+클릭 선택, 빈 곳 클릭은 해제, 상태 표시줄 개수', (tester) async {
      final (api, _, items) = await _sample();
      await _pump(tester, api, prefs: MemoryLibraryViewPrefs({libraryViewModeKey: 'explorer'}));
      await _doubleClick(tester, _folderRow(LibrarySpace.core));
      // 이름순: 가(1)·나(0)·다(2)
      await _click(tester, _row(items[1].id));
      expect(_status(tester), '항목 3개    1개 항목 선택함');
      await _click(tester, _row(items[2].id), shift: true);
      expect(_status(tester), contains('3개 항목 선택함'));
      await _click(tester, _row(items[0].id), ctrl: true);
      expect(_status(tester), contains('2개 항목 선택함'));
      expect(find.text('2개 선택'), findsOneWidget); // 선택 표시줄도 같은 선택
      await _click(tester, _row(items[2].id));
      expect(_status(tester), contains('1개 항목 선택함'));
      await tester.tapAt(tester.getBottomLeft(find.byKey(const ValueKey('libraryExplorerList'))) + const Offset(300, -20));
      await tester.pumpAndSettle();
      expect(_status(tester), '항목 3개');
    });

    testWidgets('더블클릭·Enter로 연결 앱으로 열고, 화살표로 이동하며 F2로 이름 바꾸기', (tester) async {
      final (api, _, items) = await _sample();
      final opener = _FakeOpener();
      await _pump(tester, api, prefs: MemoryLibraryViewPrefs({libraryViewModeKey: 'explorer'}), opener: opener);
      await _doubleClick(tester, _folderRow(LibrarySpace.core));
      await _doubleClick(tester, _row(items[0].id));
      expect(opener.opened, ['C:/자료/나 보고서.hwpx']);

      await tester.runAsync(() => Future<void>.delayed(const Duration(milliseconds: 520)));
      await _click(tester, _row(items[1].id));
      await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
      await tester.pumpAndSettle();
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pumpAndSettle();
      expect(opener.opened.last, 'C:/자료/나 보고서.hwpx');
      await tester.sendKeyEvent(LogicalKeyboardKey.end);
      await tester.pumpAndSettle();
      await tester.sendKeyEvent(LogicalKeyboardKey.f2);
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('libraryRenameItemInput')), findsOneWidget);
      await tester.enterText(find.byKey(const ValueKey('libraryRenameItemInput')), '라 붙임.xlsx');
      await tester.tap(find.text('저장'));
      await tester.pumpAndSettle();
      expect(find.text('라 붙임.xlsx'), findsOneWidget);
      // Backspace는 뒤로(맨 위로) 간다.
      await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
      await tester.pumpAndSettle();
      expect(_folderRow(LibrarySpace.core), findsOneWidget);
    });

    testWidgets('Delete는 기존과 같이 확인한 뒤 고른 자료를 삭제한다', (tester) async {
      final (api, box, items) = await _sample();
      await _pump(tester, api, prefs: MemoryLibraryViewPrefs({libraryViewModeKey: 'explorer'}));
      await _doubleClick(tester, _folderRow(LibrarySpace.core));
      await _click(tester, _row(items[1].id));
      await _click(tester, _row(items[0].id), ctrl: true);
      await tester.sendKeyEvent(LogicalKeyboardKey.delete);
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const ValueKey('libraryConfirmBatchDeleteYes')));
      await tester.pumpAndSettle();
      expect((await api.listItems(box.id)).map((i) => i.id), isNot(contains(items[0].id)));
      expect(_status(tester), '항목 1개');
    });

    testWidgets('트리 폴더로 끌어 놓으면 이동, Ctrl을 누르고 놓으면 복사', (tester) async {
      final (api, box, items) = await _sample();
      await _pump(tester, api, prefs: MemoryLibraryViewPrefs({libraryViewModeKey: 'explorer'}));
      await _doubleClick(tester, _folderRow(LibrarySpace.core));

      Future<void> dragTo(Finder from, Finder to, {bool ctrl = false}) async {
        final gesture = await tester.startGesture(tester.getCenter(from));
        await tester.pump();
        await gesture.moveBy(const Offset(20, 0));
        await tester.pump();
        await gesture.moveTo(tester.getCenter(to));
        await tester.pump();
        if (ctrl) await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
        await gesture.up();
        await tester.pumpAndSettle();
        if (ctrl) await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
        await tester.pumpAndSettle();
      }

      await dragTo(_row(items[0].id), _tree(LibrarySpace.reference));
      var stored = await api.listItems(box.id);
      expect(stored.firstWhere((i) => i.id == items[0].id).space, LibrarySpace.reference);
      expect(_row(items[0].id), findsNothing);

      await tester.runAsync(() => Future<void>.delayed(const Duration(milliseconds: 520)));
      await dragTo(_row(items[1].id), _tree(LibrarySpace.temp), ctrl: true);
      stored = await api.listItems(box.id);
      expect(stored.firstWhere((i) => i.id == items[1].id).space, LibrarySpace.core);
      expect(stored.where((i) => i.space == LibrarySpace.temp).map((i) => i.title), ['가 계획.pdf (사본)']);
    });

    testWidgets('오른쪽 클릭 메뉴로 다른 영역으로 옮긴다', (tester) async {
      final (api, box, items) = await _sample();
      await _pump(tester, api, prefs: MemoryLibraryViewPrefs({libraryViewModeKey: 'explorer'}));
      await _doubleClick(tester, _folderRow(LibrarySpace.core));
      await tester.tap(_row(items[2].id), buttons: kSecondaryButton);
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const ValueKey('libraryExplorerMenuMove-basis')));
      await tester.pumpAndSettle();
      final stored = await api.listItems(box.id);
      expect(stored.firstWhere((i) => i.id == items[2].id).space, LibrarySpace.basis);
    });

    testWidgets('창에 놓은 파일은 지금 연 영역 폴더에 들어간다', (tester) async {
      final api = _DropApi();
      final box = await api.createBox('놓기');
      void Function()? notify;
      tester.view.physicalSize = const Size(1500, 900);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      await tester.pumpWidget(MaterialApp(home: LibraryScreen(
        api: api,
        viewPrefs: MemoryLibraryViewPrefs({libraryViewModeKey: 'explorer'}),
        watchDropEvents: (cb) {
          notify = cb;
          return () {};
        },
      )));
      await tester.pumpAndSettle();
      await tester.tap(_tree(LibrarySpace.reference));
      await tester.pumpAndSettle();
      api.pending = ['C:/받은 파일/회의록.hwpx'];
      notify!();
      await tester.pumpAndSettle();
      final stored = await api.listItems(box.id);
      expect(stored.single.space, LibrarySpace.reference);
      expect(stored.single.title, '회의록.hwpx');
    });
  });

  testWidgets('수천 개 자료도 보이는 줄만 만들고 조회한다', (tester) async {
    final api = InMemoryLibraryApi();
    final box = await api.createBox('대량');
    for (var i = 0; i < 3000; i++) {
      await api.addItem(boxId: box.id, kind: LibraryItemKind.file, title: '파일 ${i.toString().padLeft(4, '0')}.hwpx',
          // 캔버스 겹침 정리가 일을 하지 않도록 겹치지 않는 격자 좌표에 둔다.
          space: LibrarySpace.core, x: 16 + (i % 50) * 150.0, y: 16 + (i ~/ 50) * 80.0, source: 'C:/대량/$i.hwpx');
    }
    final info = _FakeInfo({});
    await _pump(tester, api, prefs: MemoryLibraryViewPrefs({libraryViewModeKey: 'explorer'}), info: info);
    await _doubleClick(tester, _folderRow(LibrarySpace.core));
    expect(_status(tester), '항목 3000개');
    expect(find.text('파일 0000.hwpx'), findsOneWidget);
    expect(find.text('파일 2999.hwpx'), findsNothing);
    final built = find.byWidgetPredicate((w) => w.key is ValueKey<String> && (w.key as ValueKey<String>).value.startsWith('libraryExplorerRow-'));
    expect(built.evaluate().length, lessThan(100));
    expect(info.askedIds.length, lessThan(100));
    await tester.sendKeyEvent(LogicalKeyboardKey.end);
    await tester.pumpAndSettle();
    expect(find.text('파일 2999.hwpx'), findsOneWidget);
  });
}

/// 창 놓기 알림을 흉내 내는 API.
class _DropApi extends InMemoryLibraryApi implements LibraryDropApi {
  List<String> pending = [];

  @override
  Future<List<String>> takeDroppedPaths() async {
    final paths = pending;
    pending = [];
    return paths;
  }

  @override
  Future<void> watchDrops(bool active) async {}
}
