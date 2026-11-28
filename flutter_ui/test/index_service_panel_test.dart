import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:docfit_ui/features/settings/index_service_panel.dart';

void main() {
  Widget host(Future<Object?> Function(String, List<Object?>) api) =>
      MaterialApp(home: Scaffold(body: IndexServicePanel(api: api)));

  testWidgets('꺼짐 상태에서 안내 후 관리자 권한으로 켜기', (tester) async {
    final calls = <String>[];
    var installed = false;
    await tester.pumpWidget(host((method, args) async {
      expect(method, 'index_service');
      calls.add(args.first as String);
      if (args.first == 'install') installed = true;
      return {
        'ok': true,
        'supported': true,
        'installed': installed,
        'running': installed,
        'busy': null,
        'message': installed ? '서비스를 시작했습니다.' : '',
        'last_ok': installed ? true : null,
      };
    }));
    await tester.pumpAndSettle();
    expect(calls, ['status']);
    expect(find.textContaining('꺼짐'), findsWidgets);
    expect(find.byKey(const ValueKey('index-service-install')), findsOneWidget);

    await tester.tap(find.byKey(const ValueKey('index-service-install')));
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('index-service-install-dialog')), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('index-service-install-confirm')));
    await tester.pumpAndSettle();
    expect(calls, ['status', 'install']);
    expect(find.textContaining('켜짐'), findsWidgets);
    expect(find.text('서비스를 시작했습니다.'), findsOneWidget);
    expect(find.byKey(const ValueKey('index-service-uninstall')), findsOneWidget);
    expect(find.byKey(const ValueKey('index-service-install')), findsNothing);
  });

  testWidgets('승인을 거절하면 오류 메시지를 보이고 꺼짐 유지', (tester) async {
    await tester.pumpWidget(host((method, args) async {
      if (args.first == 'install') {
        return {'ok': true, 'supported': true, 'installed': false, 'running': false, 'busy': null,
            'message': '관리자 승인을 거절했습니다.', 'last_ok': false};
      }
      return {'ok': true, 'supported': true, 'installed': false, 'running': false, 'busy': null, 'message': '', 'last_ok': null};
    }));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('index-service-install')));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('index-service-install-confirm')));
    await tester.pumpAndSettle();
    expect(find.text('관리자 승인을 거절했습니다.'), findsOneWidget);
    expect(find.byKey(const ValueKey('index-service-install')), findsOneWidget);
  });
}
