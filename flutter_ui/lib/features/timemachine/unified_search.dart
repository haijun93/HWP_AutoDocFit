/// 몽땅검색 통합 보기(UNIFIEDAPP1): 파일명 > 위계 > 본문, 같은 문서는 가장 높은 단에 한 번.
///
/// 검색·단 배정·0건 안내 문구는 호스트(ktm)가 정하고, 화면은 가장 최근 응답을 조회해 그린다.
library;

import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'timemachine_api.dart';
import 'timemachine_colors.dart';

@immutable
class UnifiedRow {
  const UnifiedRow({required this.path, required this.name, required this.parent, required this.tier,
    required this.location, required this.snippet});
  final String path, name, parent, tier, location, snippet;

  factory UnifiedRow.fromJson(Map value) => UnifiedRow(
        path: '${value['path'] ?? ''}',
        name: '${value['name'] ?? ''}',
        parent: '${value['parent'] ?? ''}',
        tier: '${value['tier'] ?? ''}',
        location: '${value['location'] ?? ''}',
        snippet: '${value['snippet'] ?? ''}',
      );
}

class UnifiedSearchController extends ChangeNotifier {
  UnifiedSearchController(this.api);
  final TimemachineApi api;

  /// 첫 1초는 촘촘히, 그 뒤에는 느리게 조회한다. 검색 중이 아니면 조회하지 않는다.
  static const fastPoll = Duration(milliseconds: 30), slowPoll = Duration(milliseconds: 200);

  List<UnifiedRow> rows = const [];
  List<String> terms = const [];
  List<String> errors = const [];
  List<int>? totals;
  String? noticeTitle, noticeDetail;
  String summary = '';
  bool searching = false, paging = false, cancelled = false;

  int? _generation, _next;
  final List<int> _pages = [0];
  int _token = 0;
  Timer? _timer;
  final Stopwatch _clock = Stopwatch();
  bool _disposed = false, _used = false;

  bool get hasNext => !searching && !paging && _next != null;
  bool get hasPrevious => !searching && !paging && _pages.length > 1;
  int get pageNumber => _pages.length;

  void _reset() {
    _token++;
    _timer?.cancel();
    rows = const [];
    terms = const [];
    errors = const [];
    totals = null;
    noticeTitle = noticeDetail = null;
    summary = '';
    searching = paging = cancelled = false;
    _generation = _next = null;
    _pages
      ..clear()
      ..add(0);
  }

  void _notify() {
    if (!_disposed) notifyListeners();
  }

  void _fail(String reason) {
    searching = paging = false;
    summary = '검색 못함';
    noticeTitle = '검색을 완료하지 못했습니다';
    noticeDetail = "0건은 '파일이 없다'는 뜻이 아닙니다. $reason";
    _notify();
  }

  /// 새 검색. 이전 검색은 호스트가 취소하고, 늦게 온 응답은 버린다.
  Future<void> start(String query, Map<String, Object?> filters) async {
    _reset();
    final token = _token;
    _used = true;
    searching = true;
    summary = noticeTitle = '검색 중…';
    _clock
      ..reset()
      ..start();
    _notify();
    Map<String, Object?> response;
    try {
      response = await api.command('unified_start', {'query': query, ...filters});
    } catch (_) {
      if (token == _token) _fail('앱 연결을 확인해 주세요.');
      return;
    }
    if (token != _token || _disposed) return;
    final generation = response['generation'];
    if (response['ok'] != true || generation is! int) {
      _fail('${response['message'] ?? '통합 검색을 시작하지 못했습니다.'}');
      return;
    }
    _generation = generation;
    _schedule(token, Duration.zero);
  }

  void _schedule(int token, Duration delay) {
    _timer?.cancel();
    _timer = Timer(delay, () => unawaited(_poll(token)));
  }

  Future<void> _poll(int token) async {
    if (token != _token || _disposed) return;
    Map<String, Object?> response;
    try {
      response = await api.command('unified_poll', {'generation': _generation});
    } catch (_) {
      if (token == _token) _fail('앱 연결을 확인해 주세요.');
      return;
    }
    if (token != _token || _disposed) return;
    if (response['ok'] != true) {
      _fail('${response['message'] ?? '검색 상태를 읽지 못했습니다.'}');
      return;
    }
    final result = response['result'];
    if (result is Map) {
      if (result['event'] == 'stale') {
        _fail('다른 검색이 시작되었습니다. 다시 검색해 주세요.');
        return;
      }
      _apply(result);
      if (result['event'] != 'part') return;
    }
    _schedule(token, _clock.elapsed < const Duration(seconds: 1) ? fastPoll : slowPoll);
  }

  void _apply(Map result) {
    final event = result['event'];
    final answer = result['answer'];
    if (answer is Map) {
      rows = [
        for (final raw in (answer['rows'] as List? ?? const []))
          if (raw is Map) UnifiedRow.fromJson(raw),
      ];
      terms = [for (final term in (answer['terms'] as List? ?? const [])) '$term'];
    } else if (event == 'error') {
      rows = const [];
    }
    errors = [for (final error in (result['errors'] as List? ?? const [])) '$error'];
    final notice = result['notice'];
    noticeTitle = notice is Map ? '${notice['title']}' : null;
    noticeDetail = notice is Map ? '${notice['detail']}' : null;
    summary = '${result['summary'] ?? ''}';
    searching = event == 'part';
    if (event == 'page') {
      final raw = result['totals'];
      totals = raw is List ? [for (final n in raw) (n as num).toInt()] : null;
      _next = (result['next_cursor'] as num?)?.toInt();
    }
    _notify();
  }

  Future<void> nextPage() async {
    final next = _next;
    if (!hasNext || next == null) return;
    await _page(next, () => _pages.add(next));
  }

  Future<void> previousPage() async {
    if (!hasPrevious) return;
    await _page(_pages[_pages.length - 2], () => _pages.removeLast());
  }

  Future<void> _page(int cursor, VoidCallback moved) async {
    final token = _token;
    paging = true;
    _notify();
    Map<String, Object?> response;
    try {
      response = await api.command('unified_page', {'generation': _generation, 'cursor': cursor});
    } catch (_) {
      response = {'ok': false, 'message': '앱 연결을 확인해 주세요.'};
    }
    if (token != _token || _disposed) return;
    paging = false;
    final result = response['result'];
    if (response['ok'] == true && result is Map && result['event'] == 'page') {
      moved();
      _apply(result);
    } else {
      // 보이는 페이지는 남기고 실패만 알린다.
      final message = result is Map ? result['message'] : response['message'];
      errors = ['${message ?? '다음 결과를 받지 못했습니다. 다시 검색해 주세요.'}'];
      _notify();
    }
  }

  /// 진행 중 검색을 멈춘다. 이미 받은 행은 남긴다.
  Future<void> cancel() async {
    if (!searching) return;
    final generation = _generation;
    _token++;
    _timer?.cancel();
    searching = false;
    cancelled = true;
    summary = '검색 취소';
    noticeTitle = '검색을 취소했습니다';
    noticeDetail = '일부만 확인한 상태입니다. Enter를 누르면 다시 검색합니다.';
    _notify();
    if (generation != null) await _send('unified_cancel', {'generation': generation});
  }

  /// 검색어가 비면 세션만 놓는다(호스트는 다음 입력을 위해 남긴다).
  void clear() {
    final searched = _generation != null || searching;
    _reset();
    _notify();
    if (searched) unawaited(_send('unified_release'));
  }

  /// 통합 보기를 닫으면 호스트 프로세스도 끝낸다.
  void close() {
    final used = _used;
    _used = false;
    _reset();
    _notify();
    if (used) unawaited(_send('unified_close'));
  }

  Future<void> _send(String action, [Map<String, Object?> payload = const {}]) async {
    try {
      await api.command(action, payload);
    } catch (_) {
      // 정리 요청 실패는 다음 검색이 새 세션을 만들면 해소된다.
    }
  }

  @override
  void dispose() {
    _disposed = true;
    _token++;
    _timer?.cancel();
    if (_used) unawaited(_send('unified_close'));
    super.dispose();
  }
}

class UnifiedResultsView extends StatelessWidget {
  const UnifiedResultsView({super.key, required this.controller, required this.marked, required this.onPreview,
    required this.onOpen, required this.onReveal, this.selectedPath});

  final UnifiedSearchController controller;
  final Widget Function(String text, Iterable<String> terms, {int? maxLines, TextStyle? style}) marked;
  final ValueChanged<UnifiedRow> onPreview, onOpen, onReveal;
  final String? selectedPath;

  @override
  Widget build(BuildContext context) => CallbackShortcuts(
        bindings: {
          const SingleActivator(LogicalKeyboardKey.pageDown, control: true): () => unawaited(controller.nextPage()),
          const SingleActivator(LogicalKeyboardKey.pageUp, control: true): () => unawaited(controller.previousPage()),
          const SingleActivator(LogicalKeyboardKey.escape): () => unawaited(controller.cancel()),
        },
        child: Focus(
          child: ListenableBuilder(listenable: controller, builder: (context, _) => _body(context)),
        ),
      );

  Widget _body(BuildContext context) {
    final theme = Theme.of(context);
    final totals = controller.totals;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 2, 12, 2),
          child: Wrap(spacing: 12, runSpacing: 4, crossAxisAlignment: WrapCrossAlignment.center, children: [
            Text(controller.summary, key: const ValueKey('tm-unified-summary'),
                style: const TextStyle(fontSize: 12, fontWeight: FontWeight.bold)),
            // 검색하지 못한 것을 '0 · 0 · 0'으로 적지 않는다. 일부 실패면 확인한 수임을 밝힌다.
            if (totals != null && totals.length == 3 && controller.summary != '검색 못함')
              Text('${controller.errors.isEmpty ? '' : '확인한 결과 · '}파일명 ${totals[0]} · 위계 ${totals[1]} · 본문 ${totals[2]}',
                  key: const ValueKey('tm-unified-totals'), style: const TextStyle(fontSize: 12)),
            if (controller.searching) ...[
              const SizedBox(width: 60, child: LinearProgressIndicator()),
              TextButton(key: const ValueKey('tm-unified-cancel'), onPressed: controller.cancel, child: const Text('취소')),
            ],
          ]),
        ),
        // 일부 원천만 실패하면 확인한 결과를 두고 사유를 함께 보인다.
        if (controller.errors.isNotEmpty && controller.rows.isNotEmpty)
          Padding(
            key: const ValueKey('tm-unified-partial'),
            padding: const EdgeInsets.fromLTRB(16, 0, 16, 4),
            child: Text('일부만 검색했습니다. ${controller.errors.join(' / ')}',
                style: TextStyle(fontSize: 12, color: TimemachineColors.of(context).error)),
          ),
        Expanded(
          child: controller.rows.isEmpty
              ? Center(
                  child: SingleChildScrollView(
                    padding: const EdgeInsets.all(24),
                    child: Column(mainAxisSize: MainAxisSize.min, children: [
                      Text(controller.noticeTitle ?? '', key: const ValueKey('tm-unified-notice'),
                          textAlign: TextAlign.center, style: theme.textTheme.titleMedium),
                      if ((controller.noticeDetail ?? '').isNotEmpty) ...[
                        const SizedBox(height: 8),
                        Text(controller.noticeDetail!, textAlign: TextAlign.center, style: theme.textTheme.bodySmall),
                      ],
                    ]),
                  ),
                )
              : ListView.builder(
                  // 페이지가 바뀌면 처음부터 보인다.
                  key: ValueKey('tm-unified-list-${controller.pageNumber}'),
                  itemCount: controller.rows.length,
                  itemBuilder: (context, index) => _row(context, controller.rows[index], index),
                ),
        ),
        if (controller.hasNext || controller.hasPrevious || controller.paging)
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 4),
            child: Row(mainAxisAlignment: MainAxisAlignment.center, children: [
              TextButton.icon(
                key: const ValueKey('tm-unified-previous'),
                onPressed: controller.hasPrevious ? controller.previousPage : null,
                icon: const Icon(Icons.chevron_left, size: 18),
                label: const Text('이전'),
              ),
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: 12),
                child: Text('${controller.pageNumber}쪽', style: const TextStyle(fontSize: 12)),
              ),
              TextButton.icon(
                key: const ValueKey('tm-unified-next'),
                onPressed: controller.hasNext ? controller.nextPage : null,
                icon: const Icon(Icons.chevron_right, size: 18),
                label: const Text('이어보기'),
              ),
            ]),
          ),
      ],
    );
  }

  Widget _row(BuildContext context, UnifiedRow row, int index) {
    final theme = Theme.of(context);
    final scheme = theme.colorScheme;
    // 단이 바뀌는 곳에 머리글을 둔다.
    final heading = index == 0 || controller.rows[index - 1].tier != row.tier;
    final detail = [row.parent, if (row.location.isNotEmpty) row.location].join(' · ');
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      children: [
        if (heading)
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 8, 16, 2),
            child: Text('${row.tier} 일치', key: ValueKey('tm-unified-tier-${row.tier}'),
                style: theme.textTheme.titleSmall),
          ),
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 2),
          child: Card(
            color: selectedPath == row.path ? scheme.primaryContainer.withValues(alpha: 0.25) : null,
            // 단추는 더블클릭 영역 밖에 둔다(안에 두면 누른 뒤 더블클릭 대기만큼 늦게 반응한다).
            child: Row(children: [
              Expanded(
                child: InkWell(
                  key: ValueKey('tm-unified-row-${row.path}'),
                  onTap: () => onPreview(row),
                  onDoubleTap: () => onOpen(row),
                  child: Padding(
                    padding: const EdgeInsets.fromLTRB(12, 8, 4, 8),
                    child: Row(children: [
                      Container(
                        width: 48,
                        padding: const EdgeInsets.symmetric(vertical: 2),
                        alignment: Alignment.center,
                        decoration: BoxDecoration(color: scheme.secondaryContainer, borderRadius: BorderRadius.circular(8)),
                        child: Text(row.tier, style: theme.textTheme.labelSmall?.copyWith(color: scheme.onSecondaryContainer)),
                      ),
                      const SizedBox(width: 10),
                      Expanded(
                        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                          marked(row.name, controller.terms, maxLines: 1,
                              style: const TextStyle(fontWeight: FontWeight.bold, fontSize: 14)),
                          Text(detail, maxLines: 1, overflow: TextOverflow.ellipsis, style: theme.textTheme.bodySmall),
                          if (row.snippet.isNotEmpty) ...[
                            const SizedBox(height: 2),
                            marked(row.snippet, controller.terms, maxLines: 2, style: theme.textTheme.bodyMedium),
                          ],
                        ]),
                      ),
                    ]),
                  ),
                ),
              ),
              IconButton(
                key: ValueKey('tm-unified-open-${row.path}'),
                tooltip: '문서 열기',
                icon: const Icon(Icons.open_in_new, size: 18),
                onPressed: () => onOpen(row),
              ),
              IconButton(
                key: ValueKey('tm-unified-reveal-${row.path}'),
                tooltip: '폴더 열기',
                icon: const Icon(Icons.folder_open, size: 18),
                onPressed: () => onReveal(row),
              ),
              const SizedBox(width: 4),
            ]),
          ),
        ),
      ],
    );
  }
}
