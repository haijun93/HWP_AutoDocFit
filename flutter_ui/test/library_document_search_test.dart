import 'package:flutter_test/flutter_test.dart';
import 'package:docfit_ui/features/library/library_document_search.dart';

void main() {
  test('ISUI2: 가상 폴더 본문·위계검색은 타임머신 기존 명령에 검색어만 보낸다', () async {
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
}
