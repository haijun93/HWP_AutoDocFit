import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:docfit_ui/features/workspace/tab_navigation_shell.dart';
import 'package:docfit_ui/features/workspace/workspace_adapters.dart';

void main() {
  testWidgets('IntegratedWorkspace mounts TabNavigationShell with 12 tabs', (tester) async {
    tester.view.physicalSize = const Size(1400, 900);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    int currentTab = 0;
    Future<Object?> mockApi(String endpoint, List<Object?> params) async {
      return {'ok': true};
    }

    await tester.pumpWidget(
      MaterialApp(
        home: IntegratedWorkspace(
          api: mockApi,
          documentEditorWidget: const Text('DocEditorContent'),
          fileConversionWidget: const Text('FileConversionContent'),
          onTabChanged: (idx) => currentTab = idx,
        ),
      ),
    );

    // Initial state: Tab 0 active
    expect(find.byType(TabNavigationShell), findsOneWidget);
    expect(find.text('DocEditorContent'), findsOneWidget);
    expect(find.text('문서편집'), findsOneWidget);
    expect(find.text('표템플릿'), findsOneWidget);
    expect(find.text('표그림판'), findsOneWidget);
    expect(find.text('다이어그램'), findsOneWidget);
    expect(find.text('마인드맵'), findsOneWidget);
    expect(find.text('파일명 일괄변경'), findsOneWidget);
    expect(find.text('몽땅검색'), findsOneWidget);
    expect(find.text('문서뷰'), findsOneWidget);
    expect(find.text('사진뷰'), findsOneWidget);
    expect(find.text('파일변환'), findsOneWidget);
    expect(find.text('PDF 편집'), findsOneWidget);

    await tester.ensureVisible(find.byKey(const Key('workspaceTab-10')));
    await tester.tap(find.byKey(const Key('workspaceTab-10')));
    await tester.pumpAndSettle();
    expect(currentTab, 10);
    expect(find.text('FileConversionContent'), findsOneWidget);

    await tester.ensureVisible(find.byKey(const Key('workspaceTab-11')));
    await tester.tap(find.byKey(const Key('workspaceTab-11')));
    await tester.pumpAndSettle();
    expect(currentTab, 11);
    expect(find.text('PDF 편집'), findsWidgets);

    // Switch back to Tab 0 (문서편집) — 탭 이름이 길어져 좁은 창에서는 탭 줄을 다시 밀어야 보인다.
    await tester.ensureVisible(find.text('문서편집'));
    await tester.tap(find.text('문서편집'));
    await tester.pumpAndSettle();
    expect(currentTab, 0);
    expect(find.text('DocEditorContent'), findsOneWidget);
  });

  testWidgets('Each adapter wraps child in stable KeyedSubtree', (tester) async {
    const docKey = ValueKey('tab-document-editor-adapter');
    const convKey = ValueKey('tab-file-conversion-adapter');
    const docGridKey = ValueKey('tab-doc-grid-adapter');
    const photoAlbumKey = ValueKey('tab-photo-album-adapter');

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: Column(
            children: [
              const DocumentEditorAdapter(child: Text('Child 1')),
              const FileConversionAdapter(child: Text('Child 2')),
              Expanded(child: DocGridAdapter(api: (endpoint, params) async => {'ok': true})),
              Expanded(child: PhotoAlbumAdapter(api: (endpoint, params) async => {'ok': true})),
            ],
          ),
        ),
      ),
    );

    expect(find.byKey(docKey), findsOneWidget);
    expect(find.byKey(convKey), findsOneWidget);
    expect(find.byKey(docGridKey), findsOneWidget);
    expect(find.byKey(photoAlbumKey), findsOneWidget);
  });

  test('tab constants keep conversion before PDF editing', () {
    expect(kTabFileRename, 6);
    expect(kTabTimemachine, 7);
    expect(kTabDocGrid, 8);
    expect(kTabPhotoAlbum, 9);
    expect(kTabFileConversion, 10);
    expect(kTabPdfEdit, 11);
    expect(kTabCount, 12);
  });
}
