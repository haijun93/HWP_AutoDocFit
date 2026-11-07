/// 큰 글 보기(3단계 EDCORE3)의 문서 상태. 글 전체를 Flutter로 가져오지 않는다.
///
/// - Rust 코어(klib edit_*)에 열린 문서가 원본이다. 화면은 보이는 줄만 `edit_line_range`로 [blockLines]줄씩 받아 캐시한다
///   (응답 한도 64 KiB를 넘으면 줄 수를 반씩 줄여 다시 받고, 한 줄이 그보다 길면 '너무 긴 줄'로 표시만 한다).
/// - 편집은 줄 단위다: 화면이 편집 중인 한 줄(문단)만 입력칸으로 두고(IME 조합은 그 TextField가 처리),
///   확정되면 [replaceLine]·[joinWithPrevious]가 그 줄의 바이트 구간을 `edit_replace`로 바꾼다.
///   캐시는 응답을 기다리지 않고 같은 규칙으로 고쳐(뒤쪽 줄의 시작 바이트·줄 번호 이동) 다시 받지 않는다.
/// - 실행취소·다시실행·모두 바꾸기는 어디가 바뀌었는지 모르므로 캐시를 비우고 보이는 줄을 다시 받는다.
/// - 요청은 보낸 순서대로 하나씩 처리하고(요청 줄), 편집 뒤에 도착한 옛 줄 응답은 버전 번호로 버린다.
library;

import 'dart:async';

import 'package:flutter/foundation.dart' show ChangeNotifier;

import 'editor_files.dart';
import 'klib_editor_core.dart' show EditorBackend, EditorBackendError;
import 'text_offsets.dart';

int _int(Object? v) => v is num ? v.toInt() : int.tryParse('$v') ?? 0;

/// 캐시된 한 줄: 시작 바이트(UTF-8)와 글(줄바꿈 없음).
class LargeLine {
  const LargeLine(this.start, this.text, {this.tooLong = false});
  final int start;
  final String text;

  /// 응답 한도를 넘는 아주 긴 줄(표시만, 편집 불가).
  final bool tooLong;

  int get byteLength => utf8Length(text);
}

/// 찾은 자리: 줄(0부터)과 그 줄 안의 UTF-16 구간.
class LargeMatch {
  const LargeMatch(this.line, this.start, this.end);
  final int line;
  final int start;
  final int end;
}

class LargeTextSession extends ChangeNotifier implements EditorFileSupport {
  LargeTextSession(
    this._backend, {
    required this.doc,
    required Map<String, Object?> info,
    this.blockLines = 120,
    this.keepAlive = const Duration(seconds: 30),
    this.cacheLimit = 20000,
  }) {
    _apply(info);
    if (info['name'] is String) _fileName = info['name'] as String;
    if (keepAlive > Duration.zero) _timer = Timer.periodic(keepAlive, (_) => unawaited(_heartbeat()));
  }

  final EditorBackend _backend;
  /// Rust 코어 문서 번호.
  final int doc;

  /// 한 번에 받는 줄 수(응답 64 KiB 안: 줄당 약 500바이트까지).
  final int blockLines;

  /// klib 유휴 종료를 막는 주기. 0이면 끈다(시험용).
  final Duration keepAlive;

  /// 캐시 줄 수 상한(넘으면 지금 보는 곳에서 먼 줄부터 버린다).
  final int cacheLimit;

  final _cache = <int, LargeLine>{};
  final _loading = <int>{};
  int _version = 0;
  Future<void> _tail = Future.value();
  Timer? _timer;
  bool _disposed = false;

  int totalLines = 1;
  int chars = 0;
  int bytes = 0;
  bool canUndo = false;
  bool canRedo = false;
  bool dirty = false;
  bool mixedLineEndings = false;
  String _encoding = 'UTF-8';
  String _lineEnding = 'CRLF';
  String? _fileName;

  /// klib가 다시 시작돼 문서를 잃었으면 그 안내(복구 불가: 원본은 Rust에만 있었다).
  String? lostMessage;

  /// 줄 범위 요청 횟수(시험·진단: 보이는 줄만 가져오는지).
  int rangeRequests = 0;

  @override
  String get encoding => _encoding;

  @override
  String get lineEnding => _lineEnding;

  @override
  String? get fileName => _fileName;

  @override
  bool get canSaveToFile => lostMessage == null && !_disposed;

  int get cachedLines => _cache.length;

  void _notify() {
    if (!_disposed) notifyListeners();
  }

  void _apply(Map<String, Object?> r) {
    if (r.containsKey('lines')) totalLines = _int(r['lines']).clamp(1, 1 << 40);
    if (r.containsKey('total_lines')) totalLines = _int(r['total_lines']).clamp(1, 1 << 40);
    if (r.containsKey('chars')) chars = _int(r['chars']);
    if (r.containsKey('len')) bytes = _int(r['len']);
    if (r['can_undo'] is bool) canUndo = r['can_undo'] as bool;
    if (r['can_redo'] is bool) canRedo = r['can_redo'] as bool;
    if (r['dirty'] is bool) dirty = r['dirty'] as bool;
    if (r['mixed_line_endings'] is bool) mixedLineEndings = r['mixed_line_endings'] as bool;
    if (r['encoding'] is String) _encoding = r['encoding'] as String;
    if (r['line_ending'] is String) _lineEnding = r['line_ending'] as String;
  }

  Future<T> _enqueue<T>(Future<T> Function() job) {
    final run = _tail.then((_) => job());
    _tail = run.then((_) {}, onError: (Object _) {});
    return run;
  }

  Future<Map<String, Object?>> _send(String cmd, [Map<String, Object?> args = const {}]) async {
    if (lostMessage != null) throw EditorBackendError(lostMessage!);
    final Map<String, Object?> r;
    try {
      r = await _backend({'cmd': cmd, 'doc': doc, ...args});
    } catch (e) {
      throw EditorBackendError('$e');
    }
    if (r['ok'] != true) {
      final error = EditorBackendError('${r['message'] ?? '편집 코어 요청이 실패했습니다.'}');
      if (error.lostDocument) {
        lostMessage = '편집 코어(klib)가 다시 시작돼 열린 파일을 잃었습니다. 파일을 다시 여세요.';
        _notify();
      }
      throw error;
    }
    _apply(r);
    return r;
  }

  Future<void> _heartbeat() async {
    if (lostMessage != null || _disposed) return;
    try {
      await _enqueue(() => _send('edit_info'));
      _notify();
    } catch (_) {}
  }

  // ---- 줄 읽기 ----

  /// 캐시된 줄. 없으면 그 줄이 든 묶음을 받아 오도록 요청하고 null(받으면 알림).
  LargeLine? lineAt(int line) {
    final hit = _cache[line];
    if (hit != null || line < 0 || line >= totalLines) return hit;
    final block = line ~/ blockLines;
    if (_loading.add(block) && lostMessage == null && !_disposed) {
      final gen = _version;
      unawaited(
        _enqueue(() => _loadRange(block * blockLines, blockLines, gen)).then((_) {}, onError: (Object _) {}).whenComplete(() {
          _loading.remove(block);
          _trim(line);
          _notify();
        }),
      );
    }
    return null;
  }

  /// 캐시에 없으면 받아서 돌려준다(찾기·이동용).
  Future<LargeLine?> ensureLine(int line) async {
    final hit = _cache[line];
    if (hit != null) return hit;
    final gen = _version;
    await _enqueue(() => _loadRange(line, 1, gen));
    return _cache[line];
  }

  Future<void> _loadRange(int start, int count, int gen) async {
    if (gen != _version || _disposed || start >= totalLines) return;
    final Map<String, Object?> r;
    try {
      rangeRequests++;
      r = await _send('edit_line_range', {'start_line': start, 'count': count});
    } on EditorBackendError catch (e) {
      if (e.lostDocument) return;
      if (count > 1) {
        // 응답이 너무 크다(긴 줄): 반씩 나눠 다시 받는다.
        final half = count ~/ 2;
        await _loadRange(start, half, gen);
        await _loadRange(start + half, count - half, gen);
      } else if (gen == _version) {
        _cache[start] = const LargeLine(-1, '(이 줄은 너무 길어 큰 파일 보기에서 표시할 수 없습니다)', tooLong: true);
      }
      return;
    }
    if (gen != _version) return; // 그사이 편집됨: 옛 위치 정보라 버린다
    for (final row in (r['lines'] as List? ?? const [])) {
      if (row is! Map) continue;
      _cache[_int(row['line'])] = LargeLine(_int(row['start']), '${row['text'] ?? ''}');
    }
  }

  void _trim(int around) {
    if (_cache.length <= cacheLimit) return;
    final keep = cacheLimit ~/ 2;
    _cache.removeWhere((line, _) => (line - around).abs() > keep);
  }

  void _invalidate() {
    _version++;
    _cache.clear();
    _loading.clear();
  }

  // ---- 편집(줄 단위) ----

  /// [line] 줄 전체를 [text]로 바꾼다([text]에 줄바꿈이 있으면 줄이 나뉜다). 한 번의 확정 = 실행취소 한 단계.
  Future<void> replaceLine(int line, String text) async {
    final old = _cache[line];
    if (old == null || old.tooLong) throw EditorBackendError('이 줄은 아직 편집할 수 없습니다.');
    final next = text.replaceAll('\r\n', '\n').replaceAll('\r', '\n');
    if (next == old.text) return;
    final start = old.start;
    final end = start + old.byteLength;
    final r = await _enqueue(() async {
      await _send('edit_commit');
      return _send('edit_replace', {'start': start, 'end': end, 'text': next});
    });
    _apply(r);
    // 캐시를 같은 규칙으로 고친다: 이 줄은 나뉜 줄들로, 뒤쪽 줄은 줄 번호 +k, 시작 바이트 +delta.
    final parts = next.split('\n');
    final k = parts.length - 1;
    final delta = utf8Length(next) - old.byteLength;
    final moved = <int, LargeLine>{};
    _cache.removeWhere((i, l) {
      if (i < line) return false;
      if (i > line) moved[i + k] = LargeLine(l.start + delta, l.text, tooLong: l.tooLong);
      return true;
    });
    var at = start;
    for (var j = 0; j < parts.length; j++) {
      _cache[line + j] = LargeLine(at, parts[j]);
      at += utf8Length(parts[j]) + 1;
    }
    _cache.addAll(moved);
    _version++; // 편집 전에 보낸 줄 요청의 응답은 버린다
    _loading.clear();
    _notify();
  }

  /// [line] 줄을 앞 줄에 붙인다(줄 맨 앞 Backspace). 붙은 자리(앞 줄의 UTF-16 길이)를 돌려준다.
  Future<int> joinWithPrevious(int line) async {
    final prev = _cache[line - 1];
    final cur = _cache[line];
    if (line <= 0 || prev == null || cur == null || prev.tooLong || cur.tooLong) {
      throw EditorBackendError('이 줄은 아직 편집할 수 없습니다.');
    }
    final r = await _enqueue(() async {
      await _send('edit_commit');
      return _send('edit_replace', {'start': cur.start - 1, 'end': cur.start, 'text': ''});
    });
    _apply(r);
    final moved = <int, LargeLine>{};
    _cache.removeWhere((i, l) {
      if (i < line - 1) return false;
      if (i > line) moved[i - 1] = LargeLine(l.start - 1, l.text, tooLong: l.tooLong);
      return true;
    });
    _cache[line - 1] = LargeLine(prev.start, prev.text + cur.text);
    _cache.addAll(moved);
    _version++;
    _loading.clear();
    _notify();
    return prev.text.length;
  }

  /// 바이트 위치 → 줄(0부터).
  Future<int> lineOf(int byte) async {
    final r = await _enqueue(() => _send('edit_line_col', {'pos': byte}));
    return (_int(r['line']) - 1).clamp(0, totalLines - 1);
  }

  /// 실행취소·다시실행. 바뀌었으면 커서 바이트 위치, 아니면 null.
  Future<int?> history({required bool redo}) async {
    final r = await _enqueue(() => _send(redo ? 'edit_redo' : 'edit_undo'));
    if (r['changed'] != true) return null;
    _invalidate();
    _notify();
    return _int(r['cursor']);
  }

  // ---- 찾기·바꾸기 ----

  Future<LargeMatch?> _matchAt(int startByte, int endByte) async {
    final line = await lineOf(startByte);
    final l = await ensureLine(line);
    if (l == null || l.tooLong) return LargeMatch(line, 0, 0);
    final a = utf8ToUtf16(l.text, startByte - l.start);
    final b = utf8ToUtf16(l.text, (endByte - l.start).clamp(0, l.byteLength));
    return LargeMatch(line, a, b);
  }

  /// [fromByte] 뒤(또는 [backward]면 앞)의 일치. 끝에 닿으면 처음부터. 거꾸로 찾기는 앞쪽 1000건 안에서 고른다.
  Future<LargeMatch?> find(String query, FindOptions options, int fromByte, {bool backward = false}) async {
    if (query.isEmpty) return null;
    if (backward) {
      final r = await _enqueue(() => _send('edit_find_all', {'query': query, 'limit': 1000, ...options.toJson()}));
      final all = [for (final m in (r['matches'] as List? ?? const [])) (_int((m as Map)['start']), _int(m['end']))];
      if (all.isEmpty) return null;
      final before = all.where((m) => m.$1 < fromByte);
      final m = before.isEmpty ? all.last : before.last;
      return _matchAt(m.$1, m.$2);
    }
    final r = await _enqueue(() => _send('edit_find', {'query': query, 'from': fromByte, 'wrap': true, ...options.toJson()}));
    if (r['found'] != true) return null;
    return _matchAt(_int(r['start']), _int(r['end']));
  }

  /// 일치 개수(최대 [limit] — 응답 크기 때문에 큰 파일에서는 1000까지만 센다).
  Future<int> countMatches(String query, FindOptions options, {int limit = 1000}) async {
    if (query.isEmpty) return 0;
    final r = await _enqueue(() => _send('edit_find_all', {'query': query, 'limit': limit, ...options.toJson()}));
    return _int(r['count']);
  }

  Future<int> replaceAll(String query, String replacement, FindOptions options) async {
    if (query.isEmpty) return 0;
    final r = await _enqueue(() async {
      await _send('edit_commit');
      return _send('edit_replace_all', {'query': query, 'with': replacement, ...options.toJson()});
    });
    final n = _int(r['count']);
    if (n > 0) {
      _invalidate();
      // 모두 바꾸기 응답에는 줄 수가 없다: 줄 수·글자 수를 다시 받는다.
      await _enqueue(() => _send('edit_info'));
    }
    _notify();
    return n;
  }

  // ---- 파일 ----

  @override
  Future<UnencodableReport> unencodable(String encoding) async =>
      UnencodableReport.fromJson(await _enqueue(() => _send('edit_unencodable', {'encoding': encoding, 'limit': 50})));

  @override
  Future<void> saveToFile(String selectionId, {required String encoding, required String lineEnding}) async {
    try {
      final r = await _enqueue(
        () => _send('edit_save', {'selection_id': selectionId, 'encoding': encoding, 'line_ending': lineEnding}),
      );
      if (r['name'] is String) _fileName = r['name'] as String;
    } finally {
      _notify();
    }
  }

  @override
  void dispose() {
    if (_disposed) return;
    _timer?.cancel();
    if (lostMessage == null) {
      unawaited(_enqueue(() => _backend({'cmd': 'edit_close', 'doc': doc})).then((_) {}, onError: (Object _) {}));
    }
    _disposed = true;
    super.dispose();
  }
}
