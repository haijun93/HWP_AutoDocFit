import 'package:docfit_ui/features/workspace/tab_navigation_shell.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

TabNavigationShell shell({int? solo, ValueChanged<int>? changed, ValueNotifier<int?>? request}) => TabNavigationShell(
      soloTab: solo,
      onTabChanged: changed,
      tabRequest: request,
      libraryScreen: const Text('page-library'),
      documentEditor: const Text('page-0'),
      tableTemplates: const Text('page-1'),
      tableSketch: const Text('page-2'),
      diagram: const Text('page-3'),
      mindMap: const Text('page-4'),
      paint: const Text('page-5'),
      fileRename: const Text('page-6'),
      timemachine: const Text('page-7'),
      docGrid: const Text('page-8'),
      photoAlbum: const Text('page-9'),
      fileConversion: const Text('page-10'),
      pdfEdit: const Text('page-11'),
    );

void main() {
  test('주소 조각에서 전용 창 번호를 읽는다', () {
    expect(soloTabFrom(Uri.parse('http://127.0.0.1:1/index.html#screen=3')), 3);
    expect(soloTabFrom(Uri.parse('http://127.0.0.1:1/index.html#/screen=12')), 12);
    for (final url in ['index.html', 'index.html#', 'index.html#screen=7', 'index.html#screen=16', 'index.html#screen=x']) {
      expect(soloTabFrom(Uri.parse(url)), isNull, reason: url);
    }
  });

  testWidgets('전용 창은 메뉴 없이 그 화면만 보이고 다른 화면으로 바뀌지 않는다', (tester) async {
    final changed = <int>[];
    final request = ValueNotifier<int?>(null);
    await tester.pumpWidget(MaterialApp(home: shell(solo: 3, changed: changed.add, request: request)));
    await tester.pump();
    expect(find.byType(WorkspaceTabBar), findsNothing);
    expect(find.text('page-3'), findsOneWidget);
    expect(find.text('page-1', skipOffstage: false), findsNothing);
    expect(find.text('page-0', skipOffstage: false), findsOneWidget);
    expect(changed, [3]);
    request.value = 1;
    await tester.pump();
    expect(find.text('page-3'), findsOneWidget);
    expect(changed, [3]);
  });

  testWidgets('가상 폴더 전용 창은 뒤로 단추 없이 그 화면을 보인다', (tester) async {
    await tester.pumpWidget(MaterialApp(home: shell(solo: 12)));
    await tester.pump();
    expect(find.text('page-library'), findsOneWidget);
    expect(find.byType(HostClosablePage), findsNothing);
    expect(find.byType(WorkspaceTabBar), findsNothing);
  });
}
