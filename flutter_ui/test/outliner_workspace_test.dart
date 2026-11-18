import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:docfit_ui/features/outliner/outliner_workspace.dart';

void main() {
  testWidgets('renders page navigation, daily journal and backlinks panel', (
    tester,
  ) async {
    Future<Object?> api(String method, [List<Object?> args = const []]) async {
      if (method == 'outliner') {
        final payload = args.first as Map;
        switch (payload['action']) {
          case 'list':
            return {
              'ok': true,
              'data': {
                'pages': [
                  {'name': 'Home'},
                ],
              },
            };
          case 'daily':
          case 'load':
            return {
              'ok': true,
              'data': {
                'page': payload['action'] == 'daily'
                    ? '2026-11-18'
                    : payload['page'],
                'markdown': '- Hello\n',
                'version': 'v1',
                'document': {
                  'blocks': [
                    {'id': 1, 'text': 'Hello', 'indent': 0, 'folded': false},
                  ],
                },
              },
            };
          case 'backlinks':
            return {
              'ok': true,
              'data': {'block_ids': []},
            };
        }
      }
      return {'ok': true, 'data': {}};
    }

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(body: OutlinerWorkspace(api: api)),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('오늘의 일지'), findsOneWidget);
    expect(find.text('Home'), findsOneWidget);
    expect(find.text('역링크'), findsOneWidget);
    expect(find.text('Hello'), findsOneWidget);
  });
}
