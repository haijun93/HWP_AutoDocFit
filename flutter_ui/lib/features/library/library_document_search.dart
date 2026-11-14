/// 가상 폴더 검색창의 본문검색·위계검색(ISUI2).
///
/// 새 백엔드 명령을 만들지 않고 몽땅검색의 기존 명령을 그대로 쓴다.
/// - 본문검색: `timemachine` → `content_search` {query, limit, sort: relevance}
/// - 위계검색: `timemachine` → `related_search` {query, limit, sort: hierarchy_relevance}(검색어만 보내는 방식)
/// 결과는 몽땅검색 조사 폴더 전체에서 오므로, 가상 폴더 화면이 자기 자료(원본 경로·복사본 경로)와 맞는 것만 남긴다.
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

/// 몽땅검색 명령으로 본문·위계검색을 하는 기본 구현.
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

/// 서버가 상자 범위로 검색한 결과. 자료 ID는 경로 없는 메모도 식별한다.
class ScopedLibraryHits {
  const ScopedLibraryHits({required this.rows, required this.totalDocs,
    required this.totalSentences, required this.engine, this.reason, this.countExact = true});
  final List<Map> rows;
  final int totalDocs;
  final int totalSentences;
  final String engine;
  final String? reason;
  final bool countExact;

  String get countLabel => countExact ? '$totalDocs개 · $totalSentences문장' : '표시 $totalDocs개';
}

typedef ScopedLibraryDocumentSearch = Future<ScopedLibraryHits> Function(
  String boxId, String mode, String query);

ScopedLibraryDocumentSearch scopedLibraryDocumentSearch(ApiCall call, {int limit = 200}) {
  return (boxId, mode, query) async {
    final payload = <String, Object?>{'cmd': 'search', 'box_id': int.parse(boxId),
      'query': query, 'limit': limit, 'mode': mode == 'related' ? 'hierarchy' : 'keyword'};
    Future<Map> request(Map<String, Object?> data) async {
      final response = await call('library', [data]);
      if (response is! Map || response['ok'] != true) {
        throw Exception(response is Map ? response['message'] ?? '검색에 실패했습니다.' : '검색 응답을 확인해 주세요.');
      }
      return response;
    }
    final response = await request(payload);
    final count = response['engine'] == 'legacy' ? response : await request({...payload, 'count_only': true});
    // 세대가 다른 count는 사용하지 않는다. 다음 입력이 새 결과를 요청한다.
    final sameGeneration = (response['generation'] ?? response['gen']) == (count['generation'] ?? count['gen']) && response['engine'] == count['engine'];
    final totals = sameGeneration ? count : response;
    return ScopedLibraryHits(rows: [for (final row in response['results'] as List? ?? const []) if (row is Map) row],
      totalDocs: (totals['total_docs'] as num?)?.toInt() ?? 0,
      totalSentences: (totals['total_sentences'] as num?)?.toInt() ?? 0,
      engine: '${response['engine'] ?? 'legacy'}', reason: response['engine_reason'] as String?,
      countExact: totals['count_exact'] != false);
  };
}
