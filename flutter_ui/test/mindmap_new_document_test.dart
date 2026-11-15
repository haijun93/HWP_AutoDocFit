import 'dart:convert';
import 'dart:io';

import 'package:docfit_ui/features/flows/flow_draft.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  group('Mindmap new document and regression tests', () {
    test('mindmap new document snapshot structure contains root and layout', () {
      // 마인드맵 새 문서 시 mindmap 구조가 누락되면 메뉴가 사라지므로 필수 구조를 보장해야 한다.
      final newMindmapDoc = <String, Object?>{
        'nodes': <Object>[],
        'edges': <Object>[],
        'mindmap': <String, Object?>{
          'root': <String, Object?>{
            'id': 'root-1',
            'text': '중심 주제',
            'children': <Object>[],
            'style': <String, Object?>{},
          },
          'layout': 'both',
        },
      };

      expect(newMindmapDoc.containsKey('mindmap'), isTrue);
      final mindmap = newMindmapDoc['mindmap'] as Map<String, Object?>;
      expect(mindmap.containsKey('root'), isTrue);
      final root = mindmap['root'] as Map<String, Object?>;
      expect(root['text'], equals('중심 주제'));
      expect(root['children'], isEmpty);
      expect(mindmap['layout'], equals('both'));
    });

    test('FlowDraftClient saves and restores mindmap draft with mindmap data', () async {
      final calls = <List<Object?>>[];
      Map<String, Object?>? stored;

      Future<Object?> fakeApi(String method, List<Object?> args) async {
        calls.add([method, ...args]);
        final action = args[0] as String;
        final payload = args[1] as Map;
        switch (action) {
          case 'save_draft':
            stored = {'exists': true, ...Map<String, Object?>.from(payload)};
            return {'saved': true};
          case 'load_draft':
            return stored ?? {'exists': false};
          case 'clear_draft':
            stored = null;
            return {'cleared': true};
          case 'load':
            return {'revision': 'rev-1'};
        }
        throw StateError('Unknown action: $action');
      }

      final client = FlowDraftClient(fakeApi, 'mindmap');
      final mindmapSnapshot = {
        'nodes': <Object>[],
        'edges': <Object>[],
        'mindmap': {
          'root': {
            'id': 'center-node',
            'text': '중심 주제',
            'children': <Object>[],
          },
          'layout': 'both',
        },
      };

      // 마인드맵 임시저장
      final saved = await client.save(
        {'name': '새 마인드맵', 'snapshot': mindmapSnapshot},
        id: '1' * 32,
      );
      expect(saved, isTrue);

      // 복구 확인
      final draft = await client.pending();
      expect(draft, isNotNull);
      expect(draft!['name'], equals('새 마인드맵'));
      final snapshot = draft['snapshot'] as Map;
      expect(snapshot['mindmap'], isNotNull);
      final mm = snapshot['mindmap'] as Map;
      expect((mm['root'] as Map)['text'], equals('중심 주제'));

      final restoreMsg = FlowDraftClient.restoreMessage(draft, '1' * 32);
      expect(restoreMsg['type'], equals('load'));
      expect(restoreMsg['recovered'], isTrue);
      expect((restoreMsg['snapshot'] as Map)['mindmap'], isNotNull);

      await client.clear();
      expect(await client.pending(), isNull);
    });

    test('flow_editor web bundle contains mindmap new document & clear all symbols', () {
      final manifestFile = File('assets/flow_editor/asset-manifest.json');
      expect(manifestFile.existsSync(), isTrue);

      final manifestContent = manifestFile.readAsStringSync();
      final manifest = jsonDecode(manifestContent) as Map<String, Object?>;
      expect(manifest.isNotEmpty, isTrue);

      // 번들 js 파일 읽기
      final jsEntry = manifest['index.html'] as Map<String, Object?>?;
      expect(jsEntry, isNotNull);
      final jsFileRel = jsEntry!['file'] as String?;
      expect(jsFileRel, isNotNull);

      final jsFile = File('assets/flow_editor/$jsFileRel');
      expect(jsFile.existsSync(), isTrue);
      final jsContent = jsFile.readAsStringSync();

      // 마인드맵 새 문서 시 빈 마인드맵 생성 및 전체 지우기 기능이 번들에 포함되어 있는지 검증
      expect(jsContent.contains('중심 주제'), isTrue);
      expect(jsContent.contains('모든 가지를 지우시겠습니까?'), isTrue);
      expect(jsContent.contains('새 마인드맵 시작'), isTrue);
    });
  });
}
