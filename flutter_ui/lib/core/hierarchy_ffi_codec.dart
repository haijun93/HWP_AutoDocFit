// 위계 엔진 FFI 의 순수 Dart 부분(웹에서도 쓸 수 있게 dart:ffi 를 쓰지 않는다).
import 'dart:convert';

// hierarchy/src/lib.rs 의 C 규약과 같아야 한다(Python docfit_app/hierarchy_bridge.py 와 동일).
//   char* hierarchy_analyze_json(const char* utf8_json_nul_terminated);
//   void  hierarchy_json_free(char* value);
const String hierarchyAnalyzeSymbol = 'hierarchy_analyze_json';
const String hierarchyFreeSymbol = 'hierarchy_json_free';

/// 엔진에 넘길 요청 JSON. `lib.rs`는 `{"paragraphs": [...]}`를 읽는다.
String encodeHierarchyRequest(List<Map<String, Object?>> paragraphs) =>
    jsonEncode({'paragraphs': paragraphs});

/// 엔진 응답 해석. `lib.rs`는 `role_sequence`(문단별 역할 이름 목록)와
/// `document_type` 등을 담은 객체를 돌려준다. 모양이 다르면 null.
Map<String, Object?>? decodeHierarchyResponse(String text, {int? paragraphCount}) {
  final Object? decoded;
  try {
    decoded = jsonDecode(text);
  } on FormatException {
    return null;
  }
  if (decoded is! Map<String, Object?>) return null;
  final roles = decoded['role_sequence'];
  if (roles is! List) return null;
  if (paragraphCount != null && roles.length != paragraphCount) return null;
  return decoded;
}

/// 화면에 보일 한 줄 요약(문서 유형 · 첫 문단 역할). 값이 없으면 빈 문자열.
String hierarchySummaryLine(Object? response) {
  if (response is! Map) return '';
  final roles = response['role_sequence'];
  final parts = <String>[
    if (response['document_type'] is String) response['document_type'] as String,
    if (roles is List && roles.isNotEmpty && roles.first is String) roles.first as String,
  ].where((part) => part.isNotEmpty);
  return parts.join(' · ');
}
