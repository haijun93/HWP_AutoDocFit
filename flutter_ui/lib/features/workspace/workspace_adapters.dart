import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';

import '../../core/api/abbreviation_api.dart' show ApiCall;
import '../file_rename/file_rename_screen.dart';
import '../timemachine/timemachine_screen.dart';
import '../flows/flow_editor_screen.dart';
import '../library/library_api.dart';
import '../library/library_document_search.dart';
import '../library/library_screen.dart';
import '../notebook/notebook_api.dart';
import '../notebook/notebook_screen.dart';
import '../external_apps/external_app_launcher.dart';
import '../doc_grid/doc_grid_screen.dart';
import '../doc_grid/docview_ui_gate.dart';
import '../photo_album/photo_album_screen.dart';
import '../photo_album/photoview_ui_gate.dart';
import '../table_sketch/table_sketch_screen.dart';
import '../table_units/table_units_screen.dart';
import '../pdf_edit/pdf_edit_workspace.dart';
import '../outliner/outliner_ui_gate.dart';
import '../qms_bench/qms_bench_screen.dart';
import 'tab_navigation_shell.dart';
import 'quick_action_bar.dart';

/// Workspace adapter for Tab 1: Document Editor / Processing
class DocumentEditorAdapter extends StatelessWidget {
  const DocumentEditorAdapter({super.key, required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) => KeyedSubtree(
    key: const ValueKey('tab-document-editor-adapter'),
    child: child,
  );
}

/// Workspace adapter for Tab 2: Table Templates
class TableTemplatesAdapter extends StatelessWidget {
  const TableTemplatesAdapter({super.key, required this.api});

  final ApiCall api;

  @override
  Widget build(BuildContext context) => KeyedSubtree(
    key: const ValueKey('tab-table-templates-adapter'),
    child: TableUnitsScreen(api: api),
  );
}

/// Workspace adapter for Tab 3: Table Sketch
class TableSketchAdapter extends StatelessWidget {
  const TableSketchAdapter({super.key, required this.api});

  final ApiCall api;

  @override
  Widget build(BuildContext context) => KeyedSubtree(
    key: const ValueKey('tab-table-sketch-adapter'),
    child: TableSketchScreen(api: api),
  );
}

/// Workspace adapter for Tab 4: Diagram Editor
class DiagramAdapter extends StatelessWidget {
  const DiagramAdapter({super.key, required this.api});

  final ApiCall api;

  @override
  Widget build(BuildContext context) => KeyedSubtree(
    key: const ValueKey('tab-diagram-adapter'),
    // 탭에서도 상단 메뉴와 같은 편집기(다이어그램 모드)를 연다 — 예전 도형 편집기는 웹 화면에서
    // "Flutter Windows 앱에서만" 안내만 보였다(2026-10-29 사용자 보고).
    child: FlowEditorScreen(api: api, initialMode: 'diagram', isModal: false),
  );
}

/// Workspace adapter for Tab 5: Mind Map / Flow Editor
class MindMapAdapter extends StatelessWidget {
  const MindMapAdapter({super.key, required this.api});

  final ApiCall api;

  @override
  Widget build(BuildContext context) => KeyedSubtree(
    key: const ValueKey('tab-mindmap-adapter'),
    child: FlowEditorScreen(api: api, initialMode: 'mindmap', isModal: false),
  );
}

/// Workspace adapter for Tab 6: 그림판 (photo background + pen, shapes, text boxes)
class PaintAdapter extends StatelessWidget {
  const PaintAdapter({super.key, required this.api, this.photoPasteTrigger});

  final ApiCall api;
  final ValueListenable<int>? photoPasteTrigger;

  @override
  Widget build(BuildContext context) => KeyedSubtree(
    key: const ValueKey('tab-paint-adapter'),
    child: FlowEditorScreen(api: api, initialMode: 'paint', isModal: false, photoPasteTrigger: photoPasteTrigger),
  );
}

/// Workspace adapter for Tab 7: 파일이름 (bulk rename with rules, preview, undo)
class FileRenameAdapter extends StatelessWidget {
  const FileRenameAdapter({super.key, required this.api});

  final ApiCall api;

  @override
  Widget build(BuildContext context) => KeyedSubtree(
    key: const ValueKey('tab-file-rename-adapter'),
    child: FileRenameScreen(api: api),
  );
}

/// Workspace adapter for Tab 8: 몽땅검색.
class TimemachineAdapter extends StatelessWidget {
  const TimemachineAdapter({super.key, required this.api});

  final ApiCall api;

  @override
  Widget build(BuildContext context) => KeyedSubtree(
    key: const ValueKey('tab-timemachine-adapter'),
    child: TimemachineScreen(api: api),
  );
}

/// Workspace adapter for Tab 9: 문서뷰 (Document thumbnail grid view).
class DocGridAdapter extends StatelessWidget {
  const DocGridAdapter({super.key, required this.api, this.directoryPicker});

  final ApiCall api;
  final Future<String?> Function()? directoryPicker;

  @override
  Widget build(BuildContext context) => KeyedSubtree(
    key: const ValueKey('tab-doc-grid-adapter'),
    child: DocGridScreen(api: api, directoryPicker: directoryPicker),
  );
}

/// Workspace adapter for Tab 10: 사진뷰 (Photo library & photo album sheet).
class PhotoAlbumAdapter extends StatelessWidget {
  const PhotoAlbumAdapter({super.key, required this.api, this.directoryPicker});

  final ApiCall api;
  final Future<String?> Function()? directoryPicker;

  @override
  Widget build(BuildContext context) => KeyedSubtree(
    key: const ValueKey('tab-photo-album-adapter'),
    child: PhotoAlbumScreen(api: api, directoryPicker: directoryPicker),
  );
}

/// Workspace adapter for Tab 11 (always last): File Format Conversion & Merge
class FileConversionAdapter extends StatelessWidget {
  const FileConversionAdapter({super.key, required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) => KeyedSubtree(
    key: const ValueKey('tab-file-conversion-adapter'),
    child: child,
  );
}

/// Integrated 6-Tab Workspace Root Shell
class IntegratedWorkspace extends StatelessWidget {
  const IntegratedWorkspace({
    super.key,
    required this.api,
    required this.documentEditorWidget,
    required this.fileConversionWidget,
    this.initialIndex = 0,
    this.tabRequest,
    this.photoPasteTrigger,
    this.onTabChanged,
    this.trailing,
    this.savedSidebarExpanded,
    this.onSidebarExpandedChanged,
    this.developerMode = false,
    this.libraryScreen,
    this.notebookScreen,
    this.docGridScreen,
    this.photoAlbumScreen,
    this.qmsBenchScreen,
    this.hiddenTabs = kDefaultHiddenTabs,
    this.quickActions = const [],
  });

  /// 오른쪽 빠른 실행 막대 항목.
  final List<QuickAction> quickActions;
  final Widget? trailing;
  final ValueListenable<bool?>? savedSidebarExpanded;
  final ValueChanged<bool>? onSidebarExpandedChanged;
  final bool developerMode;
  final Widget? libraryScreen;
  final Widget? notebookScreen;
  final Widget? docGridScreen;
  final Widget? photoAlbumScreen;
  final Widget? qmsBenchScreen;
  /// 사용자가 끈 왼쪽 메뉴 번호(설정 > 쓸 메뉴 고르기).
  final Set<int> hiddenTabs;
  final ApiCall api;
  final Widget documentEditorWidget;
  final Widget fileConversionWidget;
  final int initialIndex;
  final ValueListenable<int?>? tabRequest;
  final ValueListenable<int>? photoPasteTrigger;
  final ValueChanged<int>? onTabChanged;

  @override
  Widget build(BuildContext context) {
    return TabNavigationShell(
      hiddenTabs: hiddenTabs,
      initialIndex: initialIndex,
      tabRequest: tabRequest,
      onTabChanged: onTabChanged,
      trailing: trailing,
      savedSidebarExpanded: savedSidebarExpanded,
      onSidebarExpandedChanged: onSidebarExpandedChanged,
      developerMode: developerMode,
      libraryScreen: libraryScreen ?? LibraryScreen(
            api: BridgeLibraryApi(api),
            notebookApi: BridgeNotebookApi(api),
            scopedDocumentSearch: scopedLibraryDocumentSearch(api),
          ),
      notebookScreen: notebookScreen ?? NotebookScreen(api: BridgeNotebookApi(api), library: BridgeLibraryApi(api)),
      onOpenExternalApp: (context, app) => openExternalApp(context, api, app),
      quickActions: quickActions,
      documentEditor: DocumentEditorAdapter(child: documentEditorWidget),
      tableTemplates: TableTemplatesAdapter(api: api),
      tableSketch: TableSketchAdapter(api: api),
      diagram: DiagramAdapter(api: api),
      mindMap: MindMapAdapter(api: api),
      fileConversion: FileConversionAdapter(child: fileConversionWidget),
      pdfEdit: PdfEditWorkspace(api: api),
      paint: PaintAdapter(api: api, photoPasteTrigger: photoPasteTrigger),
      fileRename: FileRenameAdapter(api: api),
      timemachine: _LazyTimemachineAdapter(api: api),
      docGrid: docGridScreen ?? _LazyDocGridAdapter(api: api),
      photoAlbum: photoAlbumScreen ?? _LazyPhotoAlbumAdapter(api: api),
      outliner: _LazyOutlinerAdapter(api: api),
      qmsBench: qmsBenchScreen ?? _LazyQmsBenchAdapter(api: api),
    );
  }
}

class _LazyTimemachineAdapter extends StatefulWidget {
  const _LazyTimemachineAdapter({required this.api});
  final ApiCall api;

  @override
  State<_LazyTimemachineAdapter> createState() => _LazyTimemachineAdapterState();
}

class _LazyTimemachineAdapterState extends State<_LazyTimemachineAdapter> {
  bool _visited = false;

  @override
  Widget build(BuildContext context) {
    final active = TickerMode.valuesOf(context).enabled;
    if (active && !_visited) _visited = true;
    return _visited
        ? TimemachineAdapter(api: widget.api)
        : const SizedBox.expand();
  }
}

class _LazyDocGridAdapter extends StatefulWidget {
  const _LazyDocGridAdapter({required this.api});
  final ApiCall api;

  @override
  State<_LazyDocGridAdapter> createState() => _LazyDocGridAdapterState();
}

class _LazyDocGridAdapterState extends State<_LazyDocGridAdapter> {
  bool _visited = false;

  @override
  Widget build(BuildContext context) {
    final active = TickerMode.valuesOf(context).enabled;
    if (active && !_visited) _visited = true;
    return _visited
        ? DocViewUiGate(api: widget.api)
        : const SizedBox.expand();
  }
}

class _LazyPhotoAlbumAdapter extends StatefulWidget {
  const _LazyPhotoAlbumAdapter({required this.api});
  final ApiCall api;

  @override
  State<_LazyPhotoAlbumAdapter> createState() => _LazyPhotoAlbumAdapterState();
}

class _LazyOutlinerAdapter extends StatefulWidget {
  const _LazyOutlinerAdapter({required this.api});
  final ApiCall api;

  @override
  State<_LazyOutlinerAdapter> createState() => _LazyOutlinerAdapterState();
}

class _LazyOutlinerAdapterState extends State<_LazyOutlinerAdapter> {
  bool _visited = false;

  @override
  Widget build(BuildContext context) {
    final active = TickerMode.valuesOf(context).enabled;
    if (active && !_visited) _visited = true;
    return _visited
        ? OutlinerUiGate(api: widget.api)
        : const SizedBox.expand();
  }
}

class _LazyPhotoAlbumAdapterState extends State<_LazyPhotoAlbumAdapter> {
  bool _visited = false;

  @override
  Widget build(BuildContext context) {
    final active = TickerMode.valuesOf(context).enabled;
    if (active && !_visited) _visited = true;
    return _visited
        ? PhotoViewUiGate(api: widget.api)
        : const SizedBox.expand();
  }
}

/// Workspace adapter for Tab 15: 검색 벤치마크 (Everything vs kfind).
class QmsBenchAdapter extends StatelessWidget {
  const QmsBenchAdapter({super.key, required this.api});

  final ApiCall api;

  @override
  Widget build(BuildContext context) => KeyedSubtree(
    key: const ValueKey('tab-qms-bench-adapter'),
    child: QmsBenchScreen(api: api),
  );
}

class _LazyQmsBenchAdapter extends StatefulWidget {
  const _LazyQmsBenchAdapter({required this.api});
  final ApiCall api;

  @override
  State<_LazyQmsBenchAdapter> createState() => _LazyQmsBenchAdapterState();
}

class _LazyQmsBenchAdapterState extends State<_LazyQmsBenchAdapter> {
  bool _visited = false;

  @override
  Widget build(BuildContext context) {
    final active = TickerMode.valuesOf(context).enabled;
    if (active && !_visited) _visited = true;
    return _visited
        ? QmsBenchAdapter(api: widget.api)
        : const SizedBox.expand();
  }
}
