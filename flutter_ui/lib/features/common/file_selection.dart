import 'package:flutter/services.dart';

/// 문서뷰·사진뷰 공용 선택 상태(FILEOPS1).
///
/// `order`는 화면에 보이는 순서의 경로 목록이다. Shift 범위와 전체 선택은 이 순서를 따른다.
class FileSelection {
  final Set<String> paths = {};

  /// Shift 범위의 기준 항목(마지막으로 Shift 없이 누른 항목).
  String? anchor;

  static bool get toggleKeyPressed =>
      HardwareKeyboard.instance.isControlPressed ||
      HardwareKeyboard.instance.isMetaPressed;

  static bool get rangeKeyPressed => HardwareKeyboard.instance.isShiftPressed;

  /// [toggle]=Ctrl(추가/해제), [range]=Shift(기준부터 범위). 둘 다면 범위를 더한다.
  void click(
    String path,
    List<String> order, {
    bool toggle = false,
    bool range = false,
  }) {
    if (range) {
      final from = anchor == null ? -1 : order.indexOf(anchor!);
      final to = order.indexOf(path);
      if (from >= 0 && to >= 0) {
        if (!toggle) paths.clear();
        paths.addAll(
          order.sublist(from < to ? from : to, (from < to ? to : from) + 1),
        );
        return;
      }
    }
    if (toggle && !range) {
      if (!paths.remove(path)) paths.add(path);
    } else {
      paths
        ..clear()
        ..add(path);
    }
    anchor = path;
  }

  void selectAll(List<String> order) {
    paths
      ..clear()
      ..addAll(order.where((p) => p.isNotEmpty));
  }

  void clear() {
    paths.clear();
    anchor = null;
  }

  /// 선택한 것 가운데 지금 화면에 있는 항목(화면 순서). 파일 작업은 이것만 대상으로 한다.
  List<String> visible(List<String> order) =>
      {for (final p in order) if (paths.contains(p)) p}.toList();

  /// 파일 작업 결과를 반영한다: 지운 것은 빼고, 옮기거나 이름이 바뀐 것은 새 경로로 바꾼다.
  void applyResults(String op, List<Map<String, Object?>> results) {
    if (op != 'delete' && op != 'move' && op != 'rename') return;
    for (final result in results) {
      if (result['ok'] != true) continue;
      final path = '${result['path']}';
      final newPath = result['listed'] == true ? result['new_path'] : null;
      if (paths.remove(path) && newPath is String) paths.add(newPath);
      if (anchor == path) anchor = newPath is String ? newPath : null;
    }
  }
}

/// 파일 작업 결과로 화면 목록을 고친다. 실패한 파일은 그대로 둔다.
///
/// [relocate]는 항목을 새 경로·이름으로 바꾼 사본을 만든다.
List<Map<String, Object?>> applyFileResults(
  List<Map<String, Object?>> items,
  String op,
  List<Map<String, Object?>> results,
  Map<String, Object?> Function(Map<String, Object?> item, String path, String name)
  relocate,
) {
  final done = {
    for (final result in results)
      if (result['ok'] == true) '${result['path']}': result,
  };
  final next = <Map<String, Object?>>[];
  for (final item in items) {
    final result = done['${item['path']}'];
    if (result == null || op == 'copy' || op == 'clipboard') next.add(item);
    if (result == null || result['listed'] != true) continue;
    final path = result['new_path'];
    if (path is String) {
      next.add(relocate(item, path, '${result['new_name']}'));
    }
  }
  return next;
}
