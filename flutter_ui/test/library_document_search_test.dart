import 'package:flutter_test/flutter_test.dart';
import 'package:docfit_ui/features/library/library_document_search.dart';

void main() {
  test('ISUI2: 가상 폴더 본문·위계검색은 몽땅검색 기존 명령에 검색어만 보낸다', () async {
    final sent = <(String, List<Object?>)>[];
    Future<Object?> call(String method, List<Object?> args) async {
      sent.add((method, args));
      return {
        'ok': true,
        'items': [
          {'path': r'C:\자료\보고.hwpx', 'name': '보고.hwpx', 'snippet': '예산 증액', 'breadcrumb_text': 'Ⅰ. 개요'},
          {'name': '경로 없음.txt', 'snippet': '버린다'},
        ],
      };
    }

    final search = timemachineLibraryDocumentSearch(call);
    final related = await search('related', '예산');
    expect(sent.last.$1, 'timemachine');
    expect(sent.last.$2, ['related_search', {'query': '예산', 'limit': 200, 'sort': 'hierarchy_relevance'}]);
    expect(related.single.path, r'C:\자료\보고.hwpx');
    expect(related.single.breadcrumb, 'Ⅰ. 개요');

    await search('content', '예산');
    expect(sent.last.$2, ['content_search', {'query': '예산', 'limit': 200, 'sort': 'relevance'}]);
  });

  test('실패 응답은 예외로 알린다', () async {
    final search = timemachineLibraryDocumentSearch((_, _) async => {'ok': false, 'message': '색인이 없습니다.'});
    expect(() => search('content', '예산'), throwsA(isA<Exception>()));
  });

  test('경로 비교는 대소문자·구분자를 무시한다', () {
    expect(normalizeLibraryPath('C:/자료/A.TXT'), normalizeLibraryPath(r'c:\자료\a.txt'));
  });

  test('APP1-H: scopedLibraryDocumentSearch는 서버 범위(scope=box)로 검색하고 count를 합친다', () async {
    final calls = <(String, List<Object?>)>[];
    Future<Object?> call(String method, List<Object?> args) async {
      calls.add((method, args));
      final payload = args.first as Map<String, Object?>;
      if (payload['count_only'] == true) {
        return {
          'ok': true,
          'generation': 3,
          'total_docs': 5,
          'total_sentences': 12,
          'engine': 'kfind',
        };
      }
      return {
        'ok': true,
        'generation': 3,
        'results': [
          {'title': '예산보고.txt', 'snippet': '예산 집행', 'item_ids': [1]},
        ],
        'total_docs': 5,
        'total_sentences': 12,
        'engine': 'kfind',
      };
    }

    final search = scopedLibraryDocumentSearch(call);
    final hits = await search('10', 'content', '예산');

    expect(calls.length, 2);
    expect(calls[0].$1, 'library');
    final queryPayload = calls[0].$2.first as Map;
    expect(queryPayload['cmd'], 'search');
    expect(queryPayload['box_id'], 10);
    expect(queryPayload['query'], '예산');
    expect(queryPayload['mode'], 'keyword');

    final countPayload = calls[1].$2.first as Map;
    expect(countPayload['count_only'], true);

    expect(hits.totalDocs, 5);
    expect(hits.totalSentences, 12);
    expect(hits.engine, 'kfind');
    expect(hits.rows.length, 1);
  });
}
