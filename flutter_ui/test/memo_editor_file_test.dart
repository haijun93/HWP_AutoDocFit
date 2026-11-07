// 메모 편집기 3단계(EDCORE3): 파일 열기(인코딩·줄바꿈 감지 → 상태 표시줄), 큰 글 보기(보이는 줄만),
// 다른 이름으로 저장(인코딩·줄바꿈 선택, CP949 변환 불가 글자 목록).
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:docfit_ui/features/editor/editor_files.dart';
import 'package:docfit_ui/features/editor/klib_editor_core.dart';
import 'package:docfit_ui/features/editor/large_text_session.dart';
import 'package:docfit_ui/features/editor/memo_editor.dart';

import 'fake_editor_backend.dart';

KlibEditorCore _core(FakeEditorBackend fake, String text) => KlibEditorCore(fake.call, text: text, keepAlive: Duration.zero);

Future<MemoEditorController> _pump(WidgetTester tester, FakeEditorBackend fake, {String text = '', int? largeLines}) async {
  tester.view.physicalSize = const Size(1200, 800);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  final controller = MemoEditorController();
  await tester.pumpWidget(
    MaterialApp(
      home: Scaffold(
        body: MemoEditor(
          initial: MemoDocument(title: '새 메모', markdown: text),
          controller: controller,
          createCore: (t) => _core(fake, t),
          fileBackend: fake.call,
          largeFileLines: largeLines ?? kMemoLargeFileLines,
        ),
      ),
    ),
  );
  await tester.pumpAndSettle();
  return controller;
}

String _text(WidgetTester tester, String key) {
  final w = tester.widget<Text>(find.byKey(ValueKey(key)));
  return w.data ?? w.textSpan!.toPlainText();
}

Future<void> _tap(WidgetTester tester, String key) async {
  await tester.tap(find.byKey(ValueKey(key)));
  await tester.pumpAndSettle();
}

TextEditingController _body(WidgetTester tester) => tester.widget<TextField>(find.byKey(const ValueKey('memo-body'))).controller!;

void main() {
  testWidgets('opening a small CP949/CRLF file shows its real encoding and line ending; Save As changes both', (tester) async {
    final fake = FakeEditorBackend()
      ..files['s1'] = const FakeTextFile('보고서.txt', '가나\r\n다라', encoding: 'CP949')
      ..pickQueue.add('s1');
    await _pump(tester, fake);
    expect(find.byKey(const ValueKey('memo-open-file')), findsOneWidget);
    await _tap(tester, 'memo-open-file');

    expect(_body(tester).text, '가나\n다라');
    expect(_text(tester, 'memo-status-encoding'), 'CP949');
    expect(_text(tester, 'memo-status-eol'), 'Windows (CRLF)');
    expect(tester.widget<TextField>(find.byKey(const ValueKey('memo-title'))).controller!.text, '보고서');
    expect(_text(tester, 'memo-file-message'), contains('열었습니다: 보고서.txt (CP949, Windows (CRLF))'));
    // 작은 파일은 글 전체를 입력칸에 넣고, 연 문서를 그대로 이어 쓴다(다시 열지 않음).
    expect(fake.docs.length, 1); // 처음 빈 메모 문서는 닫고 연 파일 문서만 남는다
    await tester.pumpAndSettle();

    // 입력하면 연 파일 문서에 반영된다.
    await tester.enterText(find.byKey(const ValueKey('memo-body')), '가나\n다라마');
    await tester.pumpAndSettle();
    final opened = fake.docs.values.firstWhere((d) => d.name == '보고서.txt');
    expect(opened.text, '가나\n다라마');

    // 다른 이름으로 저장: 기본값은 지금 형식(CP949, CRLF). UTF-8 BOM + LF로 바꿔 저장.
    await _tap(tester, 'memo-save-file');
    expect(tester.widget<ChoiceChip>(find.byKey(const ValueKey('memo-save-enc-CP949'))).selected, isTrue);
    expect(tester.widget<ChoiceChip>(find.byKey(const ValueKey('memo-save-eol-CRLF'))).selected, isTrue);
    await _tap(tester, 'memo-save-enc-UTF-8 BOM');
    await _tap(tester, 'memo-save-eol-LF');
    fake.pickQueue.add('out1');
    await _tap(tester, 'memo-save-file-ok');
    expect(fake.saved.single.encoding, 'UTF-8 BOM');
    expect(fake.saved.single.lineEnding, 'LF');
    expect(fake.saved.single.text, '가나\n다라마');
    // 상태 표시줄 = 실제로 저장한 형식.
    expect(_text(tester, 'memo-status-encoding'), 'UTF-8 BOM');
    expect(_text(tester, 'memo-status-eol'), 'Unix (LF)');
    expect(_text(tester, 'memo-file-message'), contains('저장했습니다: out1.txt (UTF-8 BOM, Unix (LF))'));
    expect(tester.takeException(), isNull);
  });

  testWidgets('CP949 save lists unencodable characters, can go to them, and offers UTF-8 instead of replacing with ?', (tester) async {
    final fake = FakeEditorBackend();
    await _pump(tester, fake, text: '가😀\n나다😀🎉');
    await _tap(tester, 'memo-save-file');
    await _tap(tester, 'memo-save-enc-CP949');
    await _tap(tester, 'memo-save-file-ok');

    expect(find.byKey(const ValueKey('memo-unencodable')), findsOneWidget);
    expect(_text(tester, 'memo-unencodable-summary'), contains('CP949로 저장할 수 없는 글자가 3개(서로 다른 2종)'));
    expect(find.byKey(const ValueKey('memo-unencodable-char-U+1F600')), findsOneWidget);
    expect(_text(tester, 'memo-unencodable-char-U+1F600'), contains('× 2'));
    expect(find.text('2줄 3열  😀 (U+1F600)'), findsOneWidget);
    expect(fake.saved, isEmpty); // 아직 아무 파일도 쓰지 않았다

    // 돌아가기 → 다시 옵션 화면.
    await _tap(tester, 'memo-unencodable-back');
    expect(find.byKey(const ValueKey('memo-save-enc-CP949')), findsOneWidget);

    // 다시 저장 → '이동'은 그 글자를 선택한다(2줄 3열).
    await _tap(tester, 'memo-save-file-ok');
    await _tap(tester, 'memo-unencodable-goto-1');
    final sel = _body(tester).selection;
    expect(_body(tester).text.substring(sel.start, sel.end), '😀');
    expect(_text(tester, 'memo-status-position'), '줄 2, 열 3');

    // 다시 저장 → 'UTF-8로 저장'.
    await _tap(tester, 'memo-save-file');
    await _tap(tester, 'memo-save-enc-CP949');
    await _tap(tester, 'memo-save-file-ok');
    fake.pickQueue.add('utf');
    await _tap(tester, 'memo-save-as-utf8');
    expect(fake.saved.single.encoding, 'UTF-8');
    expect(fake.saved.single.text, '가😀\n나다😀🎉'); // LF로 쓴 메모는 LF로 감지돼 그대로
    expect(_text(tester, 'memo-status-encoding'), 'UTF-8');
    expect(fake.commands, contains('edit_unencodable'));
    expect(tester.takeException(), isNull);
  });

  testWidgets('cancelling the file dialog changes nothing; Dart fallback hides file buttons', (tester) async {
    final fake = FakeEditorBackend();
    await _pump(tester, fake, text: '그대로');
    await _tap(tester, 'memo-open-file');
    await _tap(tester, 'memo-open-confirm');
    expect(_body(tester).text, '그대로');
    expect(_text(tester, 'memo-file-message'), '');

    await tester.pumpWidget(const MaterialApp(home: Scaffold(body: MemoEditor())));
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('memo-open-file')), findsNothing);
    expect(find.byKey(const ValueKey('memo-save-file')), findsNothing);
  });

  testWidgets('a 100,000-line file opens in the large view: only visible lines are fetched, one line is the input', (tester) async {
    const total = 100000;
    final source = [for (var i = 0; i < total; i++) '줄 $i 내용'].join('\r\n');
    final fake = FakeEditorBackend()
      ..files['big'] = FakeTextFile('큰글.txt', source, encoding: 'CP949')
      ..pickQueue.add('big');
    final controller = await _pump(tester, fake);
    final watch = Stopwatch()..start();
    await _tap(tester, 'memo-open-file');
    watch.stop();
    // ignore: avoid_print
    print('[MEMO3] 가짜 백엔드 10만 줄 파일 열기 → 첫 화면: ${watch.elapsedMilliseconds} ms');

    expect(find.byKey(const ValueKey('memo-large-view')), findsOneWidget);
    expect(find.byKey(const ValueKey('memo-body')), findsNothing);
    expect(controller.isLargeFile, isTrue);
    expect(_text(tester, 'memo-status-mode'), '큰 파일 보기 · 100000줄');
    expect(_text(tester, 'memo-status-encoding'), 'CP949');
    expect(_text(tester, 'memo-status-eol'), 'Windows (CRLF)');
    expect(_text(tester, 'memo-large-line-0'), '줄 0 내용');
    // 글 전체를 받는 명령은 쓰지 않고, 보이는 줄 묶음(120줄)만 받았다.
    expect(fake.commands, isNot(contains('edit_text')));
    expect(fake.commands, isNot(contains('edit_lines')));
    final ranges = fake.log.where((r) => r['cmd'] == 'edit_line_range').toList();
    expect(ranges.length, lessThanOrEqualTo(2));
    expect(ranges.every((r) => r['count'] == 120), isTrue);
    // 서식 단추는 꺼진다(평문 편집).
    expect(tester.widget<IconButton>(find.byKey(const ValueKey('memo-bold'))).onPressed, isNull);

    // 끝으로 스크롤해도 그 근처 묶음만 받는다.
    final scrollable = tester.state<ScrollableState>(
      find.descendant(of: find.byKey(const ValueKey('memo-large-view')), matching: find.byType(Scrollable)),
    );
    scrollable.position.jumpTo(scrollable.position.maxScrollExtent);
    await tester.pumpAndSettle();
    expect(_text(tester, 'memo-large-line-${total - 1}'), '줄 ${total - 1} 내용');
    expect(fake.log.where((r) => r['cmd'] == 'edit_line_range').length, lessThanOrEqualTo(5));
    scrollable.position.jumpTo(0);
    await tester.pumpAndSettle();

    // 줄을 누르면 그 줄만 입력칸이 된다. 고치고 ↓로 옮기면 그 줄이 Rust에 반영된다.
    await tester.tap(find.byKey(const ValueKey('memo-large-line-3')));
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('memo-large-line-input')), findsOneWidget);
    await tester.enterText(find.byKey(const ValueKey('memo-large-line-input')), '고친 셋째 줄😀');
    await tester.pump();
    expect(_text(tester, 'memo-status-position'), '줄 4, 열 9');
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await tester.pumpAndSettle();
    final doc = fake.docs.values.firstWhere((d) => d.name == '큰글.txt');
    expect(doc.text.split('\n')[3], '고친 셋째 줄😀');
    expect(_text(tester, 'memo-large-line-3'), '고친 셋째 줄😀');
    expect(_text(tester, 'memo-status-position'), startsWith('줄 5,'));

    // Enter = 줄 나누기, 줄 맨 앞 Backspace = 앞 줄과 합치기.
    final input = tester.widget<TextField>(find.byKey(const ValueKey('memo-large-line-input'))).controller!;
    input.selection = const TextSelection.collapsed(offset: 2); // '줄 |4 내용'
    await tester.pump();
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pumpAndSettle();
    expect(doc.text.split('\n').sublist(4, 6), ['줄 ', '4 내용']);
    expect(_text(tester, 'memo-status-mode'), '큰 파일 보기 · 100001줄');
    expect(_text(tester, 'memo-large-line-6'), '줄 5 내용'); // 뒤쪽 줄 캐시도 한 칸 밀림
    expect(_text(tester, 'memo-status-position'), '줄 6, 열 1');
    await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
    await tester.pumpAndSettle();
    expect(doc.text.split('\n')[4], '줄 4 내용');
    expect(_text(tester, 'memo-status-mode'), '큰 파일 보기 · 100000줄');
    expect(_text(tester, 'memo-status-position'), '줄 5, 열 3');

    // 실행취소는 Rust 이력(합치기 취소 → 다시 나뉨).
    await _tap(tester, 'memo-undo');
    expect(doc.text.split('\n').sublist(4, 6), ['줄 ', '4 내용']);
    expect(_text(tester, 'memo-status-mode'), '큰 파일 보기 · 100001줄');

    // 찾기: 먼 줄로 이동해 그 줄을 입력칸으로 선택.
    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.keyF);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await tester.pumpAndSettle();
    await tester.enterText(find.byKey(const ValueKey('memo-find-query')), '줄 99990 ');
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pumpAndSettle();
    expect(_text(tester, 'memo-status-position'), '줄 99992, 열 9'); // 나뉜 줄 1개만큼 뒤, 커서는 일치 끝
    expect(_text(tester, 'memo-find-message'), '1개 일치');

    // 다른 이름으로 저장(CP949 그대로, 바꿀 수 없는 글자 없음 → 바로 파일 선택). 이모지를 지운 뒤.
    await _tap(tester, 'memo-undo'); // 나누기 취소
    await _tap(tester, 'memo-undo'); // 셋째 줄 고침 취소
    expect(doc.text.split('\n')[3], '줄 3 내용');
    await _tap(tester, 'memo-save-file');
    expect(tester.widget<ChoiceChip>(find.byKey(const ValueKey('memo-save-enc-CP949'))).selected, isTrue);
    fake.pickQueue.add('bigout');
    await _tap(tester, 'memo-save-file-ok');
    expect(fake.saved.single.encoding, 'CP949');
    expect(fake.saved.single.text, source);
    expect(tester.takeException(), isNull);
  });

  test('LargeTextSession halves line requests that exceed the response limit and marks a single huge line', () async {
    final fake = FakeEditorBackend()
      ..files['f'] = FakeTextFile('a.txt', ['짧은 줄', 'x' * 5000, '끝'].join('\n'))
      ..maxResponseBytes = 3000;
    final opened = await fake.call({'cmd': 'edit_open', 'selection_id': 'f'});
    final session = LargeTextSession(fake.call, doc: opened['doc']! as int, info: opened, keepAlive: Duration.zero);
    expect(session.lineAt(0), isNull); // 요청만
    await Future<void>.delayed(Duration.zero);
    for (var i = 0; i < 10; i++) {
      await Future<void>.delayed(Duration.zero);
    }
    expect(session.lineAt(0)!.text, '짧은 줄');
    expect(session.lineAt(1)!.tooLong, isTrue);
    expect(session.lineAt(2)!.text, '끝');
    await expectLater(session.replaceLine(1, 'y'), throwsA(isA<EditorBackendError>()));
    // 짧은 줄 편집은 된다.
    await session.replaceLine(2, '끝!');
    expect(fake.docs.values.single.text.endsWith('끝!'), isTrue);
    session.dispose();
  });

  test('UnencodableReport parses the core response', () {
    final r = UnencodableReport.fromJson({
      'encoding': 'CP949',
      'ok_to_save': false,
      'total': 4,
      'distinct_count': 2,
      'distinct': [
        {'char': '😀', 'code': 'U+1F600', 'count': 3},
      ],
      'first': [
        {'char': '😀', 'code': 'U+1F600', 'line': 2, 'col': 2},
      ],
    });
    expect((r.okToSave, r.total, r.distinctCount, r.distinct.single.count, r.first.single.line), (false, 4, 2, 3, 2));
    expect(memoFileFormatLabel('UTF-16 LE', 'LF'), 'UTF-16 LE, Unix (LF)');
  });
}
