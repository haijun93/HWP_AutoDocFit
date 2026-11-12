/// 가상 폴더 검색창의 본문검색·위계검색(ISUI2).
///
/// 새 백엔드 명령을 만들지 않고 타임머신의 기존 명령을 그대로 쓴다.
/// - 본문검색: `timemachine` → `content_search` {query, limit, sort: relevance}
/// - 위계검색: `timemachine` → `related_search` {query, limit, sort: hierarchy_relevance}(검색어만 보내는 방식)
/// 결과는 타임머신 조사 폴더 전체에서 오므로, 가상 폴더 화면이 자기 자료(원본 경로·복사본 경로)와 맞는 것만 남긴다.
library;

import '../../core/api/abbreviation_api.dart' show ApiCall;
import '../timemachine/timemachine_api.dart';

/// 한 문서 결과. [path]가 가상 폴더 자료의 원본 경로나 복사본 경로와 같으면 그 자료로 본다.
class LibraryDocumentHit {
  const LibraryDocumentHit({required this.path, required this.name, required this.snippet, this.breadcrumb = ''});
  final String path;
  final String name;
  final String snippet;

  /// 위계검색의 상위 제목 경로(장 > 절 > 항). 없으면 빈 글.
  final String breadcrumb;
}

/// [mode]는 'content'(본문검색) 또는 'related'(위계검색).
typedef LibraryDocumentSearch = Future<List<LibraryDocumentHit>> Function(String mode, String query);

/// 타임머신 명령으로 본문·위계검색을 하는 기본 구현.
LibraryDocumentSearch timemachineLibraryDocumentSearch(ApiCall call, {int limit = 200}) {
  final api = TimemachineApi(call);
  return (mode, query) async {
    final related = mode == 'related';
    final response = await api.command(related ? 'related_search' : 'content_search', {
      'query': query,
      'limit': limit,
      'sort': related ? 'hierarchy_relevance' : 'relevance',
    });
    if (response['ok'] == false) {
      throw Exception(response['message'] ?? (related ? '위계검색에 실패했습니다.' : '본문검색에 실패했습니다.'));
    }
    final raw = response['items'];
    if (raw is! List) return const [];
    final hits = <LibraryDocumentHit>[];
    for (final item in raw) {
      if (item is! Map) continue;
      final result = TimemachineContentResult.fromJson(item, query: query);
      final path = result.path;
      if (path == null || path.isEmpty) continue;
      hits.add(LibraryDocumentHit(path: path, name: result.fileName, snippet: result.snippet, breadcrumb: result.breadcrumbText));
    }
    return hits;
  };
}

/// 윈도 경로 비교용: 대소문자와 구분자 차이를 없앤다.
String normalizeLibraryPath(String path) => path.trim().replaceAll('/', r'\').toLowerCase();
