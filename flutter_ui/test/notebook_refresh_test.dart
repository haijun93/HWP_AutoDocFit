// 업무자료함 출처 상태 문구·다시 색인 단추(NBIDX1).
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:docfit_ui/features/notebook/notebook_api.dart';
import 'package:docfit_ui/features/notebook/notebook_screen.dart';

const _path = r'C:\자료\예산보고.txt';

NbSource _source(String status, {String reason = '', int count = 3}) =>
    NbSource(id: 1, displayName: 'a', kind: 'txt', status: status, paragraphCount: count, reason: reason);

void main() {
  test('source status labels cover every klib state', () {
    expect(nbSourceStatusLabel(_source('ready')), '문단 3개');
    expect(nbSourceStatusLabel(_source('indexed')), '문단 3개');
    expect(nbSourceStatusLabel(_source('stale')), '원본이 바뀜 · 다시 색인 대기 (문단 3개)');
    expect(nbSourceStatusLabel(_source('refreshing')), '다시 색인 중');
    expect(nbSourceStatusLabel(_source('missing')), '원본 파일 없음 · 문단 3개는 그대로');
    expect(nbSourceStatusLabel(_source('failed', reason: '본문 없음')), '다시 색인 실패: 본문 없음');
    expect(nbSourceStatusLabel(_source('failed')), '다시 색인 실패');
    expect(nbSourceStatusLabel(_source('pending')), '색인 준비 중');
  });

  test('refresh message names moved and unresolved citations', () {
    expect(nbRefreshMessage(const NbRefreshResult(paragraphCount: 4)), '같은 출처로 다시 색인했습니다(문단 4개).');
    expect(
      nbRefreshMessage(const NbRefreshResult(paragraphCount: 4, relinked: 2, unresolved: 1)),
      '같은 출처로 다시 색인했습니다(문단 4개, 인용 2개 위치 옮김, 인용 1개는 새 원본에서 찾지 못해 그대로).',
    );
  });

  test('bridge api sends nb_source_refresh and reads counts', () async {
    final sent = <Map>[];
    final api = BridgeNotebookApi((method, args) async {
      sent.add(args.first as Map);
      return {'ok': true, 'source_id': 7, 'paragraph_count': 5, 'relinked': 1, 'unresolved': 0};
    });
    final r = await api.refreshSource(3, 7);
    expect(sent.single, {'cmd': 'nb_source_refresh', 'notebook_id': 3, 'source_id': 7});
    expect((r.paragraphCount, r.relinked, r.unresolved), (5, 1, 0));
    final failing = BridgeNotebookApi((method, args) async => {'ok': false, 'message': '원본 파일을 찾을 수 없습니다.'});
    expect(() => failing.refreshSource(3, 7), throwsA(isA<StateError>()));
  });

  testWidgets('refresh button re-reads the source with the same id', (tester) async {
    tester.view.physicalSize = const Size(1400, 800);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    final api = InMemoryNotebookApi(fileContents: {_path: '하나\n둘\n셋'});
    final nb = await api.createNotebook('예산');
    final sid = await api.addSource(nb, _path);
    await tester.pumpWidget(MaterialApp(home: NotebookScreen(api: api)));
    await tester.pumpAndSettle();
    expect(find.byKey(ValueKey('nb-source-status-$sid')), findsOneWidget);
    expect(find.text('문단 3개'), findsOneWidget);

    api.setFile(_path, '하나\n둘\n셋\n넷');
    await tester.tap(find.byKey(ValueKey('nb-source-refresh-$sid')));
    await tester.pumpAndSettle();
    expect(find.text('문단 4개'), findsOneWidget);
    expect(find.text('같은 출처로 다시 색인했습니다(문단 4개).'), findsOneWidget);
    expect((await api.listSources(nb)).single.id, sid);
  });
}
