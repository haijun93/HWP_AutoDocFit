import 'dart:io';
import 'package:flutter_test/flutter_test.dart';
import 'package:docfit_ui/core/hierarchy_ffi.dart';

/// flutter_ui 에서 실행하므로 저장소 뿌리는 한 단계 위다.
File _repoFile(String relative) => File('${Directory.current.parent.path}/$relative');

void main() {
  test('Dart가 찾는 심볼 이름이 lib.rs 내보내기와 같다', () {
    final source = _repoFile('hierarchy/src/lib.rs').readAsStringSync();
    expect(source, contains('fn $hierarchyAnalyzeSymbol(input: *const std::ffi::c_char)'));
    expect(source, contains('fn $hierarchyFreeSymbol(value: *mut std::ffi::c_char)'));
    expect(_repoFile('hierarchy/src/ffi.rs').existsSync(), isFalse);
  });

  test('요청은 paragraphs 객체로 감싼다', () {
    expect(encodeHierarchyRequest([{'text': '가'}]), '{"paragraphs":[{"text":"가"}]}');
  });

  test('응답은 role_sequence가 있어야 받아들인다', () {
    final ok = decodeHierarchyResponse('{"role_sequence":["본문"],"document_type":"일반"}', paragraphCount: 1);
    expect(ok?['role_sequence'], ['본문']);
    expect(decodeHierarchyResponse('{"results":[{"role":"body"}]}'), isNull);
    expect(decodeHierarchyResponse('{"role_sequence":["본문"]}', paragraphCount: 2), isNull);
    expect(decodeHierarchyResponse('[1]'), isNull);
    expect(decodeHierarchyResponse('깨진 값'), isNull);
  });

  test('요약 줄은 문서 유형과 첫 역할을 보여 준다', () {
    expect(hierarchySummaryLine({'document_type': '일반', 'role_sequence': ['소제목', '본문']}), '일반 · 소제목');
    expect(hierarchySummaryLine({'role_sequence': []}), '');
    expect(hierarchySummaryLine({'results': [{'kind': 'body'}]}), '');
    expect(hierarchySummaryLine(null), '');
  });

  test('심볼이 없는 DLL은 연결되지 않는다', () {
    if (!Platform.isWindows) return;
    expect(HierarchyFfi.instance.load(r'C:\Windows\System32\kernel32.dll'), isFalse);
    expect(HierarchyFfi.instance.load(r'C:\없는폴더\hierarchy.dll'), isFalse);
  });

  test('실제 hierarchy.dll 호출이 role_sequence를 돌려준다', () {
    final dll = _repoFile('hierarchy/target/release/hierarchy.dll');
    if (!Platform.isWindows || !dll.existsSync()) {
      markTestSkipped('hierarchy.dll 없음(cargo build --release 뒤 실행)');
      return;
    }
    expect(HierarchyFfi.instance.load(dll.path), isTrue);
    expect(HierarchyFfi.instance.available, isTrue);
    final paragraphs = [
      {'text': '□ 추진 배경', 'left': 0, 'indent': 0, 'font': '', 'size_pt': 15},
      {'text': 'ㅇ 세부 내용', 'left': 0, 'indent': 0, 'font': '', 'size_pt': 15},
    ];
    // 여러 번 불러도 같은 결과(해제 뒤 재호출이 안전한지 확인).
    for (var i = 0; i < 200; i++) {
      final result = HierarchyFfi.instance.analyze(paragraphs);
      expect(result, isNotNull);
      expect((result!['role_sequence'] as List).length, 2);
      expect(hierarchySummaryLine(result), isNotEmpty);
    }
    expect(HierarchyFfi.instance.analyze([]), isNotNull);
  });
}
