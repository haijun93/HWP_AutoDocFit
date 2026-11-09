// 웹(dart2js)에는 dart:ffi가 없으므로 위계 엔진 FFI를 쓰지 않는다. 호출하면 항상 null(기존 백엔드 경로 사용).
class HierarchyFfi {
  HierarchyFfi._();
  static final HierarchyFfi instance = HierarchyFfi._();
  bool get available => false;
  Map<String, Object?>? analyze(List<Map<String, Object?>> paragraphs) => null;
}
