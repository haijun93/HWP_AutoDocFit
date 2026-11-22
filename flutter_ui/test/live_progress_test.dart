import 'package:docfit_ui/features/progress/live_progress.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

Map<String, dynamic> _spacing({
  bool running = true,
  bool finished = false,
  String now = '본문 자간',
}) => {
  'mode': 'spacing',
  'name': '자간 정리',
  'icon': 'spacing',
  'title': finished ? '글자 간격을 다듬었어요' : '글자 간격을 다듬고 있어요',
  'finished': finished,
  'running': running,
  'message': '줄 끝에서 단어가 갈라지지 않도록 글자 간격을 조정하고 있어요.',
  'now': now,
  'steps': [
    {'key': 'open', 'name': '문서 열기', 'detail': '', 'state': 'done'},
    {
      'key': 'body_spacing',
      'name': '본문 자간',
      'detail': '줄 끝에서 단어가 갈라지지 않도록 글자 간격을 조정하고 있어요.',
      'state': finished ? 'done' : 'active',
    },
    {
      'key': 'save',
      'name': '저장·검사',
      'detail': '',
      'state': finished ? 'done' : 'waiting',
    },
  ],
  'current': 1,
  'doc': {'index': 2, 'total': 3, 'name': '보고서.hwpx'},
  'queue': [
    {'name': 'a.hwpx', 'state': 'done', 'note': 'a(자간조정).hwpx'},
    {'name': '보고서.hwpx', 'state': 'active', 'note': ''},
    {'name': 'c.hwpx', 'state': 'waiting', 'note': ''},
  ],
  'ratio': finished ? 1.0 : 0.5,
  'tips': ['첫 안내', '둘째 안내'],
  'elapsed': 75,
  'target': 'HWPX',
};

Future<void> _pump(WidgetTester tester, Map live, {double width = 1000}) async {
  tester.view.physicalSize = Size(width, 1400);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  await tester.pumpWidget(
    MaterialApp(
      home: Scaffold(
        body: SingleChildScrollView(
          child: LiveProgress(live: live, onShowLog: () {}),
        ),
      ),
    ),
  );
  await tester.pump(const Duration(milliseconds: 1200));
}

void main() {
  testWidgets('지금 단계·문서 순번·경과 시간·설명을 보여 준다', (tester) async {
    await _pump(tester, _spacing());
    expect(find.text('본문 자간'), findsWidgets); // 아일랜드와 타임라인
    expect(find.text('2/3'), findsOneWidget);
    // 엔진이 보낸 75초 + 화면이 1.2초 동안 센 1초(1초 타이머 횟수로 세어 벽시계와 무관).
    expect(find.text('1분 16초 경과'), findsOneWidget);
    expect(find.text('단계 1/3'), findsOneWidget);
    expect(find.text('50%'), findsOneWidget);
    expect(find.text('줄 끝에서 단어가 갈라지지 않도록 글자 간격을 조정하고 있어요.'), findsOneWidget);
    expect(find.text('첫 안내'), findsOneWidget);
  });

  testWidgets('안내 문구는 6초마다 바뀐다', (tester) async {
    await _pump(tester, _spacing());
    await tester.pump(const Duration(seconds: 6));
    await tester.pump(const Duration(milliseconds: 600));
    expect(find.text('둘째 안내'), findsOneWidget);
  });

  testWidgets('끝나면 완료 표시와 걸린 시간을 보여 준다', (tester) async {
    await _pump(tester, _spacing(running: false, finished: true));
    expect(find.text('완료'), findsOneWidget);
    expect(find.byIcon(Icons.check_rounded), findsWidgets);
    expect(find.text('1분 15초 걸림'), findsOneWidget);
    expect(find.text('알고 계셨나요?'), findsNothing);
  });

  testWidgets('job 진행률·현재 항목과 취소 완료 상태를 표시한다', (tester) async {
    var cancelled = false;
    await tester.pumpWidget(MaterialApp(
      home: Scaffold(body: SingleChildScrollView(child: LiveProgress(
        live: {..._spacing(), 'ratio': 0.72, 'now': '현재.hwpx'},
        onShowLog: () {},
        onCancel: () => cancelled = true,
      ))),
    ));
    await tester.pump();
    expect(find.text('72%'), findsOneWidget);
    expect(find.text('현재.hwpx'), findsWidgets);
    await tester.tap(find.text('취소'));
    expect(cancelled, isTrue);
    await tester.pumpWidget(MaterialApp(
      home: Scaffold(body: SingleChildScrollView(child: LiveProgress(
        live: {..._spacing(running: false), 'cancelled': true},
        onShowLog: () {},
      ))),
    ));
    await tester.pump();
    expect(find.text('취소됨'), findsOneWidget);
  });

  testWidgets('파일 변환은 파일 대기열과 실패 이유를 보여 준다', (tester) async {
    await _pump(tester, {
      'mode': 'file_format',
      'name': '파일 변환',
      'icon': 'convert',
      'title': '파일 3개를 PDF로 바꾸고 있어요',
      'finished': false,
      'running': true,
      'message': '회의.docx 파일을 바꾸고 있어요.',
      'now': 'PDF 변환',
      'steps': [],
      'doc': {'index': 3, 'total': 3, 'name': '회의.docx'},
      'queue': [
        {'name': '계획.hwp', 'state': 'done', 'note': '계획.pdf'},
        {'name': '스캔.png', 'state': 'failed', 'note': '글자를 읽지 못했어요'},
        {'name': '회의.docx', 'state': 'active', 'note': ''},
      ],
      'ratio': 0.6667,
      'tips': ['원본 파일은 바꾸지 않아요.'],
      'elapsed': 9,
      'target': 'PDF',
    });
    expect(find.text('PDF 변환'), findsOneWidget);
    expect(find.text('파일 2/3'), findsOneWidget);
    expect(find.text('계획.pdf'), findsOneWidget);
    expect(find.text('글자를 읽지 못했어요 · 원본은 그대로예요'), findsOneWidget);
    expect(find.byType(CircularProgressIndicator), findsOneWidget);
  });

  testWidgets('결과가 없으면 건너뜀과 실패를 구분해 요약한다', (tester) async {
    await _pump(tester, {
      'mode': 'file_format', 'name': '파일 변환', 'icon': 'convert',
      'title': '변환된 파일이 없어요 — 건너뜀 1, 실패 1', 'finished': true, 'running': false,
      'message': '', 'steps': [], 'doc': {'index': 2, 'total': 2},
      'queue': [
        {'name': '같은.pdf', 'state': 'skipped', 'note': '같은 형식'},
        {'name': '오류.txt', 'state': 'failed', 'note': '변환기 오류'},
      ],
      'conversion_summary': {'success': 0, 'failed': 1, 'skipped': 1},
      'ratio': 1.0, 'tips': [], 'elapsed': 2, 'target': 'PDF',
    });
    expect(find.text('변환된 파일이 없어요 — 건너뜀 1, 실패 1'), findsWidgets);
    expect(find.text('같은 형식 · 건너뛰었어요'), findsOneWidget);
    expect(find.text('변환기 오류 · 원본은 그대로예요'), findsOneWidget);
  });

  testWidgets('좁은 화면에서도 넘치지 않는다', (tester) async {
    await _pump(tester, _spacing(), width: 420);
    expect(tester.takeException(), isNull);
  });

  testWidgets('동작 줄이기를 켜면 반복 애니메이션 없이 그린다', (tester) async {
    tester.view.physicalSize = const Size(1000, 1400);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    await tester.pumpWidget(
      MaterialApp(
        home: MediaQuery(
          data: const MediaQueryData(disableAnimations: true),
          child: Scaffold(
            body: SingleChildScrollView(
              child: LiveProgress(live: _spacing(), onShowLog: () {}),
            ),
          ),
        ),
      ),
    );
    await tester.pump();
    expect(tester.hasRunningAnimations, isFalse);
  });

  group('남은 시간 문구', () {
    test('계산 중·점 추정·범위·끝', () {
      expect(etaLabel({'state': 'calculating'}), '남은 시간 계산 중');
      expect(
        etaLabel({'state': 'estimate', 'seconds': 100, 'low': 100, 'high': 100}),
        '남은 시간 약 2분',
      );
      expect(
        etaLabel({'state': 'estimate', 'seconds': 32, 'low': 32, 'high': 32}),
        '남은 시간 약 35초',
      );
      expect(
        etaLabel({'state': 'range', 'seconds': 200, 'low': 150, 'high': 290}),
        '남은 시간 약 3~5분',
      );
      expect(
        etaLabel({'state': 'range', 'seconds': 30, 'low': 20, 'high': 40}),
        '남은 시간 약 20~40초',
      );
      expect(
        etaLabel({'state': 'estimate', 'seconds': 1, 'low': 1, 'high': 1}),
        '곧 끝나요',
      );
      expect(etaLabel({'state': 'none'}), isNull);
      expect(etaLabel(null), isNull);
    });

    test('화면에서 흐른 초만큼 줄여 보여 준다', () {
      final eta = {'state': 'estimate', 'seconds': 100, 'low': 100, 'high': 100};
      expect(etaLabel(eta, passed: 50), '남은 시간 약 50초');
    });
  });

  testWidgets('가중 진행 막대와 남은 시간을 보여 준다', (tester) async {
    final live = _spacing()
      ..['ratio'] = 0.4
      ..['eta'] = {'state': 'range', 'seconds': 200, 'low': 150, 'high': 290};
    await _pump(tester, live);
    final bar = tester.widget<LinearProgressIndicator>(
      find.byKey(const ValueKey('weighted-bar')),
    );
    expect(bar.value, closeTo(0.4, 0.01));
    expect(find.text('남은 시간 약 3~5분'), findsOneWidget);
    // 기존 단계 이름·표시는 그대로
    expect(find.text('본문 자간'), findsWidgets);
    expect(find.text('문서 열기'), findsWidgets);
  });

  testWidgets('처음에는 계산 중, 끝나면 막대와 남은 시간이 사라진다', (tester) async {
    await _pump(
      tester,
      _spacing()..['eta'] = {'state': 'calculating'},
    );
    expect(find.text('남은 시간 계산 중'), findsOneWidget);
    await _pump(
      tester,
      _spacing(running: false, finished: true)..['eta'] = {'state': 'none'},
    );
    expect(find.byKey(const ValueKey('eta-label')), findsNothing);
    expect(find.byKey(const ValueKey('weighted-bar')), findsNothing);
  });
}
