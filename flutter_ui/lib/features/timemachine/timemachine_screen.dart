import 'dart:async';
import 'dart:math';
import 'package:flutter/material.dart';
import 'package:file_selector/file_selector.dart';
import 'related_documents_dialog.dart';
import 'timemachine_api.dart';
import 'timemachine_thumbnails.dart';
import '../../core/api/abbreviation_api.dart' show ApiCall;
import '../../core/hierarchy_ffi.dart';
import '../../core/instant_search.dart';
import 'idle_indexing_status.dart';

/// 본문·위계검색 한 번의 결과(정렬을 함께 넘겨 화면 정렬 표시와 맞춘다).
typedef _ContentSearchOutcome = ({String sort, List<TimemachineContentResult> items});

class TimemachineScreen extends StatefulWidget {
  const TimemachineScreen({super.key, required this.api});
  final ApiCall api;
  @override
  State<TimemachineScreen> createState() => _TimemachineScreenState();
}

class _TimemachineScreenState extends State<TimemachineScreen> {
  List<Widget> _sectionMatchLabels(List<Map<String, Object?>> sections) {
    final matches = sections.where((section) {
      final parentText = '${section['parent_text'] ?? ''}'.trim();
      final count = int.tryParse('${section['count'] ?? ''}') ?? 0;
      return parentText.isNotEmpty && count >= 2;
    }).take(2);
    return [
      for (final section in matches)
        Padding(
          padding: const EdgeInsets.only(top: 2),
          child: Text(
            '「${'${section['parent_text']}'.trim()}」에서 ${int.parse('${section['count']}')}곳',
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: const TextStyle(fontSize: 11, color: Colors.black54),
          ),
        ),
    ];
  }

  static const _kinds = {'hwpx':'hwp', 'docx':'doc', 'xlsx':'sheet', 'ppt':'slide', 'PDF':'pdf', '이미지':'image'};
  late final TimemachineApi _api = TimemachineApi(widget.api);
  late final TimemachineThumbnailLoader _thumbnails = TimemachineThumbnailLoader(_api);
  bool _watchByAgent = false;
  bool _developerMode = false;
  Timer? _scanPoll;
  Timer? _developerModePoll;
  Map<String, Object?>? _settings;
  Map<String, int> _days = {};
  Map<int, int> _hourDensity = {for (var i = 0; i < 24; i++) i: 0};
  int _densityRevision = 0;
  final Set<String> _expandedHours = {};
  final List<TimemachineItem> _items = [];
  final ScrollController _scroll = ScrollController();
  // 검색창 하나를 파일명·본문검색·위계검색 탭이 함께 쓴다(ISUI1). 탭 이름은 'file'·'content'·'related'.
  late final InstantSearchController<_ContentSearchOutcome?> _searchBox = InstantSearchController(
    initialTab: 'file',
    search: _runSearch,
    onResult: _applySearchResult,
    onCleared: _searchCleared,
    onError: _searchFailed,
  );
  TextEditingController get _search => _searchBox.text;
  final Set<String> _selectedKinds = {};
  String _query = '';
  bool _contentMode = false, _contentIndexRunning = false;
  bool _relatedMode = false;
  Map<String, Object?> _contentProgress = {};
  List<TimemachineContentResult> _contentResults = [];
  bool _contentNeedsReindex = false;
  bool _contentReindexDismissed = false;
  int _contentNeedsReindexCount = 0;
  String? _contentReindexGuide;
  String _contentSort = 'relevance';
  TimemachineContentResult? _selectedContentResult;
  bool _previewLoading = false;
  Map<String, Object?>? _preview;
  int _previewRevision = 0;
  int _contentSearchRevision = 0;
  bool _showOperatorHelp = false;
  final TextEditingController _previewFind = TextEditingController();
  int _previewFindIndex = 0;

  // 파일명 모드 열 보기(테이블) 상태
  bool _fileTableView = false;
  List<TimemachineItem> _fileSearchResults = [];
  bool _fileSearchActive = false;
  bool _fileSearchSupported = true;
  int _fileSearchRevision = 0;
  String _fileSortColumn = 'name';
  bool _fileSortAscending = true;
  final Map<String, double> _fileColumnWidths = {
    'name': 140.0,
    'folder': 200.0,
    'size': 70.0,
    'mtime': 110.0,
    'kind': 60.0,
  };
  Timer? _contentPoll;
  DateTime _month = DateTime(DateTime.now().year, DateTime.now().month);
  DateTime? _start, _end, _firstDay;
  bool _loading = false, _more = true;
  String? _error;
  int _offset = 0;
  int _filterRevision = 0;
  int _requestSerial = 0;
  int _calendarRevision = 0;
  static const _extensions = ['hwpx','hwp','docx','pptx','xlsx','pdf','txt'];

  @override
  void initState() { super.initState(); _load(firstOpen: true); _scroll.addListener(_onScroll); _pollDeveloperMode(); }
  @override
  void dispose() { _scanPoll?.cancel(); _developerModePoll?.cancel(); _contentPoll?.cancel(); _thumbnails.dispose(); _scroll.dispose(); _searchBox.dispose(); _previewFind.dispose(); super.dispose(); }

  void _pollDeveloperMode() {
    _developerModePoll?.cancel();
    _developerModePoll = Timer(const Duration(seconds: 1), () async {
      var enabled = false;
      try {
        final response = await widget.api('get_settings', const []);
        if (response is Map && response['api_version'] == 1 && response['ok'] == true && response['data'] is Map) {
          final values = (response['data'] as Map)['values'];
          enabled = values is Map && values['developer_mode'] == true;
      } else if (response is Map && response['developer_mode'] is bool) {
        enabled = response['developer_mode'] == true;
      }
      } catch (_) {
        enabled = false;
      }
      if (!mounted) return;
      if (enabled != _developerMode) {
        final leaveContent = !enabled && (_contentMode || _relatedMode);
        setState(() {
          _developerMode = enabled;
          if (leaveContent) _resetContentState();
        });
        if (leaveContent) _filtersChanged();
      }
      _pollDeveloperMode();
    });
  }

  /// 검색어와 일치하는 글자의 바탕을 노란색으로 칠한다(대소문자 무시).
  Widget _marked(String text, String query, {int? maxLines, TextStyle? style}) {
    final lower = text.toLowerCase(), key = query.toLowerCase();
    if (key.isEmpty || !lower.contains(key)) {
      return Text(text, maxLines: maxLines, overflow: TextOverflow.ellipsis, style: style);
    }
    final spans = <TextSpan>[];
    var at = 0;
    for (var hit = lower.indexOf(key); hit >= 0; hit = lower.indexOf(key, at)) {
      if (hit > at) spans.add(TextSpan(text: text.substring(at, hit)));
      at = hit + key.length;
      spans.add(TextSpan(
        text: text.substring(hit, at),
        style: const TextStyle(backgroundColor: Color(0xFFFFEB3B), color: Colors.black),
      ));
    }
    if (at < text.length) spans.add(TextSpan(text: text.substring(at)));
    return Text.rich(TextSpan(style: style, children: spans), maxLines: maxLines, overflow: TextOverflow.ellipsis);
  }

  /// 다중 검색어 강조 표시
  Widget _markedTerms(String text, Iterable<String> terms, {int? maxLines, TextStyle? style}) {
    final cleanTerms = terms
        .map((t) => t.trim().toLowerCase())
        .where((t) => t.isNotEmpty)
        .toSet()
        .toList();
    if (cleanTerms.isEmpty || text.isEmpty) {
      return Text(text, maxLines: maxLines, overflow: TextOverflow.ellipsis, style: style);
    }
    final lower = text.toLowerCase();
    final intervals = <(int, int)>[];
    for (final term in cleanTerms) {
      var at = 0;
      while (at < lower.length) {
        final hit = lower.indexOf(term, at);
        if (hit < 0) break;
        intervals.add((hit, hit + term.length));
        at = hit + term.length;
      }
    }
    if (intervals.isEmpty) {
      return Text(text, maxLines: maxLines, overflow: TextOverflow.ellipsis, style: style);
    }
    intervals.sort((a, b) {
      final c = a.$1.compareTo(b.$1);
      return c != 0 ? c : b.$2.compareTo(a.$2);
    });
    final merged = <(int, int)>[];
    var curStart = intervals.first.$1;
    var curEnd = intervals.first.$2;
    for (var i = 1; i < intervals.length; i++) {
      final next = intervals[i];
      if (next.$1 <= curEnd) {
        if (next.$2 > curEnd) curEnd = next.$2;
      } else {
        merged.add((curStart, curEnd));
        curStart = next.$1;
        curEnd = next.$2;
      }
    }
    merged.add((curStart, curEnd));

    final spans = <TextSpan>[];
    var last = 0;
    for (final interval in merged) {
      if (interval.$1 > last) {
        spans.add(TextSpan(text: text.substring(last, interval.$1)));
      }
      spans.add(TextSpan(
        text: text.substring(interval.$1, interval.$2),
        style: const TextStyle(backgroundColor: Color(0xFFFFEB3B), color: Colors.black),
      ));
      last = interval.$2;
    }
    if (last < text.length) {
      spans.add(TextSpan(text: text.substring(last)));
    }

    return Text.rich(TextSpan(style: style, children: spans), maxLines: maxLines, overflow: TextOverflow.ellipsis);
  }

  Set<String> _extractKeywords(String query, Iterable<String> extraKeys) {
    final keys = <String>{};
    for (final k in extraKeys) {
      if (k.trim().isNotEmpty) keys.add(k.trim());
    }
    final terms = query.trim().split(RegExp(r'\s+'));
    for (final t in terms) {
      if (t.isEmpty) continue;
      if (t.contains(':')) continue;
      var clean = t;
      if (clean.startsWith('"') && clean.endsWith('"') && clean.length > 2) {
        clean = clean.substring(1, clean.length - 1);
      } else {
        clean = clean.replaceAll(RegExp(r'^["\-]'), '').replaceAll(RegExp(r'["~].*$'), '');
      }
      if (clean.isNotEmpty) keys.add(clean);
    }
    return keys;
  }

  String _formatMatchCounts(TimemachineContentResult result) {
    if (result.matchCounts.isEmpty) {
      return '언급 ${result.totalMatches}회';
    }
    final entries = result.matchCounts.entries.map((e) => '${e.key} ${e.value}').join(' \u00B7 ');
    return '$entries (합계 ${result.totalMatches})';
  }

  List<TimemachineContentResult> _sortContentResults(
      List<TimemachineContentResult> results, String sortKey) {
    final indexed = results.asMap().entries.toList();
    indexed.sort((a, b) {
      final itemA = a.value;
      final itemB = b.value;
      if (sortKey == 'hierarchy_relevance') {
        final hComp = itemA.hierarchyRank.compareTo(itemB.hierarchyRank);
        if (hComp != 0) return hComp;

        final mComp = itemB.totalMatches.compareTo(itemA.totalMatches);
        if (mComp != 0) return mComp;

        final dateA = itemA.mtime > itemA.ctime ? itemA.mtime : itemA.ctime;
        final dateB = itemB.mtime > itemB.ctime ? itemB.mtime : itemB.ctime;
        final dComp = dateB.compareTo(dateA);
        if (dComp != 0) return dComp;

        final bComp = itemB.bm25.compareTo(itemA.bm25);
        if (bComp != 0) return bComp;

        return a.key.compareTo(b.key);
      } else if (sortKey == 'mentions') {
        final mComp = itemB.totalMatches.compareTo(itemA.totalMatches);
        if (mComp != 0) return mComp;

        final dateA = itemA.mtime > itemA.ctime ? itemA.mtime : itemA.ctime;
        final dateB = itemB.mtime > itemB.ctime ? itemB.mtime : itemB.ctime;
        final dComp = dateB.compareTo(dateA);
        if (dComp != 0) return dComp;

        final bComp = itemB.bm25.compareTo(itemA.bm25);
        if (bComp != 0) return bComp;

        return a.key.compareTo(b.key);
      } else if (sortKey == 'relevance') {
        final scoreComp = itemB.bm25.compareTo(itemA.bm25);
        if (scoreComp != 0) return scoreComp;
        return a.key.compareTo(b.key);
      } else if (sortKey == 'recent') {
        final dateA = itemA.mtime > itemA.ctime ? itemA.mtime : itemA.ctime;
        final dateB = itemB.mtime > itemB.ctime ? itemB.mtime : itemB.ctime;
        final dComp = dateB.compareTo(dateA);
        if (dComp != 0) return dComp;

        return a.key.compareTo(b.key);
      }
      return a.key.compareTo(b.key);
    });
    return indexed.map((e) => e.value).toList();
  }


  /// 개발자 모드가 꺼지면 본문·연관검색 상태를 모두 비우고 파일명 모드로 돌린다.
  void _resetContentState() {
    _contentSearchRevision++;
    _contentPoll?.cancel();
    _contentMode = false;
    _relatedMode = false;
    _contentIndexRunning = false;
    _contentResults = [];
    _selectedContentResult = null;
    _preview = null;
    _showOperatorHelp = false;
    _contentNeedsReindex = false;
    _query = '';
    _searchBox.reset(tab: 'file');
  }

  /// 2글자 미만(또는 Enter로 1글자)일 때 그 탭이 검색하지 않는 기준. 열 보기·종류 필터 재검색이 함께 쓴다.
  String get _activeQuery => _searchBox.phase == InstantSearchPhase.empty ? '' : _searchBox.query;

  String _defaultContentSort(String tab) => tab == 'related' ? 'hierarchy_relevance' : 'relevance';

  /// 검색 컨트롤러가 부르는 실제 검색. 늦게 온 응답은 컨트롤러가 요청 번호로 버린다.
  Future<_ContentSearchOutcome?> _runSearch(InstantSearchRequest request) async {
    final query = request.query;
    if (request.tab == 'file') {
      _query = query;
      unawaited(_filtersChanged());
      if (_fileTableView || _fileSearchActive) await _runFileSearch(query);
      return null;
    }
    final relatedMode = request.tab == 'related';
    // 정렬 단추로 다시 찾을 때만 고른 정렬을 쓰고, 새 입력·탭 전환은 기본 정렬로 돌아간다.
    final chosen = request.options;
    final activeSort = chosen is String ? chosen : _defaultContentSort(request.tab);
    ++_contentSearchRevision;
    await _readContentStatus();
    if (!mounted || request.id != _searchBox.requestId) return null;
    setState(() {
      _contentSort = activeSort;
      _selectedContentResult = null;
      _preview = null;
    });
    final response = await _api.command(relatedMode ? 'related_search' : 'content_search', {
      'query': query, 'limit': 100,
      if (relatedMode) 'sort': 'hierarchy_relevance' else 'sort': activeSort,
    });
    final rawItems = response['items'];
    final items = rawItems is List ? rawItems.map((raw) => TimemachineContentResult.fromJson(Map<String, Object?>.from(raw as Map), query: query)).toList() : <TimemachineContentResult>[];
    return (
      sort: activeSort,
      items: relatedMode
          ? _sortContentResults(items, 'hierarchy_relevance')
          : activeSort == 'relevance' ? items : _sortContentResults(items, activeSort),
    );
  }

  void _applySearchResult(InstantSearchRequest request, _ContentSearchOutcome? outcome) {
    if (outcome == null || !mounted) return;
    setState(() {
      _contentSort = outcome.sort;
      _contentResults = outcome.items;
    });
  }

  void _searchFailed(InstantSearchRequest request, Object error) {
    if (!mounted || request.tab == 'file') return;
    setState(() => _error = '본문 검색을 수행하지 못했습니다.');
  }

  /// 검색어가 기준 미만이 되면 그 탭의 결과를 비운다(파일명 탭은 기간 전체 타임라인으로 돌아간다).
  void _searchCleared(String tab, String query) {
    if (!mounted) return;
    if (tab == 'file') {
      _query = '';
      unawaited(_filtersChanged());
      if (_fileTableView || _fileSearchActive) unawaited(_runFileSearch(''));
      return;
    }
    ++_contentSearchRevision; // 열려 있던 미리보기 응답도 버린다.
    setState(() {
      _contentResults = [];
      _selectedContentResult = null;
      _preview = null;
      _contentSort = _defaultContentSort(tab);
    });
    unawaited(_readContentStatus());
  }

  Future<void> _exportMarkdown() async {
    if (!_developerMode) return;
    try {
      final response = await _api.command('content_export_markdown', {'query': _search.text.trim()});
      if (!mounted) return;
      final message = response['message'] ?? response['path'] ?? (response['ok'] == true ? 'Markdown으로 저장했습니다.' : 'Markdown 저장에 실패했습니다.');
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text('$message')));
    } catch (_) {
      if (mounted) ScaffoldMessenger.of(context).showSnackBar(const SnackBar(content: Text('Markdown 저장에 실패했습니다.')));
    }
  }

  List<int> _previewFindHits() {
    final body = '${_preview?['snippet'] ?? _selectedContentResult?.snippet ?? ''}'.toLowerCase();
    final term = _previewFind.text.trim().toLowerCase();
    if (term.isEmpty || body.isEmpty) return const [];
    final hits = <int>[];
    for (var at = 0; at <= body.length - term.length;) {
      final hit = body.indexOf(term, at);
      if (hit < 0) break;
      hits.add(hit);
      at = hit + term.length;
    }
    return hits;
  }

  void _movePreviewFind(int direction) {
    final hits = _previewFindHits();
    if (hits.isEmpty) return;
    setState(() => _previewFindIndex = (_previewFindIndex + direction + hits.length) % hits.length);
  }

  Widget _previewFindText(String text) {
    final term = _previewFind.text.trim();
    final lower = text.toLowerCase();
    final needle = term.toLowerCase();
    if (needle.isEmpty) {
      return _markedTerms(
        text,
        _extractKeywords(_search.text.trim(), _selectedContentResult?.matchCounts.keys ?? const <String>[]),
        style: const TextStyle(fontSize: 13, height: 1.5),
      );
    }
    final hits = <int>[];
    for (var at = 0; at <= lower.length - needle.length;) {
      final hit = lower.indexOf(needle, at);
      if (hit < 0) break;
      hits.add(hit);
      at = hit + needle.length;
    }
    if (hits.isEmpty) return Text(text, style: const TextStyle(fontSize: 13, height: 1.5));
    final active = hits[_previewFindIndex % hits.length];
    final spans = <TextSpan>[];
    var at = 0;
    for (final hit in hits) {
      if (hit > at) spans.add(TextSpan(text: text.substring(at, hit)));
      spans.add(TextSpan(
        text: text.substring(hit, hit + needle.length),
        style: TextStyle(
          backgroundColor: hit == active ? Colors.orangeAccent : const Color(0xFFFFEB3B),
          color: Colors.black,
        ),
      ));
      at = hit + needle.length;
    }
    if (at < text.length) spans.add(TextSpan(text: text.substring(at)));
    return Text.rich(TextSpan(children: spans), style: const TextStyle(fontSize: 13, height: 1.5));
  }

  Future<void> _previewContentResult(TimemachineContentResult result) async {
    final rev = ++_previewRevision;
    final selectedSearchRevision = _contentSearchRevision;
    setState(() {
      _selectedContentResult = result;
      _previewLoading = true;
      _preview = null;
    });
    try {
      final payload = <String, Object?>{
        if (result.id != null) 'id': result.id,
        if (result.path != null) 'path': result.path,
        'query': _search.text.trim(),
        if (result.position?.page != null) 'page': result.position!.page,
        if (result.position?.sheet != null) 'sheet': result.position!.sheet,
        if (result.position?.row != null) 'row': result.position!.row,
      };
      final response = await _api.command('preview', payload);
      final ffiHierarchy = HierarchyFfi.instance.analyze([
        {'text': result.snippet, 'left': 0, 'indent': 0, 'font': '', 'size_pt': 12},
      ]);
      final previewResponse = Map<String, Object?>.from(response as Map);
      if (ffiHierarchy != null) previewResponse['hierarchy_ffi'] = ffiHierarchy;
      if (!mounted || rev != _previewRevision || selectedSearchRevision != _contentSearchRevision || _selectedContentResult != result) return;
      setState(() {
        _preview = previewResponse;
        _previewLoading = false;
      });
    } catch (_) {
      if (!mounted || rev != _previewRevision || selectedSearchRevision != _contentSearchRevision || _selectedContentResult != result) return;
      setState(() {
        _previewLoading = false;
        _preview = {'ok': false, 'message': '미리보기를 불러오지 못했습니다.'};
      });
    }
  }

  Future<void> _openDocument(Map<String, Object?> payload) async {
    try {
      final response = await _api.command('open', payload);
      if (!mounted) return;
      setState(() => _error = response['ok'] == true
          ? null
          : '${response['message'] ?? '파일을 열지 못했습니다.'}');
    } catch (_) {
      if (mounted) setState(() => _error = '파일을 열지 못했습니다. 앱 연결을 확인해 주세요.');
    }
  }

  Future<void> _openContentResult(TimemachineContentResult result) async {
    if (result.id != null) {
      await _openDocument({'id': result.id});
    } else if (result.path != null && result.path!.isNotEmpty) {
      await _openDocument({'path': result.path});
    } else if (mounted) {
      setState(() => _error = '파일 항목을 확인해 주세요.');
    }
  }

  Future<void> _runFileSearch(String query) async {
    if (!_fileSearchSupported) return;
    final rev = ++_fileSearchRevision;
    await _readContentStatus();
    if (!mounted || rev != _fileSearchRevision) return;
    if (query.trim().isEmpty) {
      // 검색어가 없으면 빈 file_search 결과 대신 현재 기간의 타임라인 항목을 표로 보여준다.
      if (!mounted) return;
      setState(() {
        _fileSearchResults = [];
        _fileSearchActive = false;
      });
      return;
    }
    try {
      final roots = _settings?['roots'];
      final results = await _api.fileSearch(
        query,
        kinds: _selectedKinds.isEmpty ? null : _selectedKinds.toList(),
        roots: roots is List ? roots.map((e) => '$e').toList() : null,
      );
      if (!mounted || rev != _fileSearchRevision) return;
      setState(() {
        _fileSearchResults = results;
        _fileSearchActive = true;
        _fileSearchSupported = true;
      });
    } catch (_) {
      if (!mounted || rev != _fileSearchRevision) return;
      setState(() {
        _fileSearchSupported = false;
        _fileSearchActive = false;
        _fileSearchResults = [];
        _fileTableView = false;
      });
      _filtersChanged();
    }
  }

  // 모든 상태 응답은 이곳에서 화면 상태로 해석한다. 구형 서비스의 running도 지원한다.
  void _applyContentStatus(Map<String, Object?> status) {
    if (!mounted || (status['ok'] == false && !status.containsKey('phase'))) return;
    final phase = status['phase'];
    final running = phase is String
        ? const {'indexing', 'reindexing', 'cancel_requested'}.contains(phase)
        : status['running'] == true;
    setState(() {
      _contentProgress = status;
      _contentIndexRunning = running;
      _contentNeedsReindex = status['needs_reindex'] == true;
      _contentNeedsReindexCount = (status['needs_reindex_count'] as num?)?.toInt() ?? 0;
      _contentReindexGuide = status['reindex_guide'] as String?;
      if (!_contentNeedsReindex) _contentReindexDismissed = false;
    });
    _contentPoll?.cancel();
    if (running) _pollContentIndex();
  }

  Future<void> _readContentStatus() async {
    try {
      _applyContentStatus(await _api.command('content_index_status'));
    } catch (_) {
      // 통신 실패는 작업 완료가 아니므로 마지막 상태를 유지하고 다시 확인한다.
      if (mounted && _contentIndexRunning) _pollContentIndex();
    }
  }

  Future<void> _startContentIndex() async {
    _applyContentStatus(await _api.command('content_index_start'));
  }
  void _pollContentIndex() {
    _contentPoll?.cancel();
    _contentPoll = Timer(const Duration(seconds: 1), _readContentStatus);
  }
  Future<void> _cancelContentIndex() async {
    _applyContentStatus(await _api.command('content_index_cancel'));
  }
  /// 검색 방식(탭) 전환. 검색어는 지우지 않고, 바뀐 탭이 그 검색어로 곧바로 찾는다(ISUI1).
  /// 검색어가 2글자 미만이면 바뀐 탭의 결과를 비운다(파일명 탭은 기간 전체 타임라인).
  void _selectSearchMode(String mode) {
    if (mode == _searchBox.tab) return;
    setState(() {
      _relatedMode = mode == 'related';
      _contentMode = mode != 'file';
      _contentResults = [];
      _selectedContentResult = null;
      _preview = null;
      _showOperatorHelp = false;
      _contentSort = _defaultContentSort(mode);
      _fileSearchResults = [];
      _fileSearchActive = false;
    });
    _searchBox.selectTab(mode);
  }

  Future<void> _load({bool firstOpen = false}) async {
    Map<String, Object?> status;
    Map<String, Object?> settings;
    try {
      status = await _api.command('status');
      settings = await _api.command('settings_get');
    } catch (_) {
      if (mounted) setState(() { _settings = {'roots': [], 'time_mode':'max', 'enabled':false}; _error = '몽땅검색에 연결하지 못했습니다. 앱 연결을 확인해 주세요.'; });
      return;
    }
    if (!mounted) return;
    if (settings['ok'] != true) {
      setState(() { _settings = {'roots': [], 'time_mode':'max', 'enabled':false}; _error = '${settings['message'] ?? '설정을 불러오지 못했습니다.'}'; });
      return;
    }
    final raw = settings['settings'];
    final loaded = raw is Map ? Map<String, Object?>.from(raw) : {'roots': [], 'time_mode':'max', 'enabled':false};
    var developerMode = false;
    try {
      final appSettings = await widget.api('get_settings', const []);
      if (appSettings is Map && appSettings['api_version'] == 1 && appSettings['ok'] == true) {
        final settingsBody = appSettings['data'] is Map
            ? Map<String, Object?>.from(appSettings['data'] as Map)
            : <String, Object?>{};
        final values = settingsBody['values'] is Map
            ? Map<String, Object?>.from(settingsBody['values'] as Map)
            : <String, Object?>{};
        developerMode = values['developer_mode'] == true;
      } else if (appSettings is Map && appSettings['developer_mode'] is bool) {
        developerMode = appSettings['developer_mode'] == true;
      }
    } catch (_) {
      developerMode = false;
    }
    setState(() { _settings = loaded; _developerMode = developerMode; _watchByAgent = status['watch'] == 'agent';
      if (status['ok'] != true) _error = '${status['message'] ?? '상태를 불러오지 못했습니다.'}'; });
    await _readContentStatus();
    if (!mounted) return;
    if (loaded['enabled'] == true) {
      // 에이전트가 없으면 앱을 열 때 증분 색인한다(사용자 결정 6). 에이전트가 있으면 폴더 설정만 맞춘다.
      if (firstOpen) { try { await _requestScan(); } catch (_) {} if (!mounted) return; }
      await _refreshDays();
      if (_start == null || _end == null) {
        await _resetRange();
      } else {
        await _refreshHourDensity();
        await _loadPage();
      }
    }
  }

  Future<void> _retryConnection() async {
    setState(() { _settings = null; _error = null; });
    await _load();
  }

  Future<Map<String, Object?>> _requestScan() async {
    final response = await _api.command('scan');
    if (!mounted) return response;
    if (response['by'] != null) setState(() => _watchByAgent = response['by'] == 'agent');
    if (response['running'] == true) _waitForScan();
    return response;
  }
  void _waitForScan() {
    _scanPoll?.cancel();
    _scanPoll = Timer(const Duration(seconds: 1), () async {
      Map<String, Object?> status;
      try { status = await _api.command('status'); } catch (_) { return; }
      if (!mounted) return;
      final scan = status['scan'];
      if (scan is Map && scan['running'] == true) { _waitForScan(); return; }
      await _filtersChanged();
    });
  }

  Map<String, Object?> get _filters => {'roots': _settings?['roots'] ?? [],
    'kinds': _selectedKinds.isEmpty ? null : _selectedKinds.toList(), 'query': _query};
  Future<void> _refreshDays() async {
    final revision = ++_calendarRevision;
    final requestedMonth = _month;
    final response = await _api.command('day_counts', {'year':_month.year, 'month':_month.month, ..._filters});
    if (!mounted || revision != _calendarRevision || _month != requestedMonth || response['ok'] != true) return;
    final raw = response['days']; final values = <String,int>{};
    if (raw is Map) { for (final entry in raw.entries) { final value = entry.value; values['${entry.key}'] = value is Map ? ((value['total'] as num?)?.toInt() ?? 0) : ((value as num?)?.toInt() ?? 0); } }
    setState(() => _days = values);
  }
  Future<void> _refreshHourDensity() async {
    if (_start == null || _end == null) return;
    final revision = ++_densityRevision;
    final response = await _api.command('hour_density', {
      'start': _start!.millisecondsSinceEpoch / 1000,
      'end': _end!.millisecondsSinceEpoch / 1000,
      ..._filters,
    });
    if (!mounted || revision != _densityRevision || response['ok'] != true) return;
    final density = TimemachineHourDensity.fromJson(response);
    setState(() => _hourDensity = density.counts);
  }
  Future<void> _moveMonth(int amount) async {
    setState(() {
      _month = DateTime(_month.year, _month.month + amount);
      _days = {};
    });
    await _refreshDays();
    await _resetRange();
  }
  Future<void> _resetRange() async {
    _filterRevision++; _requestSerial++; _loading = false;
    _start = DateTime(_month.year, _month.month).toLocal();
    _end = DateTime(_month.year, _month.month + 1).toLocal();
    _firstDay = null; _items.clear(); _offset = 0; _more = true;
    await _refreshHourDensity();
    await _loadPage();
  }
  Map<String,Object?> _rangePayload() => {
    'start': _start!.millisecondsSinceEpoch / 1000,
    'end': _end!.millisecondsSinceEpoch / 1000,
    ..._filters, 'newest_first': true, 'limit': 200, 'offset': _offset,
  };
  Future<void> _loadPage() async {
    if (_loading || !_more || _start == null || _end == null) return;
    final revision = _filterRevision;
    final requestSerial = ++_requestSerial;
    setState(() { _loading = true; _error = null; });
    Map<String, Object?> response;
    try { response = await _api.command('range', _rangePayload()); }
    catch (_) {
      if (mounted) setState(() { _loading = false; _more = false; _error = '목록을 불러오지 못했습니다. 앱 연결을 확인해 주세요.'; });
      return;
    }
    if (!mounted) return;
    if (revision != _filterRevision) {
      if (requestSerial == _requestSerial) setState(() => _loading = false);
      return;
    }
    final raw = response['items'];
    setState(() {
      if (response['ok'] == true && raw is List) {
        final page = <TimemachineItem>[];
        var skipped = 0;
        for (final entry in raw) {
          if (entry is! Map) continue;
          try {
            final item = TimemachineItem.fromJson(entry);
            if (isExcludedTimemachineName(item.name)) { skipped++; continue; }
            page.add(item);
          } on Object {
            _error = '목록 응답의 파일 정보를 읽지 못했습니다.';
          }
        }
        _items.addAll(page); _offset += page.length + skipped; _more = raw.length == 200;
      } else { _error = '${response['message'] ?? '목록을 불러오지 못했습니다.'}'; _more = false; }
      _loading = false;
    });
  }
  void _onScroll() { if (_more && !_loading && _scroll.hasClients && _scroll.position.extentAfter < 500) _loadPage(); }
  Future<void> _filtersChanged() async { _filterRevision++; _requestSerial++; _loading = false; _items.clear(); _offset = 0; _more = true; await _refreshDays(); _refreshHourDensity(); await _loadPage(); }
  Future<void> _enable() async {
    final next = Map<String,Object?>.from(_settings ?? {})..['enabled'] = true;
    Map<String, Object?> saved;
    try { saved = await _api.command('settings_set', next); }
    catch (_) { if (mounted) setState(() => _error = '설정을 저장하지 못했습니다. 앱 연결을 확인해 주세요.'); return; }
    if (saved['ok'] != true) { if (mounted) setState(() => _error = '${saved['message'] ?? '설정을 저장하지 못했습니다.'}'); return; }
    try { await _requestScan(); }
    catch (_) { if (mounted) setState(() => _error = '색인 요청을 보내지 못했습니다. 앱 연결을 확인해 주세요.'); }
    await _load();
  }
  Future<void> _saveSettings(Map<String,Object?> next) async {
    final response=await _api.command('settings_set',next);
    if(response['ok']==true){setState(()=>_settings=Map<String,Object?>.from(response['settings'] as Map));await _filtersChanged();}
    else if(mounted){setState(()=>_error='${response['message'] ?? '설정을 저장하지 못했습니다.'}');}
  }
  Future<void> _pickExtensions() async {
    final values=List<String>.from((_settings?['extensions'] as List?) ?? _extensions);
    var useRust = _settings?['rust_content_index'] != false;
    var idleIndexing = _settings?['idle_indexing'] != false;
    var idleSeconds = (_settings?['idle_seconds'] as num?)?.toInt() ?? 120;
    final secondsChoices = {...idleSecondsChoices, idleSeconds}.toList()..sort();
    final saved=await showDialog<bool>(context:context,builder:(context)=>StatefulBuilder(builder:(context,update)=>AlertDialog(
      title:const Text('적용 확장자'),
      content:SingleChildScrollView(child:Column(mainAxisSize:MainAxisSize.min,children:[
        Wrap(spacing:6,runSpacing:4,children:[for(final ext in _extensions) FilterChip(label:Text(ext),selected:values.contains(ext),onSelected:(selected)=>update(()=>selected?values.add(ext):values.remove(ext)))]),
        SwitchListTile(contentPadding:EdgeInsets.zero,title:const Text('Rust 본문 색인'),subtitle:const Text('끄면 Python 색인기를 사용합니다.'),value:useRust,onChanged:(value)=>update(()=>useRust=value)),
        SwitchListTile(key:const ValueKey('tm-idle-indexing'),contentPadding:EdgeInsets.zero,title:const Text('쉬는 시간에 색인'),subtitle:const Text('컴퓨터를 쓰지 않을 때만 바뀐 문서를 낮은 우선순위로 색인합니다. 배터리·전체 화면·발표 중에는 미룹니다.'),value:idleIndexing,onChanged:(value)=>update(()=>idleIndexing=value)),
        ListTile(contentPadding:EdgeInsets.zero,title:const Text('쉬는 시간 기준'),subtitle:const Text('키보드·마우스 입력이 없는 시간'),
          trailing:DropdownButton<int>(key:const ValueKey('tm-idle-seconds'),value:idleSeconds,onChanged:idleIndexing?(value){if(value!=null)update(()=>idleSeconds=value);}:null,
            items:[for(final seconds in secondsChoices) DropdownMenuItem(value:seconds,child:Text(idleSecondsLabel(seconds)))])),
      ])),
      actions:[TextButton(onPressed:()=>Navigator.pop(context,false),child:const Text('취소')),FilledButton(onPressed:()=>Navigator.pop(context,true),child:const Text('저장'))],
    )));
    if(saved==true&&mounted)await _saveSettings(Map<String,Object?>.from(_settings!)..['extensions']=values..['rust_content_index']=useRust..['idle_indexing']=idleIndexing..['idle_seconds']=idleSeconds);
  }
  Future<void> _addRoot() async {
    final path=await getDirectoryPath(); if(path==null||!mounted)return;
    final roots=List<String>.from((_settings?['roots'] as List?) ?? const [])..add(path);
    await _saveSettings(Map<String,Object?>.from(_settings!)..['roots']=roots);
  }
  Future<void> _chooseDay(DateTime day) async {
    final chosen = DateTime(day.year, day.month, day.day);
    setState(() {
      if (_firstDay != null) {
        if (_sameDay(_firstDay!, chosen)) {
          _start = chosen; _end = chosen.add(const Duration(days: 1)); _firstDay = null;
        } else {
          final first = _firstDay!;
          _start = first.isBefore(chosen) ? first : chosen;
          _end = (first.isAfter(chosen) ? first : chosen).add(const Duration(days: 1));
          _firstDay = null;
        }
      } else if (_start != null && _end != null && _end!.difference(_start!).inDays == 1 && _sameDay(_start!, chosen)) {
        _start = DateTime(_month.year, _month.month); _end = DateTime(_month.year, _month.month + 1);
      } else {
        _firstDay = chosen; _start = chosen; _end = chosen.add(const Duration(days: 1));
      }
      _filterRevision++; _requestSerial++; _loading = false; _items.clear(); _offset = 0; _more = true;
    });
    _refreshHourDensity();
    await _loadPage();
  }
  bool _sameDay(DateTime a, DateTime b) => a.year == b.year && a.month == b.month && a.day == b.day;
  Future<void> _quickRange(String mode) async {
    final now = DateTime.now();
    setState(() {
      _month = DateTime(now.year,now.month);
      if (mode == 'today') { _start = DateTime(now.year,now.month,now.day); _end = _start!.add(const Duration(days:1)); }
      if (mode == 'week') { _start = _sundayStart(now); _end = _start!.add(const Duration(days:7)); }
      if (mode == 'month') { _month = DateTime(now.year,now.month); _start = _month; _end = DateTime(now.year,now.month+1); }
      _firstDay = null; _filterRevision++; _requestSerial++; _loading = false; _items.clear(); _offset = 0; _more = true;
    });
    await _refreshDays(); _refreshHourDensity(); await _loadPage();
  }
  Future<void> _scan() async {
    try {
      final response = await _requestScan();
      if (response['ok'] != true && mounted) setState(() => _error = '${response['message'] ?? '다시 훑지 못했습니다.'}');
      await _load();
    } catch (_) {
      if (mounted) setState(() => _error = '다시 훑지 못했습니다. 앱 연결을 확인해 주세요.');
    }
  }
  Future<void> _openItem(TimemachineItem item) => _openDocument({'id': item.id});
  Future<void> _revealItem(TimemachineItem item) async {
    try { final response = await _api.command('reveal', {'id':item.id});
      if (response['ok'] != true && mounted) setState(() => _error = '${response['message'] ?? '폴더를 열지 못했습니다.'}');
    } catch (_) { if (mounted) setState(() => _error = '폴더를 열지 못했습니다. 앱 연결을 확인해 주세요.'); }
  }

  Future<void> _sendToFeature(String feature, int id) async {
    try {
      final response = await _api.command('send_to_feature', {'feature': feature, 'ids': [id]});
      if (!mounted) return;
      if (response['ok'] == true) {
        ScaffoldMessenger.of(context).showSnackBar(const SnackBar(content: Text('목록에 추가했습니다.')));
      } else {
        setState(() => _error = '${response['message'] ?? '보내지 못했습니다.'}');
      }
    } catch (_) {
      if (mounted) setState(() => _error = '보내지 못했습니다. 앱 연결을 확인해 주세요.');
    }
  }

  Future<void> _findRelated(TimemachineItem item) async {
    await showDialog<void>(context: context, builder: (_) => RelatedDocumentsDialog(
      api: _api,
      selectedPath: '${item.folder}\\${item.name}',
      selectedName: item.name,
      candidates: List.unmodifiable(_items),
    ));
  }

  Future<void> _clearThumbnailCache() async {
    try {
      final response = await _api.command('thumbnail_cache_clear');
      if (!mounted) return;
      if (response['ok'] == true) {
        _thumbnails.clear();
        setState(() {});
        ScaffoldMessenger.of(context).showSnackBar(const SnackBar(content: Text('썸네일 캐시를 지웠습니다.')));
      } else {
        setState(() => _error = '${response['message'] ?? '캐시를 지우지 못했습니다.'}');
      }
    } catch (_) {
      if (mounted) setState(() => _error = '캐시를 지우지 못했습니다. 앱 연결을 확인해 주세요.');
    }
  }

  String _localizedDayHeading(DateTime value) {
    final now = DateTime.now();
    final today = DateTime(now.year, now.month, now.day);
    final day = DateTime(value.year, value.month, value.day);
    if (_sameDay(day, today)) return '오늘 · ${_localizedWeekday(day)}';
    if (_sameDay(day, today.subtract(const Duration(days: 1)))) {
      return '어제 · ${_localizedWeekday(day)}';
    }
    return '${day.year}년 ${day.month}월 ${day.day}일 · ${_localizedWeekday(day)}';
  }

  String _localizedWeekday(DateTime value) =>
      '${const ['일', '월', '화', '수', '목', '금', '토'][value.weekday % 7]}요일';

  DateTime _sundayStart(DateTime value) =>
      DateTime(value.year, value.month, value.day).subtract(Duration(days: value.weekday % 7));

  IconData _kindIcon(String kind) => switch(kind) {'hwp'=>Icons.description_outlined,'doc'=>Icons.article_outlined,'sheet'=>Icons.table_chart_outlined,'slide'=>Icons.slideshow_outlined,'pdf'=>Icons.picture_as_pdf_outlined,'image'=>Icons.image_outlined,_=>Icons.insert_drive_file_outlined};

  @override
  Widget build(BuildContext context) {
    if (_settings == null) return const Center(child: CircularProgressIndicator());
    if (_settings!['enabled'] != true) return _disabledView();
    return LayoutBuilder(builder: (context, box) {
      final calendar = _calendar();
      final timeline = _timeline();
      final isNarrow = box.maxWidth < 650;
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          _filtersBar(),
          if (_contentMode && _showOperatorHelp) _operatorHelp(),
          if (_contentMode && _contentNeedsReindex && !_contentReindexDismissed && _contentReindexGuide != null)
            MaterialBanner(
              content: Text('$_contentReindexGuide (재색인 필요: $_contentNeedsReindexCount건)'),
              leading: const Icon(Icons.info_outline, color: Colors.amber),
              actions: [
                TextButton(onPressed: _startContentIndex, child: const Text('재색인 시작')),
                TextButton(onPressed: () => setState(() => _contentReindexDismissed = true), child: const Text('닫기')),
              ],
            ),
          if (_error != null)
            MaterialBanner(
              content: Text(_error!),
              actions: [
                TextButton(
                  onPressed: () => setState(() => _error = null),
                  child: const Text('\uB2EB\uAE30'),
                ),
              ],
            ),
          Expanded(
            child: (_contentMode || _relatedMode)
                ? Column(children: [if (_contentMode) _contentSearchPanel(), Expanded(child: _contentResultsView())])
                : (_fileTableView && _fileSearchSupported
                    ? Row(children: [
                        Expanded(flex: 3, child: _fileSearchTable(_fileSearchActive ? _fileSearchResults : _items)),
                        if (_selectedContentResult != null || _previewLoading) ...[
                          const VerticalDivider(width: 1),
                          Expanded(flex: 2, child: _contentPreviewPane()),
                        ],
                      ])
                    : (isNarrow
                        ? Column(
                            crossAxisAlignment: CrossAxisAlignment.stretch,
                            children: [
                              _densityBar(vertical: false),
                              ConstrainedBox(
                                constraints: const BoxConstraints(maxHeight: 180),
                                child: SingleChildScrollView(child: calendar),
                              ),
                              Expanded(child: timeline),
                            ],
                          )
                        : Row(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              _densityBar(vertical: true),
                              Expanded(child: timeline),
                              SizedBox(width: 320, child: SingleChildScrollView(child: calendar)),
                            ],
                          ))),
          ),
        ],
      );
    });
  }


  Widget _operatorHelp() => Card(
    margin: const EdgeInsets.symmetric(horizontal: 12, vertical: 4),
    color: Theme.of(context).colorScheme.surfaceContainerHighest,
    child: Padding(
      padding: const EdgeInsets.all(12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              const Icon(Icons.help_outline, size: 18),
              const SizedBox(width: 6),
              const Text('본문 키워드 검색 연산자 안내', style: TextStyle(fontWeight: FontWeight.bold)),
              const Spacer(),
              IconButton(
                icon: const Icon(Icons.close, size: 18),
                tooltip: '닫기',
                onPressed: () => setState(() => _showOperatorHelp = false),
              ),
            ],
          ),
          const SizedBox(height: 6),
          const Text('\u2022 "\uAD6C\uBB38": \uB744\uC5B4\uC4F0\uAE30\uB97C \uD3EC\uD568\uD55C \uC815\uD655\uD55C \uAD6C\uBB38 \uC77C\uCE58\n'
              '\u2022 -\uB0B1\uB9D0 \uB610\uB294 -\uC81C\uC678\uC5B4: \uD2B9\uC815 \uB2E8\uC5B4\uAC00 \uD3EC\uD568\uB41C \uBB38\uC11C \uC81C\uC678\n'
              '\u2022 ext:hwp,pdf: \uD2B9\uC815 \uD655\uC7A5\uC790 \uBB38\uC11C\uB9CC \uAC80\uC0C9\n'
              '\u2022 path:\uD3F4\uB354: \uD2B9\uC815 \uD3F4\uB354 \uACBD\uB85C \uB0B4 \uBB38\uC11C\uB9CC \uAC80\uC0C9\n'
              '\u2022 after:2024-06 \uB610\uB294 before:2024-12: \uBB38\uC11C \uC218\uC815\uC77C \uBC94\uC744 \uD544\uD130\n'
              '\u2022 ~N: N\uB0B1\uB9D0 \uC774\uB0B4 \uADFC\uC811 \uAC80\uC0C9\n'
              '\u2022 \uC5EC\uB7EC \uB2E8\uC5B4 \uC785\uB825 \uC2DC \uAE30\uBCF8 AND \uC5F0\uC0B0\uC774 \uC801\uC6A9\uB418\uBA70, \uD55C\uAE00 \uC870\uC0AC\uAC00 \uBD98\uC740 \uAC8D\uC6B0\uB3C4 \uBD80\uBD84 \uC77C\uCE58\uB85C \uCC3E\uC2B5\uB2C8\uB2E4.',
              style: TextStyle(fontSize: 12, height: 1.4)),
        ],
      ),
    ),
  );

  Widget _contentResultsView() {
    if (_contentResults.isEmpty) {
      return Center(
        child: Padding(
          padding: const EdgeInsets.all(24),
          child: ListenableBuilder(
            listenable: _searchBox,
            builder: (context, _) => Text(switch (_searchBox.phase) {
              _ when _searchBox.query.isEmpty => '본문 검색어를 입력해 주세요.',
              InstantSearchPhase.empty => '2글자 이상 입력하면 바로 찾습니다(Enter는 1글자도 찾음).',
              InstantSearchPhase.typing || InstantSearchPhase.searching => '찾는 중…',
              _ => '일치하는 본문 검색 결과가 없습니다.',
            }, key: const ValueKey('tm-content-empty-message')),
          ),
        ),
      );
    }
    return Row(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Expanded(
          flex: 3,
          child: ListView.builder(
            itemCount: _contentResults.length,
            itemBuilder: (context, index) => _contentResultRow(_contentResults[index], index),
          ),
        ),
        if (_selectedContentResult != null || _previewLoading)
          const VerticalDivider(width: 1),
        if (_selectedContentResult != null || _previewLoading)
          Expanded(
            flex: 2,
            child: _contentPreviewPane(),
          ),
      ],
    );
  }

  Widget _contentResultRow(TimemachineContentResult result, int index) {
    final isSelected = _selectedContentResult == result;
    final keywords = _extractKeywords(_search.text.trim(), result.matchCounts.keys);
    return Padding(
      key: ValueKey('tm-content-row-${result.id ?? result.path ?? index}'),
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 4),
      child: Card(
        color: isSelected ? Theme.of(context).colorScheme.primaryContainer.withValues(alpha: 0.25) : null,
        child: InkWell(
          onTap: () => _previewContentResult(result),
          onDoubleTap: () => _openContentResult(result),
          child: Padding(
            padding: const EdgeInsets.all(12),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Icon(_kindIcon(result.kind), size: 20, color: Theme.of(context).colorScheme.primary),
                    const SizedBox(width: 8),
                    Expanded(
                      child: Text(
                        result.fileName,
                        style: const TextStyle(fontWeight: FontWeight.bold, fontSize: 14),
                        overflow: TextOverflow.ellipsis,
                      ),
                    ),
                    if (result.position != null && result.position!.badgeLabel.isNotEmpty) ...[
                      const SizedBox(width: 6),
                      _Badge(result.position!.badgeLabel),
                    ],
                  ],
                ),
                const SizedBox(height: 4),
                Text(
                  '${result.folderName.isNotEmpty ? '${result.folderName} \u00B7 ' : ''}${result.folderPath}',
                  style: Theme.of(context).textTheme.bodySmall,
                  overflow: TextOverflow.ellipsis,
                ),
                const SizedBox(height: 3),
                Text('${_formatEpochDate(result.mtime)} · ${result.kind.toUpperCase()}', style: Theme.of(context).textTheme.labelSmall),
                const SizedBox(height: 6),
                if (result.breadcrumbText.isNotEmpty) ...[
                  _HierarchyBreadcrumb(text: result.breadcrumbText),
                  const SizedBox(height: 4),
                ],
                _markedTerms(result.snippet, keywords, maxLines: 3, style: Theme.of(context).textTheme.bodyMedium),
                ..._sectionMatchLabels(result.sections),
                const SizedBox(height: 6),
                Row(
                  mainAxisAlignment: MainAxisAlignment.spaceBetween,
                  children: [
                    Text(
                      _formatMatchCounts(result),
                      style: TextStyle(
                        color: Theme.of(context).colorScheme.primary,
                        fontSize: 12,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                    if (result.hierarchy.isNotEmpty)
                      Text(
                        result.hierarchy,
                        style: Theme.of(context).textTheme.labelSmall?.copyWith(color: Colors.grey),
                      ),
                  ],
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }

  String _formatEpochDate(double value) {
    if (value <= 0) return '수정일 정보 없음';
    final d = DateTime.fromMillisecondsSinceEpoch((value * 1000).round()).toLocal();
    return '${d.year}.${d.month.toString().padLeft(2, '0')}.${d.day.toString().padLeft(2, '0')} 수정';
  }

  Widget _contentPreviewPane() {
    final item = _selectedContentResult;
    if (item == null) return const SizedBox.shrink();
    return Card(
      margin: const EdgeInsets.all(8),
      child: Padding(
        padding: const EdgeInsets.all(14),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Row(
              children: [
                Icon(_kindIcon(item.kind), size: 20),
                const SizedBox(width: 8),
                Expanded(
                  child: Text(
                    item.fileName,
                    style: const TextStyle(fontWeight: FontWeight.bold, fontSize: 15),
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
                IconButton(
                  icon: const Icon(Icons.close),
                  tooltip: '미리보기 닫기',
                  onPressed: () => setState(() => _selectedContentResult = null),
                ),
              ],
            ),
            const SizedBox(height: 4),
            Text(item.folderPath, style: Theme.of(context).textTheme.bodySmall, overflow: TextOverflow.ellipsis),
            if (item.position != null && item.position!.badgeLabel.isNotEmpty) ...[
              const SizedBox(height: 6),
              Row(
                children: [
                  _Badge(item.position!.badgeLabel),
                  if (item.hierarchy.isNotEmpty) ...[
                    const SizedBox(width: 6),
                    _Badge(item.hierarchy),
                  ],
                ],
              ),
            ],
            const Divider(height: 18),
            Expanded(
              child: _previewLoading
                  ? const Center(child: CircularProgressIndicator())
                  : SingleChildScrollView(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text('\uC77C\uCE58 \uC704\uCE58 \uBCF8\uBB38 \uBBF8\uB9AC\uBCF4\uAE30',
                              style: Theme.of(context).textTheme.labelMedium?.copyWith(
                                    color: Theme.of(context).colorScheme.primary,
                                    fontWeight: FontWeight.bold,
                                  )),
                          const SizedBox(height: 8),
                          _previewFindText('${_preview?['snippet'] ?? item.snippet}'),
                          if (_preview?['hierarchy_ffi'] is Map) ...[
                            const SizedBox(height: 8),
                            Text('위계 분석 (로컬 엔진)', style: Theme.of(context).textTheme.labelSmall),
                            Text(hierarchySummaryLine(_preview!['hierarchy_ffi'])),
                          ],
                        ],
                      ),
                    ),
            ),
            const SizedBox(height: 10),
            Row(
              children: [
                Expanded(
                  child: TextField(
                    key: const ValueKey('tm-preview-find'),
                    controller: _previewFind,
                    decoration: const InputDecoration(
                      prefixIcon: Icon(Icons.search),
                      hintText: '미리보기에서 찾기',
                      isDense: true,
                    ),
                    onChanged: (_) => setState(() => _previewFindIndex = 0),
                  ),
                ),
                IconButton(
                  key: const ValueKey('tm-preview-find-previous'),
                  tooltip: '이전 찾기',
                  onPressed: _previewFindHits().isEmpty ? null : () => _movePreviewFind(-1),
                  icon: const Icon(Icons.keyboard_arrow_up),
                ),
                Text(_previewFindHits().isEmpty ? '0 / 0' : '${_previewFindIndex % _previewFindHits().length + 1} / ${_previewFindHits().length}'),
                IconButton(
                  key: const ValueKey('tm-preview-find-next'),
                  tooltip: '다음 찾기',
                  onPressed: _previewFindHits().isEmpty ? null : () => _movePreviewFind(1),
                  icon: const Icon(Icons.keyboard_arrow_down),
                ),
              ],
            ),
            const SizedBox(height: 8),
            Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                FilledButton.icon(
                  onPressed: (item.id != null || (item.path != null && item.path!.isNotEmpty))
                      ? () => _openContentResult(item)
                      : null,
                  icon: const Icon(Icons.open_in_new, size: 16),
                  label: const Text('\uBB38\uC11C \uC5F4\uAE30'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }

  Widget _fileSearchTable(List<TimemachineItem> items) {
    final sorted = List<TimemachineItem>.from(items);
    sorted.sort((a, b) {
      int cmp = 0;
      switch (_fileSortColumn) {
        case 'name':
          cmp = a.name.compareTo(b.name);
          break;
        case 'folder':
          cmp = a.folder.compareTo(b.folder);
          break;
        case 'size':
          cmp = a.size.compareTo(b.size);
          break;
        case 'mtime':
          cmp = a.mtime.compareTo(b.mtime);
          break;
        case 'kind':
          cmp = a.kind.compareTo(b.kind);
          break;
        default:
          cmp = a.name.compareTo(b.name);
      }
      return _fileSortAscending ? cmp : -cmp;
    });

    final columns = [
      ('name', '\uC774\uB984'),
      ('folder', '\uACBD\uB85C'),
      ('size', '\uD06C\uAE30'),
      ('mtime', '\uC218\uC815\uC77C'),
      ('kind', '\uD615\uC2DD'),
    ];

    return LayoutBuilder(
      builder: (context, constraints) {
        final totalWidth = _fileColumnWidths.values.fold<double>(0.0, (a, b) => a + b) + columns.length * 16.0;
        final tableWidth = max(constraints.maxWidth, totalWidth);
        return SingleChildScrollView(
          scrollDirection: Axis.horizontal,
          child: SizedBox(
            width: tableWidth,
            child: Column(
              children: [
                Container(
                  color: Theme.of(context).colorScheme.surfaceContainerHighest,
                  padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
                  child: Row(
                    children: [
                      for (final (colKey, colName) in columns) ...[
                        SizedBox(
                          width: _fileColumnWidths[colKey] ?? 100,
                          child: InkWell(
                            key: ValueKey('tm-col-header-$colKey'),
                            onTap: () {
                              setState(() {
                                if (_fileSortColumn == colKey) {
                                  _fileSortAscending = !_fileSortAscending;
                                } else {
                                  _fileSortColumn = colKey;
                                  _fileSortAscending = true;
                                }
                              });
                            },
                            child: Row(
                              mainAxisSize: MainAxisSize.min,
                              children: [
                                Text(colName, style: const TextStyle(fontWeight: FontWeight.bold, fontSize: 13)),
                                const SizedBox(width: 4),
                                if (_fileSortColumn == colKey)
                                  Icon(_fileSortAscending ? Icons.arrow_upward : Icons.arrow_downward, size: 14),
                              ],
                            ),
                          ),
                        ),
                        GestureDetector(
                          key: ValueKey('tm-col-resizer-$colKey'),
                          behavior: HitTestBehavior.translucent,
                          onHorizontalDragUpdate: (details) {
                            setState(() {
                              final cur = _fileColumnWidths[colKey] ?? 100;
                              _fileColumnWidths[colKey] = max(40.0, cur + details.delta.dx);
                            });
                          },
                          child: MouseRegion(
                            cursor: SystemMouseCursors.resizeColumn,
                            child: Container(
                              width: 12,
                              height: 20,
                              alignment: Alignment.center,
                              child: Container(width: 2, height: 16, color: Colors.grey.withValues(alpha: 0.4)),
                            ),
                          ),
                        ),
                      ],
                    ],
                  ),
                ),
                const Divider(height: 1),
                Expanded(
                  child: sorted.isEmpty
                      ? const Center(child: Text('\uD15C\uC2DC\uD560 \uD30C\uC77C\uC774 \uC5C6\uC2B5\uB2C8\uB2E4.'))
                      : ListView.separated(
                          itemCount: sorted.length,
                          separatorBuilder: (context, _) => const Divider(height: 1),
                          itemBuilder: (context, index) {
                            final item = sorted[index];
                            final date = DateTime.fromMillisecondsSinceEpoch((item.mtime * 1000).round()).toLocal();
                            final dateStr = '${date.year}-${date.month.toString().padLeft(2, '0')}-${date.day.toString().padLeft(2, '0')}';
                            final sizeKb = (item.size / 1024).toStringAsFixed(1);
                            return InkWell(
                              key: ValueKey('tm-file-row-${item.id}'),
                              onTap: () => _previewContentResult(TimemachineContentResult.fromJson({
                                'id': item.id, 'name': item.name, 'folder': item.folder,
                                'path': '${item.folder}\\${item.name}', 'kind': item.kind,
                              }, query: _search.text.trim())),
                              onDoubleTap: () => _openItem(item),
                              child: Padding(
                                padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
                                child: Row(
                                  children: [
                                    SizedBox(
                                      width: _fileColumnWidths['name'] ?? 100,
                                      child: Text(item.name, overflow: TextOverflow.ellipsis),
                                    ),
                                    const SizedBox(width: 12),
                                    SizedBox(
                                      width: _fileColumnWidths['folder'] ?? 100,
                                      child: Text(item.folder, overflow: TextOverflow.ellipsis, style: Theme.of(context).textTheme.bodySmall),
                                    ),
                                    const SizedBox(width: 12),
                                    SizedBox(
                                      width: _fileColumnWidths['size'] ?? 100,
                                      child: Text('${sizeKb}KB', overflow: TextOverflow.ellipsis),
                                    ),
                                    const SizedBox(width: 12),
                                    SizedBox(
                                      width: _fileColumnWidths['mtime'] ?? 100,
                                      child: Text(dateStr, overflow: TextOverflow.ellipsis),
                                    ),
                                    const SizedBox(width: 12),
                                    SizedBox(
                                      width: _fileColumnWidths['kind'] ?? 100,
                                      child: Text(item.kind, overflow: TextOverflow.ellipsis),
                                    ),
                                  ],
                                ),
                              ),
                            );
                          },
                        ),
                ),
              ],
            ),
          ),
        );
      },
    );
  }

  Widget _densityBar({bool vertical = false}) {
    final maximum = _hourDensity.values.fold<int>(0, (a, b) => a > b ? a : b);
    final bars = List.generate(24, (h) {
      final count = _hourDensity[h] ?? 0;
      final ratio = maximum == 0 ? 0.0 : count / maximum;
      final barLength = 4.0 + 36.0 * ratio;
      final color = count > 0
          ? Colors.blue.withValues(alpha: 0.3 + 0.7 * ratio)
          : Colors.grey.withValues(alpha: 0.25);
      return Semantics(
        key: ValueKey('tm-density-bar-$h'),
        container: true,
        excludeSemantics: true,
        label: '$h시, $count개',
        child: Tooltip(
          excludeFromSemantics: true,
          message: '$h시, $count개',
          child: Padding(
            padding: vertical
                ? const EdgeInsets.symmetric(vertical: 2.0, horizontal: 4.0)
                : const EdgeInsets.symmetric(horizontal: 2.5),
            child: vertical
                ? Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      SizedBox(
                        width: 14,
                        child: Text(
                          '$h',
                          style: const TextStyle(fontSize: 8),
                          textAlign: TextAlign.right,
                        ),
                      ),
                      const SizedBox(width: 4),
                      Container(
                        width: barLength,
                        height: 8,
                        decoration: BoxDecoration(
                          color: color,
                          borderRadius: BorderRadius.circular(2),
                        ),
                      ),
                    ],
                  )
                : Column(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Container(
                        width: 8,
                        height: barLength,
                        decoration: BoxDecoration(
                          color: color,
                          borderRadius: BorderRadius.circular(2),
                        ),
                      ),
                      const SizedBox(height: 2),
                      Text('$h', style: const TextStyle(fontSize: 8)),
                    ],
                  ),
          ),
        ),
      );
    });

    if (vertical) {
      return SizedBox(
        width: 68,
        child: SingleChildScrollView(
          padding: const EdgeInsets.symmetric(vertical: 8),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: bars,
          ),
        ),
      );
    }

    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 4, 16, 4),
      child: SingleChildScrollView(
        scrollDirection: Axis.horizontal,
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.end,
          children: bars,
        ),
      ),
    );
  }
  Widget _disabledView() {
    final roots = (_settings?['roots'] as List?) ?? const [];
    final disconnected = _error?.contains('연결하지 못했습니다') == true;
    return Center(child:ConstrainedBox(constraints:const BoxConstraints(maxWidth:620),child:Card(child:Padding(padding:const EdgeInsets.all(24),child:Column(mainAxisSize:MainAxisSize.min,crossAxisAlignment:CrossAxisAlignment.start,children:[
      Text('몽땅검색',style:Theme.of(context).textTheme.headlineSmall), const SizedBox(height:8), const Text('켜면 선택한 폴더의 문서 정보만 색인해 시간순으로 찾아볼 수 있습니다.'), const SizedBox(height:16), const Text('조사 폴더'),
      for(final root in roots) Row(children:[Expanded(child:Text('• $root')),IconButton(tooltip:'조사 폴더 삭제',onPressed:(){final next=Map<String,Object?>.from(_settings!)..['roots']=List<String>.from(roots.cast<String>())..remove(root);_saveSettings(next);},icon:const Icon(Icons.close))]),
      OutlinedButton.icon(onPressed:_addRoot,icon:const Icon(Icons.folder_open),label:const Text('조사 폴더 추가')),
      const SizedBox(height:16), FilledButton.icon(onPressed:_enable,icon:const Icon(Icons.power_settings_new),label:const Text('켜기')),
      if (_error != null) Text(_error!,style:const TextStyle(color:Colors.red)),
      if (disconnected) TextButton(onPressed:_retryConnection,child:const Text('다시 연결')),
    ])))));
  }
  Widget _contentSearchPanel() => Padding(
    padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 4),
    child: Wrap(
      spacing: 12,
      crossAxisAlignment: WrapCrossAlignment.center,
      children: [
        Text(_relatedMode ? '위계검색' : '본문검색', style: const TextStyle(fontWeight: FontWeight.bold, fontSize: 13)),
        if (_contentIndexRunning) ...[
          SizedBox(width: 80, child: LinearProgressIndicator(value: idleIndexingProgress(_contentProgress))),
          Text(idleIndexingStatusText(_contentProgress) ?? '${_contentProgress['seen'] ?? 0}건 확인 · ${_contentProgress['indexed'] ?? 0}건 색인 · 실패 ${_contentProgress['errors'] ?? 0}건', key: const ValueKey('tm-content-index-status'), style: const TextStyle(fontSize: 12)),
          TextButton(onPressed: _cancelContentIndex, child: const Text('\uCDE8\uC18C')),
        ] else
          TextButton.icon(
            onPressed: _startContentIndex,
            icon: const Icon(Icons.play_arrow, size: 16),
            label: const Text('\uBCF8\uBB38 \uC0C9\uC778'),
          ),
        Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            const Text('정렬: ', style: TextStyle(fontSize: 12)),
            DropdownButton<String>(
              key: const ValueKey('tm-content-sort-dropdown'),
              value: _contentSort,
              isDense: true,
            items: [
                if (_relatedMode)
                  const DropdownMenuItem(value: 'hierarchy_relevance', child: Text('위계순')),
                const DropdownMenuItem(value: 'relevance', child: Text('관련도순')),
                if (_developerMode)
                  const DropdownMenuItem(value: 'mentions', child: Text('언급 횟수 많은 순')),
                const DropdownMenuItem(value: 'recent', child: Text('수정일순')),
                const DropdownMenuItem(value: 'name', child: Text('이름순')),
              ],
              onChanged: (val) {
                if (val == null) return;
                setState(() => _contentSort = val);
                _searchBox.refresh(options: val);
              },
            ),
          ],
        ),
        Text('검색 결과 ${_contentResults.length}건', style: const TextStyle(fontSize: 12)),
        if (!_relatedMode)
          OutlinedButton.icon(
            key: const ValueKey('tm-export-markdown'),
            onPressed: _search.text.trim().isEmpty ? null : _exportMarkdown,
            icon: const Icon(Icons.download_outlined, size: 16),
            label: const Text('Markdown 내보내기'),
          ),
      ],
    ),
  );
  // 상단 메뉴 반응형 기준(메뉴 줄 안쪽 너비).
  // 900 미만: 새로고침·썸네일 캐시·감시 상태를 '더보기'로 접는다.
  // 600 미만: 종류 칩을 '종류' 메뉴로 접고 검색창을 한 줄 전체로 넓힌다.
  // 340 미만: 검색 방식 버튼을 메뉴 하나로 접는다.
  static const double tmToolsCollapseWidth = 900, tmKindsCollapseWidth = 600, tmModeCollapseWidth = 340;

  void _toggleKind(String kind, bool value) {
    setState(() => value ? _selectedKinds.add(kind) : _selectedKinds.remove(kind));
    _filtersChanged();
    if (_fileTableView && _fileSearchSupported) _runFileSearch(_activeQuery);
  }

  Widget _searchModeSelector({required bool collapsed}) {
    final modes = <(String, String)>[
      ('file', '파일명'),
      ('content', '본문검색'),
      ('related', '위계검색'),
    ];
    final selected = _relatedMode ? 'related' : _contentMode ? 'content' : 'file';
    if (collapsed) {
      final label = modes.firstWhere((m) => m.$1 == selected).$2;
      return PopupMenuButton<String>(
        key: const ValueKey('tm-search-mode-menu'),
        tooltip: '검색 방식',
        onSelected: _selectSearchMode,
        itemBuilder: (context) => [
          for (final m in modes) CheckedPopupMenuItem(value: m.$1, checked: m.$1 == selected, child: Text(m.$2)),
        ],
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
          child: Row(mainAxisSize: MainAxisSize.min, children: [Text(label), const Icon(Icons.arrow_drop_down, size: 18)]),
        ),
      );
    }
    return SegmentedButton<String>(
      key: const ValueKey('tm-search-mode'),
      showSelectedIcon: false,
      style: const ButtonStyle(visualDensity: VisualDensity.compact),
      segments: [for (final m in modes) ButtonSegment(value: m.$1, label: Text(m.$2))],
      selected: {selected},
      onSelectionChanged: (values) => _selectSearchMode(values.first),
    );
  }

  Widget _kindMenu() {
    final count = _selectedKinds.length;
    return PopupMenuButton<String>(
      key: const ValueKey('tm-kind-menu'),
      tooltip: '종류 필터',
      onSelected: (kind) => _toggleKind(kind, !_selectedKinds.contains(kind)),
      itemBuilder: (context) => [
        for (final entry in _kinds.entries)
          CheckedPopupMenuItem(value: entry.value, checked: _selectedKinds.contains(entry.value), child: Text(entry.key)),
      ],
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
        child: Row(mainAxisSize: MainAxisSize.min, children: [
          const Icon(Icons.filter_list, size: 16),
          const SizedBox(width: 4),
          Text(count == 0 ? '종류' : '종류 $count'),
          const Icon(Icons.arrow_drop_down, size: 18),
        ]),
      ),
    );
  }

  /// 파일명·본문검색·위계검색이 함께 쓰는 검색창. 2글자부터 입력하는 대로 찾고 Enter는 바로 찾는다.
  /// 오른쪽 표시는 컨트롤러 상태(검색 중 모래시계)를 그대로 보여준다.
  Widget _searchField() => ListenableBuilder(
    listenable: _searchBox,
    builder: (context, _) => TextField(
      key: const ValueKey('tm-file-search'),
      controller: _search,
      decoration: InputDecoration(
        prefixIcon: const Icon(Icons.search),
        suffixIcon: _searchBox.isSearching
            ? const Tooltip(message: '찾는 중', child: Icon(Icons.hourglass_top, key: ValueKey('tm-search-busy'), size: 16))
            : null,
        hintText: _relatedMode ? '위계 검색' : _contentMode ? '본문 검색' : '이름 찾기',
        isDense: true,
      ),
      onSubmitted: (_) => _searchBox.submit(),
    ),
  );

  Widget _filtersBar() =>Padding(padding:const EdgeInsets.fromLTRB(12,8,12,4),child:LayoutBuilder(builder:(context, constraints){
    final width = constraints.maxWidth;
    final compact = width < tmToolsCollapseWidth;
    final narrow = width < tmKindsCollapseWidth;
    final tools = <Widget>[
      OutlinedButton.icon(onPressed:_scan,icon:const Icon(Icons.refresh),label:const Text('다시 훑기')),
      OutlinedButton.icon(key:const ValueKey('tm-clear-thumb-cache'),onPressed:_clearThumbnailCache,icon:const Icon(Icons.delete_outline),label:const Text('썸네일 캐시 지우기')),
      Row(mainAxisSize:MainAxisSize.min,children:[Icon(_watchByAgent?Icons.sync:Icons.hourglass_empty,size:18),const SizedBox(width:4),Text(_watchByAgent?'실시간 감시: K에이전트':'실시간 감시: 앱을 열 때만',key:const ValueKey('tm-watch-status'),style:Theme.of(context).textTheme.bodySmall)]),
    ];
    final children=<Widget>[
    OutlinedButton.icon(key:const ValueKey('tm-extensions'),onPressed:_pickExtensions,icon:const Icon(Icons.tune,size:16),label:const Text('확장자')),
    if (narrow) _kindMenu()
    else for(final entry in _kinds.entries) FilterChip(label:Text(entry.key),selected:_selectedKinds.contains(entry.value),onSelected:(value)=>_toggleKind(entry.value, value)),
    _searchModeSelector(collapsed: width < tmModeCollapseWidth),
    // 좁은 창에서는 검색창을 넓히되 오른쪽 아이콘 두 개(도움말/열 보기, 더보기)가 같은 줄에 남게 한다.
    SizedBox(width:narrow ? max(120.0, width - 120) : 180,child:_searchField()),
    if (_contentMode && !_relatedMode)
      IconButton(
        icon: const Icon(Icons.help_outline),
        tooltip: '검색 연산자 도움말',
        onPressed: () => setState(() => _showOperatorHelp = !_showOperatorHelp),
      )
    else if (_fileSearchSupported)
      IconButton(
        icon: Icon(_fileTableView ? Icons.view_timeline : Icons.table_chart),
        tooltip: _fileTableView ? '타임라인 보기' : '열 보기',
        onPressed: () {
          if (!_fileSearchSupported) return;
          setState(() => _fileTableView = !_fileTableView);
          if (_fileTableView) {
            _runFileSearch(_activeQuery);
          }
        },
      ),
    if(!compact)...tools,
    ];
    if(compact){
      children.add(PopupMenuButton<int>(key:const ValueKey('tm-more-menu'),tooltip:'더보기',icon:const Icon(Icons.more_horiz),onSelected:(index){if(index==0)_scan();if(index==1)_clearThumbnailCache();},itemBuilder:(context)=>[
        const PopupMenuItem(value:0,child:ListTile(leading:Icon(Icons.refresh),title:Text('다시 훑기'),contentPadding:EdgeInsets.zero)),
        const PopupMenuItem(value:1,child:ListTile(leading:Icon(Icons.delete_outline),title:Text('썸네일 캐시 지우기'),contentPadding:EdgeInsets.zero)),
        PopupMenuItem(value:2,enabled:false,child:ListTile(leading:Icon(_watchByAgent?Icons.sync:Icons.hourglass_empty),title:Text(_watchByAgent?'실시간 감시: K에이전트':'실시간 감시: 앱을 열 때만'),contentPadding:EdgeInsets.zero)),
      ]));
    }
    return Wrap(spacing:8,runSpacing:6,alignment:WrapAlignment.start,crossAxisAlignment:WrapCrossAlignment.center,children:children);
  }));
  Widget _calendar() {
    final first = DateTime(_month.year,_month.month,1); final offset = first.weekday%7; final cells = ((offset+DateTime(_month.year,_month.month+1,0).day+6)~/7)*7;
    final maximum = _days.values.fold<int>(0,(a,b)=>a>b?a:b);
    return Card(margin:const EdgeInsets.all(12),child:Padding(padding:const EdgeInsets.all(12),child:Column(mainAxisSize:MainAxisSize.min,children:[
      Row(children:[IconButton(onPressed:()=>_moveMonth(-1),icon:const Icon(Icons.chevron_left)),Expanded(child:Center(child:Text('${_month.year}년 ${_month.month}월',style:Theme.of(context).textTheme.titleMedium))),IconButton(onPressed:()=>_moveMonth(1),icon:const Icon(Icons.chevron_right))]),
      Row(key:const ValueKey('tm-calendar-weekdays'),mainAxisAlignment:MainAxisAlignment.spaceAround,children:const ['일','월','화','수','목','금','토'].map((e)=>SizedBox(width:36,child:Center(child:Text(e)))).toList()),
      for(var row=0;row<cells~/7;row++) Row(mainAxisAlignment:MainAxisAlignment.spaceAround,children:[for(var col=0;col<7;col++) _dateCell(row*7+col-offset,DateTime(_month.year,_month.month,1+row*7+col-offset), maximum)]),
      Wrap(spacing:4,children:[TextButton(onPressed:()=>_quickRange('today'),child:const Text('오늘')),TextButton(onPressed:()=>_quickRange('week'),child:const Text('이번 주')),TextButton(onPressed:()=>_quickRange('month'),child:const Text('이번 달'))]),
    ])));
  }
  Widget _dateCell(int index,DateTime date,int maximum) {
    if(index<0 || date.month!=_month.month) return const SizedBox(width:36,height:36);
    final count=_days['${date.day}']??0; final alpha=maximum==0?0.04:0.08+0.42*count/maximum;
    return SizedBox(width:36,height:36,child:TextButton(key:ValueKey('tm-day-${date.day}'),style:TextButton.styleFrom(backgroundColor:Colors.blue.withValues(alpha:alpha),padding:EdgeInsets.zero),onPressed:()=>_chooseDay(date),child:Text('${date.day}')));
  }
  Widget _timeline() {
    if (_items.isEmpty && !_loading) return const Center(child: Padding(padding: EdgeInsets.all(24), child: Text('이 기간에 기록된 파일이 없습니다.')));
    final entries = _groupTimeline(_items);
    final count = entries.length + (_loading || _more ? 1 : 0);
    return ListView.builder(
      controller: _scroll,
      itemCount: count,
      itemBuilder: (context, index) {
        if (index >= entries.length) {
          if (_loading) return const Center(child: Padding(padding: EdgeInsets.all(16), child: CircularProgressIndicator()));
          return Center(child: Padding(padding: const EdgeInsets.all(16), child: TextButton(onPressed: _loadPage, child: const Text('더 불러오기'))));
        }
        final entry = entries[index];
        final entryDate = entry.isGroup ? entry.hourStart! : entry.localTime!;
        final day = DateTime(entryDate.year, entryDate.month, entryDate.day);
        final prevDate = index == 0 ? null : (entries[index - 1].isGroup ? entries[index - 1].hourStart! : entries[index - 1].localTime!);
        final prevDay = prevDate == null ? null : DateTime(prevDate.year, prevDate.month, prevDate.day);
        final showHeading = index == 0 || prevDay == null || !_sameDay(day, prevDay);
        return Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          mainAxisSize: MainAxisSize.min,
          children: [
            if (showHeading)
              Padding(padding: const EdgeInsets.fromLTRB(24, 14, 8, 6), child: Text(_localizedDayHeading(day), style: Theme.of(context).textTheme.titleSmall)),
            if (entry.isGroup)
              _hourGroupCard(entry)
            else
              _itemCard(entry.item!, entry.localTime!),
          ],
        );
      },
    );
  }

  Widget _hourGroupCard(_TimelineEntry group) {
    final hourStart = group.hourStart!;
    final keyStr = hourStart.toIso8601String();
    final isExpanded = _expandedHours.contains(keyStr);
    final label = _hourGroupLabel(hourStart, group.files.length);
    return Padding(
      padding: const EdgeInsets.fromLTRB(18, 2, 12, 8),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          Card(
            key: ValueKey('tm-group-$keyStr'),
            child: InkWell(
              onTap: () {
                setState(() {
                  if (isExpanded) {
                    _expandedHours.remove(keyStr);
                  } else {
                    _expandedHours.add(keyStr);
                  }
                });
              },
              child: Padding(
                padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
                child: Row(
                  children: [
                    const Icon(Icons.folder_zip_outlined, color: Colors.blue),
                    const SizedBox(width: 12),
                    Expanded(
                      child: Text(label, style: const TextStyle(fontWeight: FontWeight.w600)),
                    ),
                    Icon(isExpanded ? Icons.expand_less : Icons.expand_more),
                  ],
                ),
              ),
            ),
          ),
          if (isExpanded)
            for (final file in group.files)
              _itemCard(file, DateTime.fromMillisecondsSinceEpoch((file.workTime * 1000).round()).toLocal()),
        ],
      ),
    );
  }

  Widget _itemCard(TimemachineItem item, DateTime date) => Padding(
    key: ValueKey('tm-row-${item.id}'),
    padding: const EdgeInsets.fromLTRB(18, 2, 12, 8),
    child: Row(
      crossAxisAlignment: CrossAxisAlignment.center,
      children: [
        SizedBox(
          width: 24,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Container(width: 2, height: 8, color: Colors.blue),
              Container(width: 2, height: 32, color: Colors.blue.withValues(alpha: .35)),
              Container(width: 9, height: 9, decoration: const BoxDecoration(color: Colors.blue, shape: BoxShape.circle)),
            ],
          ),
        ),
        Expanded(
          child: Card(
            child: InkWell(
              onTap: () => _openItem(item),
              child: Padding(
                padding: const EdgeInsets.all(12),
                child: Row(
                  children: [
                    TimemachineThumbnail(item: item, icon: _kindIcon(item.kind), loader: _thumbnails),
                    const SizedBox(width: 10),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          _marked(item.name, _contentMode ? '' : _query, maxLines: 1, style: const TextStyle(fontWeight: FontWeight.w600)),
                          Text('${date.hour.toString().padLeft(2, '0')}:${date.minute.toString().padLeft(2, '0')}  ·  ${item.folder}', maxLines: 1, overflow: TextOverflow.ellipsis, style: Theme.of(context).textTheme.bodySmall),
                          Wrap(spacing: 4, children: [if (item.flags.contains('copied')) const _Badge('복사됨'), if (item.flags.contains('new')) const _Badge('새 문서')]),
                        ],
                      ),
                    ),
                    TextButton(onPressed: () => _revealItem(item), child: const Text('폴더 열기')),
                    PopupMenuButton<String>(
                      key: ValueKey('tm-send-menu-${item.id}'),
                      tooltip: '다른 기능으로 보내기',
                      icon: const Icon(Icons.more_vert),
                      onSelected: (feature) { if (feature == 'related') { _findRelated(item); } else { _sendToFeature(feature, item.id); } },
                      itemBuilder: (context) => const [
                        PopupMenuItem(value: 'related', child: Text('연관 문서 찾기')),
                        PopupMenuItem(value: 'rename_batch', child: Text('파일명 일괄변경 목록에 추가')),
                        PopupMenuItem(value: 'convert', child: Text('파일변환 목록에 추가')),
                        PopupMenuItem(value: 'document_edit', child: Text('문서편집 목록에 추가')),
                      ],
                    ),
                  ],
                ),
              ),
            ),
          ),
        ),
      ],
    ),
  );
}

class _HierarchyBreadcrumb extends StatelessWidget {
  const _HierarchyBreadcrumb({required this.text});

  final String text;

  @override
  Widget build(BuildContext context) => Text(
        text,
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
        style: Theme.of(context).textTheme.labelSmall?.copyWith(color: Colors.grey),
      );
}

class _TimelineEntry {
  const _TimelineEntry.file(this.item, this.localTime) : files = const [], hourStart = null;
  const _TimelineEntry.group(this.hourStart, this.files) : item = null, localTime = null;
  final TimemachineItem? item;
  final DateTime? localTime;
  final DateTime? hourStart; // 지역 시각 정각
  final List<TimemachineItem> files;
  bool get isGroup => hourStart != null;
}

List<_TimelineEntry> _groupTimeline(List<TimemachineItem> newestFirst) {
  if (newestFirst.isEmpty) return const [];
  final entries = <_TimelineEntry>[];
  var i = 0;
  while (i < newestFirst.length) {
    final firstItem = newestFirst[i];
    final firstDate = DateTime.fromMillisecondsSinceEpoch((firstItem.workTime * 1000).round()).toLocal();
    final hourWindow = DateTime(firstDate.year, firstDate.month, firstDate.day, firstDate.hour);

    var j = i + 1;
    while (j < newestFirst.length) {
      final nextItem = newestFirst[j];
      final nextDate = DateTime.fromMillisecondsSinceEpoch((nextItem.workTime * 1000).round()).toLocal();
      final nextWindow = DateTime(nextDate.year, nextDate.month, nextDate.day, nextDate.hour);
      if (nextWindow != hourWindow) {
        break;
      }
      j++;
    }

    final count = j - i;
    if (count > 5) {
      final groupFiles = newestFirst.sublist(i, j);
      entries.add(_TimelineEntry.group(hourWindow, groupFiles));
    } else {
      for (var k = i; k < j; k++) {
        final item = newestFirst[k];
        final local = DateTime.fromMillisecondsSinceEpoch((item.workTime * 1000).round()).toLocal();
        entries.add(_TimelineEntry.file(item, local));
      }
    }
    i = j;
  }
  return entries;
}

String _hourGroupLabel(DateTime hourStart, int count) {
  if (hourStart.hour == 23) {
    return '23:00~24:00 파일 $count개';
  }
  final startStr = hourStart.hour.toString().padLeft(2, '0');
  final endStr = (hourStart.hour + 1).toString().padLeft(2, '0');
  return '$startStr:00~$endStr:00 파일 $count개';
}

class _Badge extends StatelessWidget {
  const _Badge(this.label); final String label;
  @override Widget build(BuildContext context)=>Container(padding:const EdgeInsets.symmetric(horizontal:6,vertical:2),decoration:BoxDecoration(color:Theme.of(context).colorScheme.secondaryContainer,borderRadius:BorderRadius.circular(8)),child:Text(label,style:Theme.of(context).textTheme.labelSmall));
}
