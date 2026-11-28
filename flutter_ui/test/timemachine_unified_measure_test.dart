import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:docfit_ui/features/timemachine/timemachine_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

/// 앱 화면의 첫 결과 시간 측정 도구. 환경 변수 UNIFIED_BENCH_EXE·KTM_HOST_EXE·UNIFIED_MEASURE_OUT 이
/// 모두 있을 때만 돌고, 일반 시험에서는 건너뛴다. 합성 색인만 쓴다(tools/unified_app_measure.py).
///
/// 시계: 검색어를 넣기 직전부터 첫 결과 행이 있는 프레임까지(시험 렌더러, 실제 화면 표시는 제외).
void main() {
  final bench = Platform.environment['UNIFIED_BENCH_EXE'];
  final ktm = Platform.environment['KTM_HOST_EXE'];
  final out = Platform.environment['UNIFIED_MEASURE_OUT'];
  final skip = bench == null || ktm == null || out == null;
  if (skip) {
    test('통합 보기 화면 측정(환경 변수 없으면 건너뜀)', () {}, skip: true);
    return;
  }
  final binding = LiveTestWidgetsFlutterBinding.ensureInitialized();
  binding.framePolicy = LiveTestWidgetsFlutterBindingFramePolicy.fullyLive;

  testWidgets('통합 보기 첫 결과·전체 결과 시간', (tester) async {
    final process = await Process.start(
        'python', ['../tools/unified_app_measure.py', bench, ktm],
        environment: {'PYTHONIOENCODING': 'utf-8'});
    final lines = StreamIterator(process.stdout.transform(utf8.decoder).transform(const LineSplitter()));
    final errors = process.stderr.transform(utf8.decoder).join();
    expect(await lines.moveNext(), isTrue, reason: '측정 연결이 시작되지 않았습니다');

    // 앱의 IPC처럼 요청을 하나씩 차례로 처리한다.
    var queue = Future<void>.value();
    Future<Object?> call(String method, List<Object?> args) {
      if (method != 'timemachine') return Future.value({'developer_mode': false});
      final done = Completer<Object?>();
      queue = queue.then((_) async {
        process.stdin.add(utf8.encode('${jsonEncode([args[0], args.length > 1 ? args[1] : {}])}\n'));
        await lines.moveNext();
        done.complete(jsonDecode(lines.current));
      });
      return done.future;
    }

    await tester.binding.setSurfaceSize(const Size(1100, 900));
    await tester.pumpWidget(MaterialApp(home: Scaffold(body: TimemachineScreen(api: call))));

    Future<void> until(bool Function() check, String what) async {
      final limit = Stopwatch()..start();
      while (!check()) {
        if (limit.elapsed > const Duration(seconds: 30)) fail('시간 초과: $what');
        await Future<void>.delayed(const Duration(milliseconds: 1));
        await tester.pump();
      }
    }

    final field = find.byKey(const ValueKey('tm-file-search'));
    final totals = find.byKey(const ValueKey('tm-unified-totals'));
    final row = find.byWidgetPredicate((w) => w.key is ValueKey<String> &&
        (w.key! as ValueKey<String>).value.startsWith('tm-unified-row-'));
    await until(() => field.evaluate().isNotEmpty && find.text('전체').evaluate().isNotEmpty, '화면');
    // 화면 초기화가 이번 달 범위를 잡은 뒤에 눌러야 한다.
    await Future<void>.delayed(const Duration(seconds: 2));
    await tester.pump();
    // 합성 문서의 날짜는 실제 달력과 무관하므로 기간 없음으로 잰다.
    await tester.tap(find.text('전체'));
    await Future<void>.delayed(const Duration(milliseconds: 500));
    await tester.pump();

    Future<Map<String, Object?>> measure(String query, {required bool enter}) async {
      await tester.enterText(field, '');
      await until(() => row.evaluate().isEmpty, '비우기');
      await Future<void>.delayed(const Duration(milliseconds: 300));
      final controller = tester.widget<TextField>(field).controller!;
      final clock = Stopwatch()..start();
      controller.text = query;
      if (enter) tester.widget<TextField>(field).onSubmitted!(query);
      await until(() => row.evaluate().isNotEmpty, '첫 결과 $query');
      final first = clock.elapsedMicroseconds / 1000;
      await until(() => totals.evaluate().isNotEmpty, '전체 결과 $query');
      final done = clock.elapsedMicroseconds / 1000;
      // 다른 조건(기간 등)으로 잰 값을 기록하지 않는다.
      expect(tester.widget<Text>(totals).data,
          query == '예산집행' ? '파일명 60 · 위계 449 · 본문 0' : '파일명 1 · 위계 3 · 본문 0');
      return {'first_ms': first, 'complete_ms': done, 'totals': tester.widget<Text>(totals).data};
    }

    final report = <String, Object?>{
      'documents': 1000,
      'clock': '검색어 입력 직전 → 첫 결과 행/전체 개수가 있는 프레임(flutter_test, 실제 화면 표시 제외)',
      'transport': '화면 → 파이썬 TimemachineService(표준 입출력) → ktm --search-host → 합성 색인·격리 파이프',
      // 시험 렌더러(디버그 모드)에서 바뀐 것 없이 한 프레임을 도는 시간.
      'idle_frame_ms': await () async {
        final clock = Stopwatch()..start();
        for (var i = 0; i < 20; i++) {
          await tester.pump();
        }
        return clock.elapsedMicroseconds / 20000;
      }(),
      // 호스트 프로세스를 처음 띄우는 비용이 든 첫 검색은 따로 적는다.
      'cold': await measure('예산집행', enter: true),
    };
    for (final enter in [false, true]) {
      final runs = <String, Object?>{};
      for (final query in ['예산집행', '2026; 공사 계약']) {
        runs[query] = [for (var i = 0; i < 5; i++) await measure(query, enter: enter)];
      }
      report[enter ? 'enter' : 'typing_with_150ms_delay'] = runs;
    }
    File(out).writeAsStringSync('${const JsonEncoder.withIndent('  ').convert(report)}\n');

    await tester.pumpWidget(const SizedBox());
    await queue;
    await process.stdin.close();
    expect(await process.exitCode.timeout(const Duration(seconds: 30)), 0, reason: await errors);
  });
}
