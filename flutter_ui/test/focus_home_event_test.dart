import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:docfit_ui/features/workspace/tab_navigation_shell.dart';

void main() {
  testWidgets('focus home tab request selects index zero', (tester) async {
    final request = ValueNotifier<int?>(null);
    await tester.pumpWidget(MaterialApp(
      home: Scaffold(
        body: TabNavigationShell(
          initialIndex: 4,
          tabRequest: request,
          documentEditor: const Text('document'),
          tableTemplates: const Text('templates'),
          tableSketch: const Text('sketch'),
          diagram: const Text('diagram'),
          mindMap: const Text('mindmap'),
          fileConversion: const Text('conversion'),
          paint: const Text('paint'),
          fileRename: const Text('rename'),
          timemachine: const Text('time'),
        ),
      ),
    ));
    request.value = 0;
    await tester.pumpAndSettle();
    expect(find.text('document'), findsOneWidget);
    request.dispose();
  });
}
