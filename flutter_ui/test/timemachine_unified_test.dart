import 'dart:io';
import 'dart:ui' as ui;

import 'package:docfit_ui/core/theme/app_theme.dart';
import 'package:docfit_ui/features/timemachine/timemachine_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

/// 통합 보기 시험. 모든 경로·내용은 합성 자료다.
/// 환경 변수 UNIFIED_CAPTURE_DIR 이 있으면 라이트·다크 × 폭 700·1100 PNG를 만든다.
const _toggle = ValueKey('tm-unified-toggle');
const _search = ValueKey('tm-file-search');

Map<String, Object?> _row(String name, String tier, {String snippet = ''}) => {
      'path': 'Z:\\합성\\$name',
      'name': name,
      'parent': r'Z:\합성',
      'tier': tier,
      'location': tier == '파일명' ? '12KB · 2026-03-02' : '문단 3',
      'snippet': snippet,
    };

Map<String, Object?> _page(
  List<Map<String, Object?>> rows, {
  int cursor = 0,
  int? next,
  List<int> totals = const [1, 1, 1],
  List<String> errors = const [],
  String? title,
  String detail = '',
  String? summary,
}) =>
    {
      'event': 'page',
      'generation': 1,
      'cursor': cursor,
      'next_cursor': next,
      'totals': totals,
      'errors': errors,
      'answer': {'rows': rows, 'terms': ['예산'], 'groups': ['예산']},
      'notice': title == null ? null : {'title': title, 'detail': detail},
      'summary': summary ?? '${totals.fold<int>(0, (a, b) => a + b)}개',
    };

final _first = [
  _row('예산 집행 계획.hwpx', '파일명'),
  _row('상반기 업무 보고.hwpx', '위계', snippet: '1. 예산 집행 현황과 하반기 추진 방향'),
  _row('회의 결과.txt', '본문', snippet: '다음 분기 예산 조정은 담당 부서와 협의한다.'),
];

class _Api {
  _Api({this.unified = false});
  bool unified;
  final List<(String, Map<String, Object?>)> calls = [];
  /// unified_poll 이 차례로 돌려줄 응답. 마지막 것은 계속 돌려준다.
  List<Map<String, Object?>?> polls = [_page(_first)];
  Map<int, Map<String, Object?>> pages = {};
  Map<String, Object?> startResponse = {'ok': true, 'generation': 1};
  Map<String, Object?> openResponse = {'ok': true};

  Iterable<Map<String, Object?>> of(String action) => calls.where((c) => c.$1 == action).map((c) => c.$2);

  Future<Object?> call(String method, List<Object?> args) async {
    if (method == 'get_settings') return {'developer_mode': false};
    if (method != 'timemachine') return {'ok': true};
    final action = args[0] as String;
    final payload = Map<String, Object?>.from(args.length > 1 ? args[1] as Map : {});
    calls.add((action, payload));
    switch (action) {
      case 'status':
        return {'ok': true, 'watch': 'app', 'scan': {'running': false}};
      case 'scan':
        return {'ok': true, 'by': 'app'};
      case 'settings_get':
        return {
          'ok': true,
          'settings': {'enabled': true, 'roots': [r'Z:\합성'], 'time_mode': 'max', 'unified_view': unified},
        };
      case 'content_index_status':
        return {'ok': true, 'running': false};
      case 'day_counts':
        return {'ok': true, 'days': {}};
      case 'hour_density':
        return {'ok': true, 'hours': {}};
      case 'range':
        return {'ok': true, 'items': []};
      case 'file_search':
        return {'ok': true, 'items': []};
      case 'unified_view_set':
        unified = payload['enabled'] == true;
        return {'ok': true, 'unified_view': unified};
      case 'unified_start':
        return startResponse;
      case 'unified_poll':
        return {'ok': true, 'result': polls.length > 1 ? polls.removeAt(0) : polls.single};
      case 'unified_page':
        return {'ok': true, 'result': pages[payload['cursor']]};
      case 'open':
      case 'reveal':
        return openResponse;
      case 'preview':
        return {'ok': false};
      default:
        return {'ok': true};
    }
  }
}

Future<void> _pumpScreen(WidgetTester tester, _Api api, {double width = 1400, ThemeData? theme}) async {
  await tester.binding.setSurfaceSize(Size(width, 900));
  addTearDown(() => tester.binding.setSurfaceSize(null));
  await tester.pumpWidget(MaterialApp(
    debugShowCheckedModeBanner: false,
    theme: theme,
    home: RepaintBoundary(
      key: const ValueKey('capture'),
      child: Scaffold(body: TimemachineScreen(api: api.call)),
    ),
  ));
  await tester.pumpAndSettle();
}

Future<void> _type(WidgetTester tester, String text) async {
  await tester.enterText(find.byKey(_search), text);
  await tester.pump(const Duration(milliseconds: 200));
  await _settle(tester);
}

/// 진행 표시가 도는 동안에도 끝나는 고정 시간 진행.
Future<void> _settle(WidgetTester tester) async {
  for (var i = 0; i < 6; i++) {
    await tester.pump(const Duration(milliseconds: 40));
  }
}

String _text(WidgetTester tester, String key) => tester.widget<Text>(find.byKey(ValueKey(key))).data!;

void main() {
  testWidgets('통합 보기는 기본 끔이고, 켜면 저장하고 끄면 3탭으로 돌아온다', (tester) async {
    final api = _Api();
    await _pumpScreen(tester, api);
    expect(find.byKey(const ValueKey('tm-search-mode')), findsOneWidget);
    await _type(tester, '예산');
    expect(api.of('unified_start'), isEmpty);

    await tester.tap(find.byKey(_toggle));
    await _settle(tester);
    expect(api.of('unified_view_set').last, {'enabled': true});
    expect(find.byKey(const ValueKey('tm-search-mode')), findsNothing);
    // 친 검색어로 바로 찾는다.
    expect(api.of('unified_start').single['query'], '예산');
    expect(find.byKey(const ValueKey('tm-unified-totals')), findsOneWidget);

    await tester.tap(find.byKey(_toggle));
    await _settle(tester);
    expect(api.of('unified_view_set').last, {'enabled': false});
    expect(api.of('unified_close'), hasLength(1));
    expect(find.byKey(const ValueKey('tm-search-mode')), findsOneWidget);
    expect(find.byKey(const ValueKey('tm-unified-totals')), findsNothing);
  });

  testWidgets('저장한 통합 보기는 부분 결과를 먼저 보이고 단 순서로 표시한다', (tester) async {
    final api = _Api(unified: true)
      ..polls = [
        null,
        {
          'event': 'part',
          'generation': 1,
          'errors': <String>[],
          'answer': {'rows': [_first.first], 'terms': ['예산']},
          'notice': null,
          'summary': '1개',
        },
        null,
      ];
    await _pumpScreen(tester, api);
    await _type(tester, '예산');
    expect(find.text('파일명 일치'), findsOneWidget);
    expect(find.byKey(const ValueKey('tm-unified-cancel')), findsOneWidget);
    expect(find.byKey(const ValueKey('tm-unified-totals')), findsNothing);

    api.polls = [_page(_first)];
    await _settle(tester);
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('tm-unified-cancel')), findsNothing);
    expect(_text(tester, 'tm-unified-totals'), '파일명 1 · 위계 1 · 본문 1');
    final tops = [for (final tier in ['파일명', '위계', '본문']) tester.getTopLeft(find.text('$tier 일치')).dy];
    expect(tops, [...tops]..sort());
    // 조회가 끝나면 더 묻지 않는다.
    final polled = api.of('unified_poll').length;
    await tester.pump(const Duration(seconds: 1));
    expect(api.of('unified_poll').length, polled);
  });

  testWidgets('이어보기·이전 보기와 단축키는 같은 세션의 커서를 쓴다', (tester) async {
    final api = _Api(unified: true)
      ..polls = [_page(_first, next: 1, totals: [1, 1, 2])]
      ..pages = {
        0: _page(_first, next: 1, totals: [1, 1, 2]),
        1: _page([_row('마지막 문서.txt', '본문')], cursor: 1, totals: [1, 1, 2]),
      };
    await _pumpScreen(tester, api);
    await _type(tester, '예산');
    await tester.tap(find.byKey(const ValueKey('tm-unified-next')));
    await tester.pumpAndSettle();
    expect(find.textContaining('마지막 문서'), findsOneWidget);
    expect(find.text('2쪽'), findsOneWidget);
    expect(api.of('unified_page').last, {'generation': 1, 'cursor': 1});
    await tester.tap(find.byKey(const ValueKey('tm-unified-previous')));
    await tester.pumpAndSettle();
    expect(api.of('unified_page').last, {'generation': 1, 'cursor': 0});
    expect(find.text('1쪽'), findsOneWidget);

    // 목록에 초점이 있으면 Ctrl+PgDn/PgUp 도 같다.
    await tester.tap(find.byKey(ValueKey('tm-unified-row-${_first.last['path']}')));
    await tester.pump(const Duration(milliseconds: 400));
    await tester.pumpAndSettle();
    Focus.of(tester.element(find.byKey(const ValueKey('tm-unified-summary')))).requestFocus();
    await tester.pump();
    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.pageDown);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await tester.pumpAndSettle();
    expect(find.text('2쪽'), findsOneWidget);
    expect(api.of('unified_start'), hasLength(1));
  });

  testWidgets('문서 열기·폴더 열기·미리보기는 경로로 동작하고 실패 이유를 보인다', (tester) async {
    final api = _Api(unified: true);
    await _pumpScreen(tester, api);
    await _type(tester, '예산');
    final path = _first.first['path']! as String;
    await tester.tap(find.byKey(ValueKey('tm-unified-open-$path')));
    await tester.tap(find.byKey(ValueKey('tm-unified-reveal-$path')));
    await tester.pumpAndSettle();
    expect(api.of('open').single, {'path': path});
    expect(api.of('reveal').single, {'path': path});

    final row = find.byKey(ValueKey('tm-unified-row-$path'));
    await tester.tap(row);
    await tester.pump(const Duration(milliseconds: 50));
    await tester.tap(row);
    await tester.pumpAndSettle();
    expect(api.of('open'), hasLength(2));

    await tester.tap(row);
    await tester.pump(const Duration(milliseconds: 400));
    await tester.pumpAndSettle();
    expect(find.text('문서 열기'), findsOneWidget);

    api.openResponse = {'ok': false, 'message': '현재 조사 폴더 안의 문서만 열 수 있습니다.'};
    await tester.tap(find.byKey(ValueKey('tm-unified-reveal-$path')));
    await tester.pumpAndSettle();
    expect(find.text('현재 조사 폴더 안의 문서만 열 수 있습니다.'), findsOneWidget);
  });

  for (final (title, summary) in [
    ('검색 결과가 없습니다', '0개'),
    ('검색을 완료하지 못했습니다', '검색 못함'),
    ('파일명 색인을 준비하는 중입니다', '검색 못함'),
    ('파일명을 검색하지 못했습니다', '검색 못함'),
  ]) {
    testWidgets('호스트 안내를 그대로 보인다: $title', (tester) async {
      final api = _Api(unified: true)
        ..polls = [_page([], totals: [0, 0, 0], title: title, detail: '합성 사유', summary: summary)];
      await _pumpScreen(tester, api);
      await _type(tester, '예산');
      expect(_text(tester, 'tm-unified-notice'), title);
      expect(_text(tester, 'tm-unified-summary'), summary);
      expect(find.text('합성 사유'), findsOneWidget);
      expect(find.byKey(const ValueKey('tm-unified-totals')), summary == '검색 못함' ? findsNothing : findsOneWidget);
    });
  }

  testWidgets('일부 원천 실패는 확인한 결과와 사유를 함께 보인다', (tester) async {
    final api = _Api(unified: true)
      ..polls = [_page([_first.first], totals: [1, 0, 0], errors: ['위계 전수 조회: 본문 색인이 없습니다.'])];
    await _pumpScreen(tester, api);
    await _type(tester, '예산');
    expect(find.textContaining('일부만 검색했습니다. 위계 전수 조회'), findsOneWidget);
    expect(find.text('파일명 일치'), findsOneWidget);
    expect(_text(tester, 'tm-unified-totals'), startsWith('확인한 결과 · 파일명 1'));
  });

  testWidgets('시작 실패는 0건이 아니라 검색 못함이다', (tester) async {
    final api = _Api(unified: true)
      ..startResponse = {'ok': false, 'message': '조사 폴더를 추가한 뒤 검색해 주세요.'};
    await _pumpScreen(tester, api);
    await _type(tester, '예산');
    expect(_text(tester, 'tm-unified-summary'), '검색 못함');
    expect(_text(tester, 'tm-unified-notice'), '검색을 완료하지 못했습니다');
    expect(find.textContaining('조사 폴더를 추가한 뒤'), findsOneWidget);
    expect(api.of('unified_poll'), isEmpty);
  });

  testWidgets('취소하면 조회를 멈추고, 검색어를 지우면 세션만 놓고 타임라인으로 돌아간다', (tester) async {
    final api = _Api(unified: true)..polls = [null];
    await _pumpScreen(tester, api);
    await _type(tester, '예산');
    expect(_text(tester, 'tm-unified-notice'), '검색 중…');
    await tester.tap(find.byKey(const ValueKey('tm-unified-cancel')));
    await tester.pumpAndSettle();
    expect(api.of('unified_cancel').single, {'generation': 1});
    expect(_text(tester, 'tm-unified-notice'), '검색을 취소했습니다');
    final polled = api.of('unified_poll').length;
    api.polls = [_page(_first)];
    await tester.pump(const Duration(seconds: 1));
    expect(api.of('unified_poll').length, polled);
    expect(find.text('파일명 일치'), findsNothing);

    await _type(tester, '');
    await tester.pumpAndSettle();
    expect(api.of('unified_release'), hasLength(1));
    expect(api.of('unified_close'), isEmpty);
    expect(find.text('이 기간에 기록된 파일이 없습니다.'), findsOneWidget);
  });

  testWidgets('종류·기간 조건을 함께 보내고 바뀌면 다시 찾는다', (tester) async {
    final api = _Api(unified: true);
    await _pumpScreen(tester, api);
    await _type(tester, '예산');
    expect(api.of('unified_start').last.containsKey('kinds'), isFalse);
    await tester.tap(find.widgetWithText(FilterChip, 'hwpx'));
    await _settle(tester);
    await tester.pumpAndSettle();
    final request = api.of('unified_start').last;
    expect(request['kinds'], ['hwp']);
    expect(request['start'], isA<double>());
  });

  testWidgets('화면을 닫으면 조회를 멈추고 세션을 놓는다', (tester) async {
    final api = _Api(unified: true)..polls = [null];
    await _pumpScreen(tester, api);
    await _type(tester, '예산');
    await tester.pumpWidget(const SizedBox());
    await tester.pumpAndSettle();
    expect(api.of('unified_close'), hasLength(1));
  });

  final captureDir = Platform.environment['UNIFIED_CAPTURE_DIR'];
  for (final (mode, theme) in [('light', lightAppTheme), ('dark', darkAppTheme)]) {
    for (final width in [700.0, 1100.0]) {
      for (final state in ['results', 'notice']) {
        testWidgets('$mode 폭 ${width.toInt()} $state 화면에 넘침이 없다', (tester) async {
          if (captureDir != null) {
            // 앱 글꼴 대신 한글이 보이는 시스템 글꼴과 Material 아이콘을 읽는다.
            await tester.runAsync(() async {
              final korean = File(r'C:\Windows\Fonts\malgun.ttf').readAsBytesSync();
              for (final family in ['NotoSansKR', 'Roboto', 'Segoe UI']) {
                await (FontLoader(family)..addFont(Future.value(ByteData.view(korean.buffer)))).load();
              }
              await (FontLoader('MaterialIcons')..addFont(rootBundle.load('fonts/MaterialIcons-Regular.otf'))).load();
            });
          }
          final api = _Api(unified: true)
            ..polls = [
              state == 'results'
                  ? _page(_first, next: 1, totals: [1, 24, 57])
                  : _page([], totals: [0, 0, 0], title: '검색을 완료하지 못했습니다', summary: '검색 못함',
                      detail: "0건은 '파일이 없다'는 뜻이 아닙니다. 파일명 전수 조회: 검색 서비스에 연결할 수 없습니다."),
            ];
          await _pumpScreen(tester, api, width: width, theme: theme);
          await _type(tester, '예산');
          await tester.pumpAndSettle();
          expect(tester.takeException(), isNull);
          if (captureDir == null) return;
          final boundary = tester.renderObject<RenderRepaintBoundary>(find.byKey(const ValueKey('capture')));
          await tester.runAsync(() async {
            final image = await boundary.toImage();
            final bytes = await image.toByteData(format: ui.ImageByteFormat.png);
            Directory(captureDir).createSync(recursive: true);
            File('$captureDir/$state-$mode-${width.toInt()}.png').writeAsBytesSync(bytes!.buffer.asUint8List());
          });
        });
      }
    }
  }
}
