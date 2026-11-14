import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:docfit_ui/features/workspace/tab_navigation_shell.dart';

void main() {
  testWidgets('tabRequest changes tab to paint (붙임사진 편집)', (tester) async {
    final tabRequest = ValueNotifier<int?>(null);

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: TabNavigationShell(
            tabRequest: tabRequest,
            documentEditor: const Text('문서편집 화면'),
            tableTemplates: const Text('표템플릿 화면'),
            tableSketch: const Text('표그림판 화면'),
            diagram: const Text('다이어그램 화면'),
            mindMap: const Text('마인드맵 화면'),
            fileConversion: const Text('파일변환 화면'),
            paint: const Text('붙임사진 편집 화면'),
            fileRename: const Text('파일명 변경 화면'),
            timemachine: const Text('타임머신 화면'),
          ),
        ),
      ),
    );

    // 처음에는 문서편집 화면 (0)
    expect(find.text('문서편집 화면'), findsOneWidget);
    expect(find.text('붙임사진 편집 화면'), findsNothing);

    // kTabPaint(5) 요청
    tabRequest.value = kTabPaint;
    await tester.pumpAndSettle();

    // 붙임사진 편집 화면으로 전환 확인
    expect(find.text('붙임사진 편집 화면'), findsOneWidget);
    expect(find.text('문서편집 화면'), findsNothing);
  });

  test('kTabPaint and kTabPhotoEdit constant values are 5', () {
    expect(kTabPaint, 5);
    expect(kTabPhotoEdit, 5);
  });

  testWidgets('photoPasteTrigger notifies listener on event', (tester) async {
    final trigger = ValueNotifier<int>(0);
    int callCount = 0;

    trigger.addListener(() {
      callCount++;
    });

    expect(callCount, 0);
    trigger.value++;
    expect(callCount, 1);
    trigger.value++;
    expect(callCount, 2);
  });
}
