/// 실시간 검색 공용 컨트롤러(ISUI1, 사용자 지시 2026-11-12).
///
/// 검색창 하나를 여러 탭(메뉴)이 함께 쓴다. 키워드는 탭을 바꿔도 남고, 바뀐 탭이 그 키워드로 곧바로 검색한다.
///
/// 상태 기계([InstantSearchPhase])
/// - empty: 유효 검색어가 [InstantSearchController.minLength] 미만. 결과를 비운다(onCleared).
/// - typing: 유효 검색어가 기준 이상이고 지연 타이머가 도는 중.
/// - searching: 요청을 보냈고 응답을 기다리는 중.
/// - results: 현재 요청의 응답을 받아 onResult로 넘겼다.
/// - error: 현재 요청이 예외로 끝났다(onError).
///
/// 전이
/// - 입력 변경 → (기준 미만) empty / (기준 이상) typing → 지연 뒤 searching
/// - Enter([submit]) → 지연 없이 searching (1글자도 검색: 짧은 이름을 일부러 찾는 길)
/// - 탭 변경([selectTab]) → 지연 없이 새 탭으로 searching(기준 미만이면 empty)
/// - searching → results | error. 그 사이 새 요청이 생기면 앞 응답은 버린다(요청 번호 비교).
/// 불가능한 전이: 응답이 늦게 온 옛 요청이 results/error로 바꾸는 것(요청 번호가 다르면 무시).
///
/// 백엔드 부담(ISUI1 조사): 데스크톱 IPC는 한 연결에서 요청을 차례로 처리하고 디스패치 전체를 전역 잠금으로
/// 감싸므로(docfit_app/ipc_server.py) 보낸 요청을 서버에서 취소할 수 없고, 겹쳐 보내면 뒤 요청이 줄을 선다.
/// 그래서 동시에 1건만 보내고, 그 사이 들어온 입력은 "가장 마지막 것 1건"만 남겨 앞 요청이 끝나면 보낸다.
///
/// 한글 조합(IME composing): 조합 중인 글자가 완성 음절(가~힣)이면 검색어에 넣고, 낱자모(ㄱ, ㅏ 등)가 섞이면
/// 그 조합 구간은 빼고 확정된 글자만 쓴다. 윈도 한글 IME는 마지막 음절을 다음 글쇠까지 조합 상태로 두므로
/// 조합 구간을 통째로 빼면 "회의"를 쳐도 "회"로만 검색되기 때문이다.
library;

import 'dart:async';

import 'package:flutter/widgets.dart';

enum InstantSearchPhase { empty, typing, searching, results, error }

/// 한 번의 검색 요청. [id]가 컨트롤러의 현재 요청 번호와 같을 때만 결과가 반영된다.
@immutable
class InstantSearchRequest {
  const InstantSearchRequest({required this.id, required this.tab, required this.query, required this.trigger, this.options});
  final int id;
  final String tab;
  final String query;

  /// 'input'(입력 뒤 지연), 'submit'(Enter), 'tab'(탭 전환), 'refresh'(정렬 등 조건 변경).
  final String trigger;

  /// [InstantSearchController.refresh]로 넘긴 조건(예: 고른 정렬). 다른 요청은 null.
  final Object? options;

  @override
  String toString() => 'InstantSearchRequest(#$id $tab "$query" $trigger)';
}

typedef InstantSearchRunner<R> = Future<R> Function(InstantSearchRequest request);

class InstantSearchController<R> extends ChangeNotifier {
  InstantSearchController({
    required String initialTab,
    required this.search,
    this.onResult,
    this.onCleared,
    this.onError,
    this.minLength = 2,
    // 150ms: 한글 두벌식 자모 입력 간격(대개 150~250ms)보다 짧아 손을 멈추면 바로 검색이 나가고,
    // 연속 입력 중에는 대부분 한 번으로 합쳐진다. 백엔드는 요청을 차례로 처리하므로(첫 본문 질의 수 초)
    // 0ms로 두면 글자마다 요청이 줄을 선다. 카드 상한(0~150ms) 안에서 요청 수를 가장 줄이는 값.
    this.delay = const Duration(milliseconds: 150),
    TextEditingController? text,
  })  : _tab = initialTab,
        text = text ?? TextEditingController(),
        _ownsText = text == null {
    _lastValue = this.text.value;
    this.text.addListener(_onTextChanged);
  }

  final InstantSearchRunner<R> search;
  final void Function(InstantSearchRequest request, R result)? onResult;

  /// 유효 검색어가 기준 미만이 되었을 때(탭 이름, 검색어). 화면은 결과를 비운다.
  final void Function(String tab, String query)? onCleared;
  final void Function(InstantSearchRequest request, Object error)? onError;
  final int minLength;
  final Duration delay;
  final TextEditingController text;
  final bool _ownsText;

  String _tab;
  InstantSearchPhase _phase = InstantSearchPhase.empty;
  String _query = '';
  int _requestId = 0;
  Timer? _timer;
  InstantSearchRequest? _inFlight;
  InstantSearchRequest? _queued;
  InstantSearchRequest? _lastRun;
  String? _submittedQuery;
  R? _result;
  Object? _error;
  late TextEditingValue _lastValue;
  bool _disposed = false;

  String get tab => _tab;
  InstantSearchPhase get phase => _phase;

  /// 지금 검색에 쓰는 유효 검색어(앞뒤 공백 제거, 조합 중 낱자모 제외).
  String get query => _query;
  int get requestId => _requestId;
  R? get result => _result;
  Object? get error => _error;
  bool get isSearching => _phase == InstantSearchPhase.searching;

  /// 에이전트·로그용 한 줄 상태.
  String describe() => 'tab=$_tab phase=${_phase.name} query="$_query" request=$_requestId'
      '${_inFlight != null ? ' inFlight=${_inFlight!.id}' : ''}${_queued != null ? ' queued=${_queued!.id}' : ''}';

  /// 조합 중 낱자모를 뺀 유효 검색어.
  static String effectiveQuery(TextEditingValue value) {
    final composing = value.composing;
    var raw = value.text;
    if (composing.isValid && !composing.isCollapsed && composing.end <= raw.length) {
      final part = composing.textInside(raw);
      if (!_isCompleteHangul(part)) raw = composing.textBefore(raw) + composing.textAfter(raw);
    }
    return raw.trim();
  }

  static bool _isCompleteHangul(String part) {
    for (final rune in part.runes) {
      final jamo = (rune >= 0x1100 && rune <= 0x11FF) || (rune >= 0x3130 && rune <= 0x318F) || (rune >= 0xA960 && rune <= 0xA97F) || (rune >= 0xD7B0 && rune <= 0xD7FF);
      if (jamo) return false;
    }
    return true;
  }

  void _onTextChanged() {
    final value = text.value;
    if (value.text == _lastValue.text && value.composing == _lastValue.composing) return; // 커서 이동만
    _lastValue = value;
    final next = effectiveQuery(value);
    if (next == _query) return; // 조합 확정 등으로 유효 검색어가 그대로면 다시 찾지 않는다.
    _query = next;
    _submittedQuery = null;
    _timer?.cancel();
    if (next.length < minLength) {
      _clear();
      return;
    }
    if (delay == Duration.zero) {
      _start('input'); // 지연 0(목록 거르기): 타이머 없이 바로 찾는다.
      return;
    }
    _setPhase(InstantSearchPhase.typing);
    _timer = Timer(delay, () => _start('input'));
  }

  /// Enter: 지연 없이 지금 검색어로 찾는다. 1글자도 찾는다(빈 검색어는 비움).
  void submit() {
    _timer?.cancel();
    _query = effectiveQuery(text.value);
    if (_query.isEmpty) {
      _clear();
      return;
    }
    _submittedQuery = _query;
    _start('submit');
  }

  /// 탭(메뉴) 전환: 키워드는 그대로 두고 새 탭에서 곧바로 검색한다.
  void selectTab(String tab) {
    if (tab == _tab) return;
    _tab = tab;
    _timer?.cancel();
    if (_canSearch) {
      _start('tab');
    } else {
      _clear();
    }
  }

  /// 정렬 같은 조건이 바뀌어 같은 검색어로 다시 찾는다. 조건은 [options]로 그 요청에만 붙는다.
  /// 검색어가 기준 미만이라 찾지 않았으면 false.
  bool refresh({Object? options}) {
    _timer?.cancel();
    if (!_canSearch) return false;
    _start('refresh', options: options);
    return true;
  }

  /// 화면 초기화(예: 개발자 모드 끔): 콜백 없이 검색어·탭·결과를 비운다.
  void reset({String? tab}) {
    _timer?.cancel();
    _requestId++;
    _queued = null;
    if (tab != null) _tab = tab;
    _query = '';
    _submittedQuery = null;
    _result = null;
    _error = null;
    _lastValue = const TextEditingValue();
    text.value = const TextEditingValue();
    _setPhase(InstantSearchPhase.empty);
  }

  bool get _canSearch => _query.length >= minLength || (_query.isNotEmpty && _query == _submittedQuery);

  void _clear() {
    _timer?.cancel();
    _requestId++; // 진행 중 응답은 버린다.
    _queued = null;
    _result = null;
    _error = null;
    _setPhase(InstantSearchPhase.empty);
    onCleared?.call(_tab, _query);
  }

  void _start(String trigger, {Object? options}) {
    final request = InstantSearchRequest(id: ++_requestId, tab: _tab, query: _query, trigger: trigger, options: options);
    _setPhase(InstantSearchPhase.searching);
    if (_inFlight != null) {
      _queued = request; // 앞 요청이 끝나면 가장 마지막 요청 1건만 보낸다.
      return;
    }
    unawaited(_run(request));
  }

  Future<void> _run(InstantSearchRequest request) async {
    _inFlight = request;
    _lastRun = request;
    try {
      final value = await search(request);
      if (_disposed || request.id != _requestId) return;
      _result = value;
      _error = null;
      onResult?.call(request, value);
      _setPhase(InstantSearchPhase.results);
    } catch (error) {
      if (_disposed || request.id != _requestId) return;
      _error = error;
      onError?.call(request, error);
      _setPhase(InstantSearchPhase.error);
    } finally {
      if (identical(_inFlight, request)) _inFlight = null;
      final next = _queued;
      _queued = null;
      if (!_disposed && next != null && next.id == _requestId) unawaited(_run(next));
    }
  }

  /// 시험·진단용: 마지막으로 실제로 보낸 요청.
  @visibleForTesting
  InstantSearchRequest? get lastRun => _lastRun;

  void _setPhase(InstantSearchPhase phase) {
    if (_disposed) return;
    _phase = phase;
    notifyListeners();
  }

  @override
  void dispose() {
    _disposed = true;
    _timer?.cancel();
    text.removeListener(_onTextChanged);
    if (_ownsText) text.dispose();
    super.dispose();
  }
}

/// 받아 둔 목록을 화면 안에서 거르는 검색창용(ISUI3, 사용자 결정 2026-11-12).
///
/// 표 단위·준말본말·상용구·설정 검색이 쓴다. 백엔드를 부르지 않으므로 지연 0이고, "검색 결과"는 거를 검색어 자체다.
/// - 2글자 이상: 입력하는 즉시 거른다.
/// - 1글자: 거르지 않고 전체 목록. Enter([submit])를 누르면 그 1글자로 거른다.
/// - 빈 검색어: 전체 목록.
/// 화면은 [filterQuery]로 목록을 거르고, 이 컨트롤러를 듣고(addListener) 다시 그린다.
class InstantFilterController extends InstantSearchController<String> {
  InstantFilterController({super.text, super.minLength})
      : super(initialTab: 'filter', search: _echo, delay: Duration.zero);

  static Future<String> _echo(InstantSearchRequest request) => Future.value(request.query);

  /// 목록을 거를 검색어. 기준 미만이라 거르지 않으면 ''.
  String get filterQuery => phase == InstantSearchPhase.empty ? '' : (result ?? '');
}
