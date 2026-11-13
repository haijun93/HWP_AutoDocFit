import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:docfit_ui/features/library/library_api.dart';
import 'package:docfit_ui/features/library/library_document_search.dart';
import 'package:docfit_ui/features/library/library_screen.dart';

void main() {
  test('상자 검색과 count는 같은 서버 범위와 검색 모드를 보낸다', () async {
    final requests = <Map>[];
    final search = scopedLibraryDocumentSearch((method, args) async {
      expect(method, 'library');
      final request = args.single as Map;
      requests.add(request);
      return {'ok': true, 'gen': 4, 'engine': 'kfind', 'total_docs': 9,
        'total_sentences': 12, 'results': [{'path': '', 'item_ids': [7], 'title': '메모', 'snippet': '예산'}]};
    });
    final result = await search('2', 'related', '예산');
    expect(requests.length, 2);
    expect(requests.every((r) => r['box_id'] == 2 && r['mode'] == 'hierarchy'), isTrue);
    expect(requests.last['count_only'], isTrue);
    expect(result.rows.single['item_ids'], [7]);
    expect(result.totalDocs, 9);
  });

  test('다른 세대의 count는 검색 결과 수를 덮지 않는다', () async {
    final search = scopedLibraryDocumentSearch((_, args) async {
      final count = (args.single as Map)['count_only'] == true;
      return {'ok': true, 'gen': count ? 5 : 4, 'total_docs': count ? 99 : 2,
        'total_sentences': count ? 100 : 3, 'results': []};
    });
    expect((await search('2', 'content', '예산')).totalDocs, 2);
  });

  test('legacy는 제한된 표시 개수로 안내하고 count를 다시 요청하지 않는다', () async {
    var calls = 0;
    final search = scopedLibraryDocumentSearch((_, args) async {
      calls++;
      return {'ok': true, 'engine': 'legacy', 'engine_reason': 'configured_legacy',
        'total_docs': 2, 'total_sentences': null, 'count_exact': false, 'results': []};
    });
    final result = await search('2', 'content', '예산');
    expect(calls, 1);
    expect(result.countLabel, '표시 2개');
    expect(result.countExact, isFalse);
  });

  testWidgets('경로 없는 메모를 자료 ID로 표시하고 서버 count 배지를 보인다', (tester) async {
    tester.view.physicalSize = const Size(1800, 1000);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    final api = InMemoryLibraryApi();
    final box = await api.createBox('자료');
    await api.addItem(boxId: box.id, kind: LibraryItemKind.text,
      title: '경로 없는 메모', space: LibrarySpace.core, x: 200, y: 16, text: '다른 메모');
    final memo = await api.addItem(boxId: box.id, kind: LibraryItemKind.text,
      title: '경로 없는 메모', space: LibrarySpace.core, x: 16, y: 16, text: '예산 증액');
    await tester.pumpWidget(MaterialApp(home: LibraryScreen(api: api,
      scopedDocumentSearch: (boxId, mode, query) async {
        expect(boxId, box.id);
        return ScopedLibraryHits(rows: [{'path': '', 'title': memo.title,
          'item_ids': [memo.id], 'snippet': '예산 증액'}], totalDocs: 5,
          totalSentences: 8, engine: 'kfind');
      })));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('librarySearchMode-content')));
    await tester.enterText(find.byKey(const ValueKey('librarySearch')), '예산');
    await tester.pump(const Duration(milliseconds: 300));
    await tester.pumpAndSettle();
    expect(find.byKey(ValueKey('libraryItem-${memo.id}')), findsOneWidget);
    expect(find.byKey(const ValueKey('librarySearchCount')), findsOneWidget);
    expect(find.text('5개 · 8문장'), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('librarySearchResult-0')));
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('libraryMemoPage')), findsOneWidget);
    await tester.pageBack();
    await tester.pumpAndSettle();
    final card = find.descendant(of: find.byKey(ValueKey('libraryItem-${memo.id}')), matching: find.byType(Card));
    expect(tester.widget<Card>(card).elevation, 5);
    expect(tester.takeException(), isNull);
  });
}
