/// 아웃라이너 문서 단위 실행 취소/다시 실행 이력(마크다운 스냅숏 스택).
///
/// Rust 엔진(`outliner_core::UndoHistory`)과 같은 규칙·같은 한도를 쓴다.
/// Flutter 탭은 편집마다 엔진 프로세스를 새로 띄우므로 스택을 매번 엔진에 오가게 하지 않고 이 탭이 쥔다.
/// - 편집 묶음(Enter 의 저장+삽입 등)은 묶음 직전 마크다운을 [record] 로 한 번만 넣어 한 단계로 만든다.
/// - 새 편집을 기록하면 다시 실행 스택을 비운다.
/// - 단계 수 [maxDepth](200)와 글자 수 합계 [maxChars] 를 넘으면 오래된 단계부터 버린다.
/// - 페이지 전환·외부 변경(저장 충돌)이면 [clear].
class OutlinerHistory {
  OutlinerHistory({this.maxDepth = defaultDepth, this.maxChars = defaultChars});

  static const int defaultDepth = 200;
  // Rust 쪽은 바이트(32 MiB) 한도다. Dart 문자열은 UTF-16 길이로 세므로 같은 크기 근처의 보수적 값으로 둔다.
  static const int defaultChars = 16 * 1024 * 1024;

  final int maxDepth;
  final int maxChars;
  final List<String> _undo = [];
  final List<String> _redo = [];

  bool get canUndo => _undo.isNotEmpty;
  bool get canRedo => _redo.isNotEmpty;
  int get undoCount => _undo.length;
  int get redoCount => _redo.length;
  int get chars =>
      _undo.fold<int>(0, (sum, s) => sum + s.length) +
      _redo.fold<int>(0, (sum, s) => sum + s.length);

  void clear() {
    _undo.clear();
    _redo.clear();
  }

  /// 새 편집 직전 마크다운을 되돌리기 단계로 넣는다. 한도보다 큰 스냅숏이면 이력을 비우고 false.
  bool record(String before) {
    _redo.clear();
    if (before.length > maxChars) {
      _undo.clear();
      return false;
    }
    _undo.add(before);
    _trim();
    return true;
  }

  /// 한 단계 되돌린다. [current] 는 지금 마크다운(다시 실행용). 없으면 null.
  String? undo(String current) {
    if (_undo.isEmpty) return null;
    final previous = _undo.removeLast();
    _redo.add(current);
    _trim();
    return previous;
  }

  String? redo(String current) {
    if (_redo.isEmpty) return null;
    final next = _redo.removeLast();
    _undo.add(current);
    _trim();
    return next;
  }

  void _trim() {
    while (_undo.length > maxDepth) {
      _undo.removeAt(0);
    }
    while (_redo.length > maxDepth) {
      _redo.removeAt(0);
    }
    while (chars > maxChars) {
      if (_undo.isNotEmpty) {
        _undo.removeAt(0);
      } else if (_redo.isNotEmpty) {
        _redo.removeAt(0);
      } else {
        break;
      }
    }
  }
}
