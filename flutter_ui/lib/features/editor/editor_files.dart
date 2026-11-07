/// 메모 편집기 3단계(EDCORE3): 텍스트 파일 열기·인코딩/줄바꿈 선택 저장·변환 불가 글자 목록.
///
/// 경로는 화면이 모른다(docs/EDITOR_CORE_DESIGN.md 12절): 호스트(Python)가 파일 대화상자를 띄우고
/// 선택 번호(selection_id)만 돌려준다. `edit_open {selection_id}`·`edit_save {selection_id, encoding, line_ending}`에서
/// 호스트가 경로로 바꿔 klib에 넘기며, 응답에는 파일 이름(`name`)만 실린다.
library;

import 'klib_editor_core.dart' show EditorBackend, EditorBackendError;

/// 저장 대화상자에서 고르는 인코딩(메모장과 같은 4가지)과 줄바꿈.
const kMemoEncodings = ['UTF-8', 'UTF-8 BOM', 'UTF-16 LE', 'CP949'];
const kMemoLineEndings = ['CRLF', 'LF'];

int _int(Object? v) => v is num ? v.toInt() : int.tryParse('$v') ?? 0;

/// 고른 인코딩으로 바꿀 수 없는 글자 한 종류(많은 순).
class UnencodableChar {
  const UnencodableChar({required this.char, required this.code, required this.count});
  final String char;
  final String code;
  final int count;
}

/// 바꿀 수 없는 글자가 처음 나오는 자리들(줄·열은 1부터).
class UnencodablePlace {
  const UnencodablePlace({required this.char, required this.code, required this.line, required this.column});
  final String char;
  final String code;
  final int line;
  final int column;
}

/// `edit_unencodable` 결과.
class UnencodableReport {
  const UnencodableReport({
    required this.encoding,
    required this.okToSave,
    required this.total,
    required this.distinctCount,
    this.distinct = const [],
    this.first = const [],
  });

  factory UnencodableReport.fromJson(Map<String, Object?> r) => UnencodableReport(
    encoding: '${r['encoding'] ?? ''}',
    okToSave: r['ok_to_save'] == true,
    total: _int(r['total']),
    distinctCount: _int(r['distinct_count']),
    distinct: [
      for (final d in (r['distinct'] as List? ?? const []))
        if (d is Map) UnencodableChar(char: '${d['char'] ?? ''}', code: '${d['code'] ?? ''}', count: _int(d['count'])),
    ],
    first: [
      for (final d in (r['first'] as List? ?? const []))
        if (d is Map) UnencodablePlace(char: '${d['char'] ?? ''}', code: '${d['code'] ?? ''}', line: _int(d['line']), column: _int(d['col'])),
    ],
  );

  final String encoding;
  final bool okToSave;
  final int total;
  final int distinctCount;
  final List<UnencodableChar> distinct;
  final List<UnencodablePlace> first;
}

/// 파일로 저장할 수 있는 편집 문서(Rust 코어에 열린 문서). [KlibEditorCore]·[LargeTextSession]이 구현한다.
abstract interface class EditorFileSupport {
  /// 지금 저장 설정(상태 표시줄과 저장 대화상자 기본값).
  String get encoding;
  String get lineEnding;

  /// 연 파일 이름(새 메모면 null).
  String? get fileName;

  /// 파일로 저장할 수 있는 상태인지(Rust 코어 연결 등).
  bool get canSaveToFile;

  Future<UnencodableReport> unencodable(String encoding);

  /// 고른 인코딩·줄바꿈으로 저장한다. 성공하면 [encoding]·[lineEnding]·[fileName]이 저장한 값으로 바뀐다.
  /// 실패하면(변환 불가 글자 등) [EditorBackendError]를 던지고 파일·설정은 그대로다.
  Future<void> saveToFile(String selectionId, {required String encoding, required String lineEnding});
}

/// 호스트 파일 대화상자와 `edit_open {selection_id}`.
class MemoFileGateway {
  const MemoFileGateway(this.backend);
  final EditorBackend backend;

  Future<Map<String, Object?>> _call(Map<String, Object?> request) async {
    final Map<String, Object?> r;
    try {
      r = await backend(request);
    } catch (e) {
      throw EditorBackendError('$e');
    }
    if (r['ok'] != true) throw EditorBackendError('${r['message'] ?? r['error'] ?? '파일 요청이 실패했습니다.'}');
    return r;
  }

  /// 파일 대화상자. 고르면 선택 번호, 취소하면 null.
  Future<String?> pick({required bool save}) async {
    final r = await _call({'cmd': 'edit_select_file', 'mode': save ? 'save' : 'open'});
    if (r['cancelled'] == true) return null;
    final id = r['selection_id'];
    if (id is! String || id.isEmpty) throw EditorBackendError('파일 선택을 다시 확인해 주세요.');
    return id;
  }

  /// 고른 파일을 Rust 코어에 연다(인코딩·줄바꿈 자동 감지). 응답: `doc`, `lines`, `len`, `chars`, `encoding`, `line_ending`, `name`.
  Future<Map<String, Object?>> open(String selectionId) => _call({'cmd': 'edit_open', 'selection_id': selectionId});

  /// 열린 문서를 닫는다(작은 파일을 일반 편집으로 옮길 때 실패 정리용).
  Future<void> close(int doc) async {
    try {
      await backend({'cmd': 'edit_close', 'doc': doc});
    } catch (_) {}
  }
}

/// 짧은 저장 설정 표시. 예: `UTF-8, Windows (CRLF)`.
String memoFileFormatLabel(String encoding, String lineEnding) =>
    '$encoding, ${switch (lineEnding) {
      'LF' => 'Unix (LF)',
      'CR' => 'Macintosh (CR)',
      _ => 'Windows (CRLF)',
    }}';
