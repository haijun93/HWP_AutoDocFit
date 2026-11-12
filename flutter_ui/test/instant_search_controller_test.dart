import 'dart:async';

import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:docfit_ui/core/instant_search.dart';

/// 요청을 손으로 끝내는 가짜 검색기.
class _Backend {
  final List<InstantSearchRequest> sent = [];
  final List<Completer<String>> _waits = [];
  final List<(InstantSearchRequest, String)> applied = [];
  final List<(String, String)> cleared = [];
  final List<InstantSearchRequest> failed = [];

  Future<String> search(InstantSearchRequest request) {
    sent.add(request);
    final wait = Completer<String>();
    _waits.add(wait);
    return wait.future;
  }

  void finish(int index, [String? value]) => _waits[index].complete(value ?? '${sent[index].tab}:${sent[index].query}');
  void fail(int index) => _waits[index].completeError(StateError('실패'));

  InstantSearchController<String> controller({String tab = 'file'}) => InstantSearchController<String>(
        initialTab: tab,
        search: search,
        onResult: (r, v) => applied.add((r, v)),
        onCleared: (t, q) => cleared.add((t, q)),
        onError: (r, _) => failed.add(r),
      );
}

void main() {
  testWidgets('2글자 미만은 찾지 않고 비우며, 2글자부터 지연 뒤 찾는다', (tester) async {
    final b = _Backend();
    final c = b.controller();
    c.text.text = '회';
    await tester.pump(const Duration(milliseconds: 500));
    expect(b.sent, isEmpty);
    expect(c.phase, InstantSearchPhase.empty);
    expect(b.cleared.last, ('file', '회'));

    c.text.text = '회의';
    expect(c.phase, InstantSearchPhase.typing);
    await tester.pump(const Duration(milliseconds: 149));
    expect(b.sent, isEmpty);
    await tester.pump(const Duration(milliseconds: 1));
    expect(b.sent.single.query, '회의');
    expect(c.phase, InstantSearchPhase.searching);
    b.finish(0);
    await tester.pump();
    expect(c.phase, InstantSearchPhase.results);
    expect(b.applied.single.$2, 'file:회의');

    c.text.text = '회';
    expect(c.phase, InstantSearchPhase.empty);
    c.dispose();
  });

  testWidgets('연속 입력은 한 번으로 합쳐지고 공백만 바뀌면 다시 찾지 않는다', (tester) async {
    final b = _Backend();
    final c = b.controller();
    for (final t in ['예산', '예산 집', '예산 집행']) {
      c.text.text = t;
      await tester.pump(const Duration(milliseconds: 80));
    }
    await tester.pump(const Duration(milliseconds: 200));
    expect(b.sent.map((r) => r.query), ['예산 집행']);
    b.finish(0);
    await tester.pump();
    c.text.text = '예산 집행 ';
    await tester.pump(const Duration(milliseconds: 200));
    expect(b.sent.length, 1);
    c.dispose();
  });

  testWidgets('늦게 온 앞 응답은 버리고, 진행 중에는 마지막 입력 1건만 대기시킨다', (tester) async {
    final b = _Backend();
    final c = b.controller(tab: 'content');
    c.text.text = '회의';
    await tester.pump(const Duration(milliseconds: 150));
    expect(b.sent.length, 1);
    c.text.text = '회의록';
    await tester.pump(const Duration(milliseconds: 150));
    c.text.text = '회의록 정리';
    await tester.pump(const Duration(milliseconds: 150));
    // 백엔드가 요청을 차례로 처리하므로 앞 요청이 끝날 때까지 새 요청을 보내지 않는다.
    expect(b.sent.length, 1);
    expect(c.phase, InstantSearchPhase.searching);

    b.finish(0);
    await tester.pump();
    expect(b.applied, isEmpty, reason: '옛 검색어 응답은 화면에 반영하지 않는다');
    expect(b.sent.map((r) => r.query), ['회의', '회의록 정리'], reason: '중간 입력(회의록)은 건너뛴다');
    b.finish(1);
    await tester.pump();
    expect(b.applied.single.$1.query, '회의록 정리');
    expect(c.phase, InstantSearchPhase.results);
    c.dispose();
  });

  testWidgets('응답 전에 검색어를 지우면 그 응답과 오류를 모두 버린다', (tester) async {
    final b = _Backend();
    final c = b.controller();
    c.text.text = '회의';
    await tester.pump(const Duration(milliseconds: 150));
    c.text.text = '';
    b.fail(0);
    await tester.pump();
    expect(b.failed, isEmpty);
    expect(c.phase, InstantSearchPhase.empty);
    c.dispose();
  });

  testWidgets('현재 요청의 오류는 error 상태가 된다', (tester) async {
    final b = _Backend();
    final c = b.controller();
    c.text.text = '회의';
    await tester.pump(const Duration(milliseconds: 150));
    b.fail(0);
    await tester.pump();
    expect(c.phase, InstantSearchPhase.error);
    expect(b.failed.single.query, '회의');
    c.dispose();
  });

  testWidgets('탭을 바꾸면 키워드를 유지한 채 새 탭에서 지연 없이 다시 찾는다', (tester) async {
    final b = _Backend();
    final c = b.controller();
    c.text.text = '사업';
    await tester.pump(const Duration(milliseconds: 150));
    b.finish(0);
    await tester.pump();

    c.selectTab('content');
    expect(c.text.text, '사업');
    expect(b.sent.last.tab, 'content');
    expect(b.sent.last.trigger, 'tab');
    b.finish(1);
    await tester.pump();
    expect(b.applied.last.$2, 'content:사업');

    c.selectTab('content'); // 같은 탭은 다시 찾지 않는다.
    expect(b.sent.length, 2);

    c.text.text = '사';
    c.selectTab('related'); // 기준 미만이면 새 탭 결과를 비운다.
    expect(b.sent.length, 2);
    expect(b.cleared.last, ('related', '사'));
    c.dispose();
  });

  testWidgets('Enter는 지연 없이 1글자도 찾고, 그 검색어는 탭을 바꿔도 다시 찾는다', (tester) async {
    final b = _Backend();
    final c = b.controller();
    c.text.text = '절';
    c.submit();
    expect(b.sent.single.query, '절');
    expect(b.sent.single.trigger, 'submit');
    b.finish(0);
    await tester.pump();
    c.selectTab('content');
    expect(b.sent.last.tab, 'content');
    expect(b.sent.last.query, '절');

    c.text.text = '';
    c.submit();
    expect(b.sent.length, 2);
    expect(c.phase, InstantSearchPhase.empty);
    c.dispose();
  });

  testWidgets('한글 조합 중 낱자모는 검색어에서 빼고 완성 음절은 넣는다', (tester) async {
    final b = _Backend();
    final c = b.controller();
    // "회" 확정 + "ㅇ" 조합 중 → 유효 검색어 "회"(1글자) → 찾지 않는다.
    c.text.value = const TextEditingValue(text: '회ㅇ', selection: TextSelection.collapsed(offset: 2), composing: TextRange(start: 1, end: 2));
    await tester.pump(const Duration(milliseconds: 300));
    expect(c.query, '회');
    expect(b.sent, isEmpty);
    // "의"가 조합 중(완성 음절) → "회의"로 찾는다.
    c.text.value = const TextEditingValue(text: '회의', selection: TextSelection.collapsed(offset: 2), composing: TextRange(start: 1, end: 2));
    await tester.pump(const Duration(milliseconds: 150));
    expect(b.sent.single.query, '회의');
    // 조합만 확정되고 글자가 같으면 다시 찾지 않는다.
    c.text.value = const TextEditingValue(text: '회의', selection: TextSelection.collapsed(offset: 2));
    await tester.pump(const Duration(milliseconds: 300));
    expect(b.sent.length, 1);
    c.dispose();
  });

  testWidgets('refresh는 조건을 그 요청에만 붙이고 기준 미만이면 찾지 않는다', (tester) async {
    final b = _Backend();
    final c = b.controller(tab: 'content');
    expect(c.refresh(options: 'recent'), isFalse);
    c.text.text = '예산';
    await tester.pump(const Duration(milliseconds: 150));
    b.finish(0);
    await tester.pump();
    expect(c.refresh(options: 'recent'), isTrue);
    expect(b.sent.last.options, 'recent');
    expect(b.sent.first.options, isNull);
    c.dispose();
  });

  testWidgets('reset은 콜백 없이 검색어·탭을 비우고 진행 중 응답을 버린다', (tester) async {
    final b = _Backend();
    final c = b.controller(tab: 'content');
    c.text.text = '예산';
    await tester.pump(const Duration(milliseconds: 150));
    c.reset(tab: 'file');
    b.finish(0);
    await tester.pump();
    expect(c.text.text, '');
    expect(c.tab, 'file');
    expect(c.phase, InstantSearchPhase.empty);
    expect(b.applied, isEmpty);
    expect(b.cleared, isEmpty);
    expect(c.describe(), contains('phase=empty'));
    c.dispose();
  });
}
