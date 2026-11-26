/// 문서뷰·사진뷰의 증분 목록 반영(DOCDB1).
///
/// 서비스가 보내는 항목은 추가(행), 변경(행 + `op: update`), 삭제(`op: remove`, `path`)다.
class LibraryDelta {
  LibraryDelta([List<Map<String, Object?>>? initial]) : items = initial ?? [] {
    _reindex();
  }

  final List<Map<String, Object?>> items;
  final Map<String, int> _index = {};

  void _reindex() {
    _index.clear();
    for (var i = 0; i < items.length; i++) {
      _index['${items[i]['path']}'] = i;
    }
  }

  /// 묶음 하나를 반영한다. 내용이 바뀐 경로(썸네일을 다시 받아야 함)와 없어진 경로를 돌려준다.
  ({Set<String> changed, Set<String> removed}) apply(Iterable<Object?> rows) {
    final changed = <String>{};
    final removed = <String>{};
    for (final raw in rows) {
      if (raw is! Map) continue;
      final row = Map<String, Object?>.from(raw);
      final op = row.remove('op');
      final path = '${row['path']}';
      if (op == 'remove') {
        if (_index.containsKey(path)) removed.add(path);
        continue;
      }
      final old = row.remove('old_path');
      final at = _index[path] ?? (old == null ? null : _index['$old']);
      removed.remove(path);
      if (at == null) {
        _index[path] = items.length;
        items.add(row);
        continue;
      }
      final previous = '${items[at]['path']}';
      if (previous != path) {
        _index.remove(previous);
        _index[path] = at;
        removed.add(previous);
      }
      items[at] = row;
      if (op == 'update') changed.add(path);
    }
    if (removed.isNotEmpty) {
      // 묶음마다 한 번만 걷어낸다.
      items.removeWhere((item) => removed.contains('${item['path']}'));
      _reindex();
    }
    return (changed: changed, removed: removed);
  }
}
