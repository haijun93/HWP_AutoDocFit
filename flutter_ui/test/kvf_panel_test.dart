import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:docfit_ui/features/settings/kvf_panel.dart';

void main() {
  testWidgets('기본 꺼짐과 켜기 뒤 실제 상태 표시', (tester) async {
    final calls = <String>[];
    var running = false;
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: KvfPanel(
            api: (method, args) async {
              expect(method, 'kvf');
              calls.add(args.first as String);
              if (args.first == 'enable') running = true;
              if (args.first == 'disable') running = false;
              return {'ok': true, 'running': running, 'autostart': running};
            },
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(calls, ['status']);
    expect(find.text('꺼짐'), findsOneWidget);
    await tester.tap(find.text('가상 폴더 창 켜기'));
    await tester.pumpAndSettle();
    expect(calls, ['status', 'enable']);
    expect(find.text('켜짐'), findsOneWidget);
    await tester.tap(find.text('가상 폴더 창 켜기'));
    await tester.pumpAndSettle();
    expect(calls.last, 'disable');
    expect(find.text('꺼짐'), findsOneWidget);
  });

  testWidgets('켜기 실패 때 꺼짐 유지하고 오류 표시', (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: KvfPanel(
            api: (method, args) async => args.first == 'status'
                ? {'ok': true, 'running': false}
                : {'ok': false, 'message': '설치 실패'},
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('가상 폴더 창 켜기'));
    await tester.pumpAndSettle();
    expect(find.textContaining('설치 실패'), findsOneWidget);
    expect(find.text('꺼짐'), findsOneWidget);
  });
}
