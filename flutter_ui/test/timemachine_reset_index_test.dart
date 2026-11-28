import 'package:docfit_ui/features/timemachine/timemachine_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

/// 색인 초기화는 확인 대화상자를 거친다. 모든 경로는 합성 자료다.
void main() {
  testWidgets('색인 초기화는 확인해야만 보내고 취소하면 아무것도 지우지 않는다', (tester) async {
    final actions = <String>[];
    Future<Object?> api(String method, List<Object?> args) async {
      if (method == 'get_settings') return {'developer_mode': false};
      if (method != 'timemachine') return {'ok': true};
      final action = args[0] as String;
      actions.add(action);
      return switch (action) {
        'status' => {'ok': true, 'watch': 'app', 'scan': {'running': false}},
        'scan' => {'ok': true, 'by': 'app'},
        'settings_get' => {
            'ok': true,
            'settings': {'enabled': true, 'roots': [r'Z:\합성'], 'time_mode': 'max'},
          },
        'content_index_status' => {'ok': true, 'running': false},
        'day_counts' => {'ok': true, 'days': {}},
        'hour_density' => {'ok': true, 'hours': {}},
        _ => {'ok': true, 'items': []},
      };
    }

    await tester.binding.setSurfaceSize(const Size(1400, 900));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(MaterialApp(home: Scaffold(body: TimemachineScreen(api: api))));
    await tester.pumpAndSettle();
    await tester.tap(find.text('본문검색'));
    await tester.pumpAndSettle();

    await tester.tap(find.byKey(const ValueKey('tm-reset-index')));
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('tm-reset-index-dialog')), findsOneWidget);
    expect(find.textContaining('원본 파일은 건드리지 않아요'), findsOneWidget);
    await tester.tap(find.text('취소'));
    await tester.pumpAndSettle();
    expect(actions, isNot(contains('content_index_reset')));

    await tester.tap(find.byKey(const ValueKey('tm-reset-index')));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('tm-reset-index-confirm')));
    await tester.pumpAndSettle();
    expect(actions.where((a) => a == 'content_index_reset'), hasLength(1));

    await tester.pumpWidget(const SizedBox());
  });
}
