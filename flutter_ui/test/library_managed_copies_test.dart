import 'package:flutter_test/flutter_test.dart';
import 'package:docfit_ui/features/library/library_api.dart';

void main() {
  const origin = 'E:/원본/자료.hwpx';
  const managed = 'E:/격리/library/1/core/자료.hwpx';
  final item = <String, Object?>{
    'id': 7, 'box_id': 1, 'kind': 'file', 'display_name': '자료.hwpx',
    'space': 'core', 'x': 12, 'y': 24, 'source_path': origin,
    'origin_path': origin, 'managed_path': managed,
  };

  test('조회·검색·복사 준비는 복사본 경로를 사용하고 원본 출처를 보존한다', () async {
    final calls = <Map>[];
    final api = BridgeLibraryApi((_, args) async {
      final request = args.single as Map;
      calls.add(request);
      return request['cmd'] == 'item_prepare'
          ? {'ok': true, ...item}
          : {'ok': true, 'items': [item]};
    });
    final listed = (await api.listItems('1')).single;
    expect(listed.source, managed);
    expect(listed.originPath, origin);
    expect(listed.copyWith(space: LibrarySpace.temp).managedPath, managed);
    expect((await api.searchContent('1', '자료')).single.path, managed);
    expect((await api.prepareItem('7')).source, managed);
    expect(calls.last, {'cmd': 'item_prepare', 'id': 7});
  });

  test('추가 후 호스트가 정한 충돌 회피 복사본 경로를 다시 읽는다', () async {
    final calls = <Map>[];
    final api = BridgeLibraryApi((_, args) async {
      final request = args.single as Map;
      calls.add(request);
      return request['cmd'] == 'item_add'
          ? {'ok': true, 'id': 7}
          : {'ok': true, ...item, 'managed_path': '$managed (1)'};
    });
    final saved = await api.addItem(boxId: '1', kind: LibraryItemKind.file,
        title: '자료.hwpx', space: LibrarySpace.core, x: 12, y: 24, source: origin);
    expect(calls.map((r) => r['cmd']), ['item_add', 'item_get']);
    expect(saved.source, '$managed (1)');
    expect(saved.originPath, origin);
  });

  test('아직 이전하지 않은 경로와 URL은 호환 조회한다', () async {
    final api = BridgeLibraryApi((_, args) async => {'ok': true, 'items': [
      {...item, 'managed_path': '', 'origin_path': ''},
      {...item, 'id': 8, 'kind': 'link', 'source_path': 'https://example.com',
        'managed_path': null, 'origin_path': null},
    ]});
    final items = await api.listItems('1');
    expect(items.first.source, origin);
    expect(items.first.managedPath, isNull);
    expect(items.last.source, 'https://example.com');
  });
}
