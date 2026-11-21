import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:docfit_ui/features/file_rename/rename_ui_gate.dart';

void main() {
  testWidgets('defaults to existing Flutter file rename screen', (tester) async {
    final calls = <String>[];
    Future<dynamic> api(String method, List<dynamic> args) async {
      calls.add(method);
      return {'ok': true, 'data': {'mode': 'flutter'}};
    }
    await tester.pumpWidget(MaterialApp(home: RenameUiGate(api: api, flutterBuilder: (_) => const Text('기존 파일명 화면'))));
    await tester.pumpAndSettle();
    expect(find.byType(RenameUiGate), findsOneWidget);
    expect(calls, ['rename_window']);
    expect(find.text('기존 파일명 화면'), findsOneWidget);
  });
}
