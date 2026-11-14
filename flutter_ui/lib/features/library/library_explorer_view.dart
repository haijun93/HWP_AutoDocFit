import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/gestures.dart' show kPrimaryButton, kSecondaryButton;
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../core/theme/app_theme.dart';
import 'library_api.dart';
import 'library_colors.dart';
import 'library_screen.dart' show LibraryFileDrop;

/// 자료함 '탐색기 형식' 보기. 캔버스형과 같은 자료를 Windows 파일 탐색기처럼 폴더 트리·목록으로 보여 준다.
/// 자료(영역·좌표·관계선)는 바꾸지 않고 표시만 다르다. 좌표·관계선은 이 보기에서 보이지 않지만 그대로 보존된다.

/// 목록 보기 방식(탐색기의 '보기' 메뉴와 같은 이름).
enum LibraryExplorerStyle { details, list, largeIcons, mediumIcons, smallIcons }

const libraryExplorerStyleNames = {
  LibraryExplorerStyle.details: '자세히',
  LibraryExplorerStyle.list: '목록',
  LibraryExplorerStyle.largeIcons: '큰 아이콘',
  LibraryExplorerStyle.mediumIcons: '보통 아이콘',
  LibraryExplorerStyle.smallIcons: '작은 아이콘',
};

/// 자세히 보기의 열.
enum LibraryExplorerColumn { name, kind, size, modified, source, space }

const libraryExplorerColumnNames = {
  LibraryExplorerColumn.name: '이름',
  LibraryExplorerColumn.kind: '종류',
  LibraryExplorerColumn.size: '크기',
  LibraryExplorerColumn.modified: '수정한 날짜',
  LibraryExplorerColumn.source: '자료 경로',
  LibraryExplorerColumn.space: '영역',
};

/// 목록에서 끌어 가는 자료 묶음(트리·폴더 줄에 놓으면 그 영역으로 이동, Ctrl을 누르고 놓으면 복사).
class LibraryExplorerDrag {
  const LibraryExplorerDrag(this.items);
  final List<LibraryItem> items;
}

/// 보기 상태(현재 폴더·방문 기록·정렬·열 폭·파일 정보 캐시). 화면이 들고 있어 캔버스와 오가도 유지된다.
class LibraryExplorerController {
  /// null이면 자료함 맨 위(4개 영역 폴더가 보인다).
  LibrarySpace? folder;
  final List<LibrarySpace?> backStack = [];
  final List<LibrarySpace?> forwardStack = [];
  LibraryExplorerStyle style = LibraryExplorerStyle.details;
  LibraryExplorerColumn sortColumn = LibraryExplorerColumn.name;
  bool ascending = true;
  final Map<LibraryExplorerColumn, double> widths = {
    LibraryExplorerColumn.name: 260,
    LibraryExplorerColumn.kind: 120,
    LibraryExplorerColumn.size: 90,
    LibraryExplorerColumn.modified: 140,
    LibraryExplorerColumn.source: 280,
    LibraryExplorerColumn.space: 90,
  };
  String? anchorId;
  String? focusId;

  /// 자료 id → 파일 정보(조회한 것만). 자료함을 바꾸면 비운다.
  final Map<String, LibraryFileInfo> info = {};

  static const minColumnWidth = 48.0;

  bool navigate(LibrarySpace? to) {
    if (to == folder) return false;
    backStack.add(folder);
    forwardStack.clear();
    folder = to;
    anchorId = null;
    focusId = null;
    return true;
  }

  bool back() {
    if (backStack.isEmpty) return false;
    forwardStack.add(folder);
    folder = backStack.removeLast();
    anchorId = focusId = null;
    return true;
  }

  bool forward() {
    if (forwardStack.isEmpty) return false;
    backStack.add(folder);
    folder = forwardStack.removeLast();
    anchorId = focusId = null;
    return true;
  }

  bool up() => folder != null && navigate(null);

  /// 다른 자료함을 열 때: 맨 위로, 기록·캐시 비우기(정렬·보기 방식·열 폭은 유지).
  void resetForBox() {
    folder = null;
    backStack.clear();
    forwardStack.clear();
    info.clear();
    anchorId = focusId = null;
  }

  void resize(LibraryExplorerColumn column, double delta) {
    widths[column] = math.max(minColumnWidth, (widths[column] ?? 100) + delta);
  }
}

/// 종류 열 글자. 파일은 확장자로 'HWPX 파일'처럼 쓴다.
String libraryKindLabel(LibraryItem item) {
  switch (item.kind) {
    case LibraryItemKind.file:
      final name = (item.source ?? item.title).replaceAll('\\', '/').split('/').last;
      final dot = name.lastIndexOf('.');
      return dot > 0 && dot < name.length - 1 ? '${name.substring(dot + 1).toUpperCase()} 파일' : '파일';
    case LibraryItemKind.folder:
      return '파일 폴더';
    case LibraryItemKind.link:
      return '웹 주소';
    case LibraryItemKind.text:
      return '텍스트';
    case LibraryItemKind.richText:
      return '서식 텍스트';
    case LibraryItemKind.image:
      return '그림';
    case LibraryItemKind.capture:
      return '화면 캡처';
    case LibraryItemKind.note:
      return '메모';
  }
}

/// 크기 열 글자(탐색기처럼 KB 단위, 올림). 값이 없으면 빈 칸.
String formatLibrarySize(int? bytes) {
  if (bytes == null) return '';
  final kb = bytes == 0 ? 0 : (bytes + 1023) ~/ 1024;
  final digits = '$kb';
  final buffer = StringBuffer();
  for (var i = 0; i < digits.length; i++) {
    if (i > 0 && (digits.length - i) % 3 == 0) buffer.write(',');
    buffer.write(digits[i]);
  }
  return '$buffer KB';
}

String formatLibraryDate(DateTime? time) {
  if (time == null) return '';
  String two(int n) => n.toString().padLeft(2, '0');
  return '${time.year}-${two(time.month)}-${two(time.day)} ${two(time.hour)}:${two(time.minute)}';
}

/// 원본 파일 정보를 물어볼 자료(경로가 있고 웹 주소가 아닌 것).
bool libraryHasLocalSource(LibraryItem item) {
  final source = (item.source ?? '').trim();
  return source.isNotEmpty && !source.contains('://');
}

/// 열 머리글 정렬. 값이 없는 칸(크기·날짜)은 방향과 관계없이 뒤로 보낸다. 같으면 이름·id 순.
List<LibraryItem> sortLibraryExplorerItems(
  List<LibraryItem> items,
  LibraryExplorerColumn column,
  bool ascending, {
  Map<String, LibraryFileInfo> info = const {},
}) {
  int byName(LibraryItem a, LibraryItem b) {
    final c = a.title.toLowerCase().compareTo(b.title.toLowerCase());
    return c != 0 ? c : a.id.compareTo(b.id);
  }

  int nullable(Comparable<Object?>? a, Comparable<Object?>? b) {
    if (a == null && b == null) return 0;
    if (a == null) return ascending ? 1 : -1; // 방향을 뒤집어도 빈 칸은 뒤에 남게 한다.
    if (b == null) return ascending ? -1 : 1;
    return a.compareTo(b);
  }

  int compare(LibraryItem a, LibraryItem b) => switch (column) {
        LibraryExplorerColumn.name => 0,
        LibraryExplorerColumn.kind => libraryKindLabel(a).compareTo(libraryKindLabel(b)),
        LibraryExplorerColumn.size => nullable(info[a.id]?.size as Comparable<Object?>?, info[b.id]?.size as Comparable<Object?>?),
        LibraryExplorerColumn.modified => nullable(info[a.id]?.modified as Comparable<Object?>?, info[b.id]?.modified as Comparable<Object?>?),
        LibraryExplorerColumn.source => (a.source ?? '').toLowerCase().compareTo((b.source ?? '').toLowerCase()),
        LibraryExplorerColumn.space => a.space.index.compareTo(b.space.index),
      };

  final sorted = [...items];
  sorted.sort((a, b) {
    final c = compare(a, b);
    final primary = c != 0 ? c : byName(a, b);
    return ascending ? primary : -primary;
  });
  return sorted;
}

IconData libraryKindIcon(LibraryItemKind kind) => switch (kind) {
      LibraryItemKind.file => Icons.insert_drive_file_outlined,
      LibraryItemKind.folder => Icons.folder_outlined,
      LibraryItemKind.link => Icons.link,
      LibraryItemKind.text => Icons.notes,
      LibraryItemKind.richText => Icons.article_outlined,
      LibraryItemKind.image || LibraryItemKind.capture => Icons.image_outlined,
      LibraryItemKind.note => Icons.sticky_note_2_outlined,
    };

class LibraryExplorerView extends StatefulWidget {
  const LibraryExplorerView({
    super.key,
    required this.boxName,
    this.visibleSpaces = LibrarySpace.values,
    required this.items,
    required this.spaceNames,
    required this.selectedIds,
    required this.controller,
    required this.onSelectionChanged,
    required this.onFolderChanged,
    required this.onOpen,
    required this.onOpenSelection,
    required this.onRename,
    required this.onDeleteSelection,
    required this.onMoveItems,
    required this.onCopyItems,
    required this.onFileDrop,
    required this.onCopyPaths,
    this.onStyleChanged,
    this.onAddToNotebook,
    this.fileInfoLoader,
    this.highlightedItemId,
    this.busy = false,
  });

  final String boxName;
  final List<LibrarySpace> visibleSpaces;

  /// 자료함의 자료(검색 중이면 검색 결과). 영역으로 걸러 보여 준다.
  final List<LibraryItem> items;
  final Map<LibrarySpace, String> spaceNames;
  final Set<String> selectedIds;
  final String? highlightedItemId;
  final LibraryExplorerController controller;
  final ValueChanged<Set<String>> onSelectionChanged;
  final ValueChanged<LibrarySpace?> onFolderChanged;
  final ValueChanged<LibraryItem> onOpen;
  final VoidCallback onOpenSelection;
  final ValueChanged<LibraryItem> onRename;
  final VoidCallback onDeleteSelection;
  final Future<void> Function(List<LibraryItem>, LibrarySpace) onMoveItems;
  final Future<void> Function(List<LibraryItem>, LibrarySpace) onCopyItems;
  final Future<void> Function(LibraryFileDrop, LibrarySpace) onFileDrop;
  final VoidCallback onCopyPaths;
  final ValueChanged<LibraryExplorerStyle>? onStyleChanged;
  final ValueChanged<LibraryItem>? onAddToNotebook;

  /// 원본 파일 정보 조회(보이는 줄만 묻는다). 없으면 크기·날짜 칸은 비운다.
  final Future<Map<String, LibraryFileInfo>> Function(List<String> itemIds)? fileInfoLoader;
  final bool busy;

  @override
  State<LibraryExplorerView> createState() => _LibraryExplorerViewState();
}

/// 목록의 한 줄: 맨 위에서는 영역 폴더, 영역 안에서는 자료.
class _Entry {
  const _Entry.folder(this.space) : item = null;
  const _Entry.item(LibraryItem this.item) : space = null;
  final LibrarySpace? space;
  final LibraryItem? item;

  String get focusKey => item?.id ?? 'space:${space!.name}';
}

class _LibraryExplorerViewState extends State<LibraryExplorerView> {
  static const _rowHeight = 28.0;
  static const _listRowHeight = 26.0;
  static const _infoBatch = 200;

  final _listFocus = FocusNode(debugLabel: 'libraryExplorerList');
  final _scroll = ScrollController();
  final _hScroll = ScrollController();
  final _pendingInfo = <String>{};
  final _requestedInfo = <String>{};
  bool _infoScheduled = false;
  String? _lastClickKey;
  DateTime _lastClickAt = DateTime.fromMillisecondsSinceEpoch(0);
  int? _resizePointer;
  double? _resizeLastX;

  LibraryExplorerController get _c => widget.controller;

  void _resizeFromPointer(PointerEvent event, LibraryExplorerColumn column) {
    if (event is PointerDownEvent && event.buttons & kPrimaryButton != 0) {
      _resizePointer = event.pointer;
      _resizeLastX = event.position.dx;
    } else if (event.pointer == _resizePointer && event is PointerMoveEvent) {
      final previousX = _resizeLastX ?? event.position.dx;
      _resizeLastX = event.position.dx;
      final delta = event.position.dx - previousX;
      if (delta != 0) setState(() => _c.resize(column, delta));
    } else if (event.pointer == _resizePointer && (event is PointerUpEvent || event is PointerCancelEvent)) {
      _resizePointer = null;
      _resizeLastX = null;
    }
  }

  @override
  void didUpdateWidget(covariant LibraryExplorerView oldWidget) {
    super.didUpdateWidget(oldWidget);
    // 자료가 다시 불려 오면(이름 바꾸기·이동 등) 경로가 바뀌었을 수 있어 다시 물을 수 있게 한다.
    if (!identical(oldWidget.items, widget.items)) _requestedInfo.removeWhere((id) => !_c.info.containsKey(id));
  }

  @override
  void dispose() {
    _listFocus.dispose();
    _scroll.dispose();
    _hScroll.dispose();
    super.dispose();
  }

  // ── 자료·정렬 ──────────────────────────────────────────

  List<LibraryItem> get _folderItems {
    final folder = _c.folder;
    if (folder == null) return const [];
    return sortLibraryExplorerItems(
      widget.items.where((i) => i.space == folder).toList(),
      _c.sortColumn,
      _c.ascending,
      info: _c.info,
    );
  }

  List<_Entry> _entries(List<LibraryItem> items) => _c.folder == null
      ? [for (final s in widget.visibleSpaces) _Entry.folder(s)]
      : [for (final i in items) _Entry.item(i)];

  int _countIn(LibrarySpace space) => widget.items.where((i) => i.space == space).length;

  void _setSort(LibraryExplorerColumn column) {
    setState(() {
      if (_c.sortColumn == column) {
        _c.ascending = !_c.ascending;
      } else {
        _c.sortColumn = column;
        _c.ascending = true;
      }
    });
    // 크기·날짜로 정렬하려면 이 영역 자료의 정보가 모두 있어야 한다.
    if (column == LibraryExplorerColumn.size || column == LibraryExplorerColumn.modified) {
      for (final item in _folderItems) {
        _wantInfo(item);
      }
    }
  }

  // ── 파일 정보 지연 조회 ─────────────────────────────────

  /// 줄을 그릴 때 부른다. 모아 두었다가 그리기가 끝난 뒤 한 번에 묻는다.
  void _wantInfo(LibraryItem item) {
    if (widget.fileInfoLoader == null || !libraryHasLocalSource(item)) return;
    if (_c.info.containsKey(item.id) || _requestedInfo.contains(item.id)) return;
    _requestedInfo.add(item.id);
    _pendingInfo.add(item.id);
    if (_infoScheduled) return;
    _infoScheduled = true;
    scheduleMicrotask(_flushInfo);
  }

  Future<void> _flushInfo() async {
    _infoScheduled = false;
    final loader = widget.fileInfoLoader;
    if (loader == null || !mounted) return;
    while (_pendingInfo.isNotEmpty) {
      final batch = _pendingInfo.take(_infoBatch).toList();
      _pendingInfo.removeAll(batch);
      Map<String, LibraryFileInfo> result;
      try {
        result = await loader(batch);
      } catch (_) {
        result = const {};
      }
      if (!mounted) return;
      setState(() {
        for (final id in batch) {
          _c.info[id] = result[id] ?? const LibraryFileInfo();
        }
      });
    }
  }

  // ── 이동(폴더) ────────────────────────────────────────

  void _go(bool Function() move) {
    if (!move()) return;
    setState(() {});
    widget.onFolderChanged(_c.folder);
    if (_scroll.hasClients) _scroll.jumpTo(0);
  }

  void _open(LibrarySpace? space) => _go(() => _c.navigate(space));

  // ── 선택 ──────────────────────────────────────────────

  bool get _ctrl => HardwareKeyboard.instance.isControlPressed || HardwareKeyboard.instance.isMetaPressed;
  bool get _shift => HardwareKeyboard.instance.isShiftPressed;

  /// 일반 클릭=그것만, Ctrl+클릭=추가/해제, Shift+클릭=기준점부터 범위(Ctrl+Shift=범위 추가).
  void _select(String id, List<LibraryItem> ordered, {required bool ctrl, required bool shift}) {
    final ids = [for (final i in ordered) i.id];
    final next = {...widget.selectedIds};
    final anchor = _c.anchorId;
    if (shift && anchor != null && ids.contains(anchor) && ids.contains(id)) {
      final a = ids.indexOf(anchor);
      final b = ids.indexOf(id);
      if (!ctrl) next.clear();
      next.addAll(ids.sublist(math.min(a, b), math.max(a, b) + 1));
    } else if (ctrl) {
      if (!next.remove(id)) next.add(id);
      _c.anchorId = id;
    } else {
      next
        ..clear()
        ..add(id);
      _c.anchorId = id;
    }
    _c.focusId = id;
    widget.onSelectionChanged(next);
  }

  List<LibraryItem> _selectedOf(List<LibraryItem> ordered) =>
      [for (final i in widget.items) if (widget.selectedIds.contains(i.id)) i];

  bool _isSelected(LibraryItem item) => widget.selectedIds.contains(item.id) || widget.highlightedItemId == item.id;

  void _activate(_Entry entry) {
    final item = entry.item;
    if (item == null) {
      _open(entry.space);
    } else {
      widget.onOpen(item);
    }
  }

  void _pointerDown(PointerDownEvent event, _Entry entry, List<LibraryItem> ordered) {
    _rowHit = true;
    _listFocus.requestFocus();
    final item = entry.item;
    if (event.buttons == kSecondaryButton) {
      if (item != null) {
        if (!widget.selectedIds.contains(item.id)) _select(item.id, ordered, ctrl: false, shift: false);
        _showItemMenu(event.position, item);
      }
      return;
    }
    if (event.buttons != kPrimaryButton) return;
    final now = DateTime.now();
    final key = entry.focusKey;
    final ctrl = _ctrl;
    final shift = _shift;
    final isDouble = !ctrl && !shift && _lastClickKey == key && now.difference(_lastClickAt) < const Duration(milliseconds: 500);
    _lastClickKey = isDouble ? null : key;
    _lastClickAt = now;
    if (isDouble) {
      _activate(entry);
      return;
    }
    if (item == null) {
      setState(() => _c.focusId = key);
      if (widget.selectedIds.isNotEmpty) widget.onSelectionChanged(<String>{});
      return;
    }
    // 이미 고른 여러 개 중 하나를 눌러 끌기 시작할 수 있게, 일반 클릭은 끌기 전에 선택을 줄이지 않는다.
    if (!ctrl && !shift && widget.selectedIds.contains(item.id) && widget.selectedIds.length > 1) {
      setState(() => _c.focusId = item.id);
      _c.anchorId = item.id;
      return;
    }
    _select(item.id, ordered, ctrl: ctrl, shift: shift);
  }

  /// 여러 개 고른 상태에서 하나를 눌렀다 떼면(끌지 않았으면) 그것만 고른다(탐색기와 같은 동작).
  void _pointerUp(PointerUpEvent event, _Entry entry, List<LibraryItem> ordered) {
    final item = entry.item;
    if (item == null || _ctrl || _shift || _lastClickKey != item.id) return;
    if (widget.selectedIds.length > 1 && widget.selectedIds.contains(item.id)) {
      _select(item.id, ordered, ctrl: false, shift: false);
    }
  }

  // ── 키보드 ────────────────────────────────────────────

  KeyEventResult _onKey(FocusNode node, KeyEvent event, List<_Entry> entries, List<LibraryItem> ordered) {
    if (event is! KeyDownEvent && event is! KeyRepeatEvent) return KeyEventResult.ignored;
    final key = event.logicalKey;
    final alt = HardwareKeyboard.instance.isAltPressed;
    if ((alt && key == LogicalKeyboardKey.arrowLeft) || key == LogicalKeyboardKey.backspace) {
      _go(_c.back);
      return KeyEventResult.handled;
    }
    if (alt && key == LogicalKeyboardKey.arrowRight) {
      _go(_c.forward);
      return KeyEventResult.handled;
    }
    if (alt && key == LogicalKeyboardKey.arrowUp) {
      _go(_c.up);
      return KeyEventResult.handled;
    }
    if (entries.isEmpty) return KeyEventResult.ignored;
    final current = entries.indexWhere((e) => e.focusKey == _c.focusId);
    int? target;
    if (key == LogicalKeyboardKey.arrowDown) target = current < 0 ? 0 : math.min(entries.length - 1, current + 1);
    if (key == LogicalKeyboardKey.arrowUp) target = current < 0 ? 0 : math.max(0, current - 1);
    if (key == LogicalKeyboardKey.home) target = 0;
    if (key == LogicalKeyboardKey.end) target = entries.length - 1;
    if (target != null) {
      final entry = entries[target];
      final item = entry.item;
      if (item == null) {
        setState(() => _c.focusId = entry.focusKey);
      } else {
        _select(item.id, ordered, ctrl: false, shift: _shift);
      }
      // 선택이 바뀌면 화면 위쪽 선택 표시줄이 생겨 목록 높이가 달라지므로 다음 프레임에 맞춘다.
      final index = target;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) _reveal(index);
      });
      return KeyEventResult.handled;
    }
    if (key == LogicalKeyboardKey.enter || key == LogicalKeyboardKey.numpadEnter) {
      final focused = current >= 0 ? entries[current] : null;
      if (focused != null && focused.item == null) {
        _activate(focused);
      } else if (widget.selectedIds.length > 1) {
        widget.onOpenSelection();
      } else if (focused != null) {
        _activate(focused);
      }
      return KeyEventResult.handled;
    }
    if (key == LogicalKeyboardKey.f2) {
      final selected = _selectedOf(ordered);
      if (selected.length == 1) widget.onRename(selected.single);
      return KeyEventResult.handled;
    }
    return KeyEventResult.ignored; // Delete·Ctrl+A·Esc·Ctrl+V는 자료함 화면 단축키가 처리한다.
  }

  /// 키보드로 옮긴 줄이 보이도록 스크롤(줄 높이가 일정한 자세히·목록 보기).
  void _reveal(int index) {
    if (!_scroll.hasClients) return;
    final extent = _c.style == LibraryExplorerStyle.details ? _rowHeight : _listRowHeight;
    if (_c.style != LibraryExplorerStyle.details && _c.style != LibraryExplorerStyle.list) return;
    final top = index * extent;
    final position = _scroll.position;
    if (top < position.pixels) {
      _scroll.jumpTo(top);
    } else if (top + extent > position.pixels + position.viewportDimension) {
      _scroll.jumpTo(math.min(position.maxScrollExtent, top + extent - position.viewportDimension));
    }
  }

  // ── 메뉴 ──────────────────────────────────────────────

  Future<void> _showItemMenu(Offset globalPosition, LibraryItem item) async {
    final overlay = Overlay.of(context).context.findRenderObject();
    if (overlay is! RenderBox) return;
    final local = overlay.globalToLocal(globalPosition);
    final selected = widget.selectedIds.contains(item.id) ? _selectedOf(const []) : [item];
    final count = selected.length;
    final others = [for (final s in LibrarySpace.values) if (s != item.space || count > 1) s];
    final action = await showMenu<String>(
      context: context,
      position: RelativeRect.fromRect(local & const Size(1, 1), Offset.zero & overlay.size),
      items: [
        PopupMenuItem(key: const ValueKey('libraryExplorerMenuOpen'), value: 'open', child: Text(count > 1 ? '열기($count개)' : '열기')),
        if (widget.onAddToNotebook != null && count == 1)
          const PopupMenuItem(key: ValueKey('libraryExplorerMenuNotebook'), value: 'notebook', child: Text('업무자료함에 출처로 추가')),
        if (count == 1)
          const PopupMenuItem(key: ValueKey('libraryExplorerMenuRename'), value: 'rename', child: Text('이름 바꾸기(F2)')),
        const PopupMenuItem(key: ValueKey('libraryExplorerMenuCopyPath'), value: 'path', child: Text('경로 복사')),
        PopupMenuItem(key: const ValueKey('libraryExplorerMenuDelete'), value: 'delete', child: Text(count > 1 ? '삭제($count개)' : '삭제')),
        const PopupMenuDivider(),
        for (final s in others)
          PopupMenuItem(key: ValueKey('libraryExplorerMenuMove-${s.name}'), value: 'move:${s.name}', child: Text('${widget.spaceNames[s]}(으)로 이동')),
        for (final s in LibrarySpace.values)
          PopupMenuItem(key: ValueKey('libraryExplorerMenuCopy-${s.name}'), value: 'copy:${s.name}', child: Text('${widget.spaceNames[s]}(으)로 복사')),
      ],
    );
    if (action == null || !mounted) return;
    if (action == 'open') {
      count > 1 ? widget.onOpenSelection() : widget.onOpen(item);
    } else if (action == 'notebook') {
      widget.onAddToNotebook?.call(item);
    } else if (action == 'rename') {
      widget.onRename(item);
    } else if (action == 'path') {
      widget.onCopyPaths();
    } else if (action == 'delete') {
      widget.onDeleteSelection();
    } else {
      final parts = action.split(':');
      final space = LibrarySpace.values.firstWhere((s) => s.name == parts.last);
      parts.first == 'move' ? await widget.onMoveItems(selected, space) : await widget.onCopyItems(selected, space);
    }
  }

  // ── 끌어 놓기 ─────────────────────────────────────────

  /// 영역 폴더(트리·맨 위 목록)에 놓기: 자료는 이동(Ctrl을 누르고 놓으면 복사), 탐색기 파일은 추가.
  Widget _folderDropTarget(LibrarySpace space, Widget Function(bool hovering) builder) => DragTarget<Object>(
        onWillAcceptWithDetails: (d) =>
            (d.data is LibraryExplorerDrag && (d.data as LibraryExplorerDrag).items.any((i) => i.space != space || _ctrl)) ||
            d.data is LibraryFileDrop,
        onAcceptWithDetails: (d) {
          final data = d.data;
          if (data is LibraryFileDrop) {
            unawaited(widget.onFileDrop(data, space));
          } else if (data is LibraryExplorerDrag) {
            if (_ctrl) {
              unawaited(widget.onCopyItems(data.items, space));
            } else {
              final moving = data.items.where((i) => i.space != space).toList();
              if (moving.isNotEmpty) unawaited(widget.onMoveItems(moving, space));
            }
          }
        },
        builder: (context, candidates, _) => builder(candidates.isNotEmpty),
      );

  Widget _draggable(LibraryItem item, List<LibraryItem> ordered, Widget child) {
    final group = widget.selectedIds.contains(item.id) ? _selectedOf(ordered) : [item];
    return Draggable<Object>(
      data: LibraryExplorerDrag(group),
      dragAnchorStrategy: pointerDragAnchorStrategy,
      feedback: Material(
        elevation: 4,
        borderRadius: BorderRadius.circular(6),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
          child: Row(mainAxisSize: MainAxisSize.min, children: [
            Icon(libraryKindIcon(item.kind), size: 18),
            const SizedBox(width: 6),
            Text(group.length > 1 ? '${item.title} 외 ${group.length - 1}개' : item.title),
          ]),
        ),
      ),
      child: child,
    );
  }

  // ── 그리기 ────────────────────────────────────────────

  @override
  Widget build(BuildContext context) {
    final items = _folderItems;
    final entries = _entries(items);
    return Column(key: const ValueKey('libraryExplorer'), children: [
      _buildAddressBar(),
      const Divider(height: 1),
      Expanded(child: Row(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        SizedBox(width: 200, child: _buildTree()),
        const VerticalDivider(width: 1),
        Expanded(child: Focus(
          focusNode: _listFocus,
          onKeyEvent: (node, event) => _onKey(node, event, entries, items),
          child: _buildMainPane(entries, items),
        )),
      ])),
      const Divider(height: 1),
      _buildStatusBar(entries, items),
    ]);
  }

  Widget _buildAddressBar() {
    final folder = _c.folder;
    Widget crumb(String label, LibrarySpace? to, Key key) => InkWell(
          key: key,
          onTap: () => _open(to),
          child: Padding(padding: const EdgeInsets.symmetric(horizontal: 4, vertical: 4), child: Text(label)),
        );
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 4, vertical: 2),
      child: Row(children: [
        IconButton(
          key: const ValueKey('libraryExplorerBack'),
          tooltip: '뒤로(Alt+←)',
          onPressed: _c.backStack.isEmpty ? null : () => _go(_c.back),
          icon: const Icon(Icons.arrow_back),
        ),
        IconButton(
          key: const ValueKey('libraryExplorerForward'),
          tooltip: '앞으로(Alt+→)',
          onPressed: _c.forwardStack.isEmpty ? null : () => _go(_c.forward),
          icon: const Icon(Icons.arrow_forward),
        ),
        IconButton(
          key: const ValueKey('libraryExplorerUp'),
          tooltip: '위로(Alt+↑)',
          onPressed: folder == null ? null : () => _go(_c.up),
          icon: const Icon(Icons.arrow_upward),
        ),
        const SizedBox(width: 4),
        Expanded(child: Semantics(
          label: '주소 표시줄',
          child: Container(
            key: const ValueKey('libraryExplorerAddress'),
            height: 32,
            padding: const EdgeInsets.symmetric(horizontal: 6),
            decoration: BoxDecoration(border: Border.all(color: LibraryColors.of(context).borderStrong), borderRadius: BorderRadius.circular(4)),
            child: Row(children: [
              const Icon(Icons.inventory_2_outlined, size: 16),
              const SizedBox(width: 4),
              crumb(widget.boxName, null, const ValueKey('libraryExplorerCrumb-root')),
              if (folder != null) ...[
                const Icon(Icons.chevron_right, size: 16),
                crumb(widget.spaceNames[folder]!, folder, ValueKey('libraryExplorerCrumb-${folder.name}')),
              ],
            ]),
          ),
        )),
        const SizedBox(width: 4),
        PopupMenuButton<LibraryExplorerStyle>(
          key: const ValueKey('libraryExplorerStyle'),
          tooltip: '보기 방식',
          initialValue: _c.style,
          onSelected: (style) {
            setState(() => _c.style = style);
            widget.onStyleChanged?.call(style);
          },
          itemBuilder: (context) => [
            for (final s in LibraryExplorerStyle.values)
              CheckedPopupMenuItem(
                key: ValueKey('libraryExplorerStyle-${s.name}'),
                value: s,
                checked: s == _c.style,
                child: Text(libraryExplorerStyleNames[s]!),
              ),
          ],
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
            child: Row(mainAxisSize: MainAxisSize.min, children: [
              const Icon(Icons.view_list_outlined, size: 18),
              const SizedBox(width: 4),
              Text(libraryExplorerStyleNames[_c.style]!),
            ]),
          ),
        ),
      ]),
    );
  }

  Widget _buildTree() => Material(
        color: context.colors.surface,
        child: ListView(key: const ValueKey('libraryExplorerTree'), children: [
          ListTile(
            key: const ValueKey('libraryExplorerTree-root'),
            dense: true,
            selected: _c.folder == null,
            leading: const Icon(Icons.inventory_2_outlined, size: 18),
            title: Text(widget.boxName, overflow: TextOverflow.ellipsis),
            onTap: () => _open(null),
          ),
          for (final space in widget.visibleSpaces)
            _folderDropTarget(space, (hovering) => ListTile(
                  key: ValueKey('libraryExplorerTree-${space.name}'),
                  dense: true,
                  contentPadding: const EdgeInsets.only(left: 32, right: 8),
                  tileColor: hovering ? LibraryColors.of(context).hoverFill : null,
                  selected: _c.folder == space,
                  leading: Icon(_c.folder == space ? Icons.folder_open : Icons.folder, size: 18, color: context.colors.warning),
                  title: Text(widget.spaceNames[space]!, overflow: TextOverflow.ellipsis),
                  trailing: Text('${_countIn(space)}', style: TextStyle(fontSize: 12, color: LibraryColors.of(context).secondaryText)),
                  onTap: () => _open(space),
                )),
        ]),
      );

  Widget _buildMainPane(List<_Entry> entries, List<LibraryItem> ordered) {
    final folder = _c.folder;
    Widget body = switch (_c.style) {
      LibraryExplorerStyle.details => _buildDetails(entries, ordered),
      LibraryExplorerStyle.list => _buildList(entries, ordered),
      _ => _buildIcons(entries, ordered),
    };
    if (entries.isEmpty) {
      body = Stack(children: [
        body,
        Center(child: Text('이 폴더는 비어 있습니다.', style: TextStyle(color: LibraryColors.of(context).hintText))),
      ]);
    }
    // 빈 곳을 누르면 선택 해제, 탐색기에서 끌어 온 파일은 지금 폴더에 넣는다.
    final pane = Listener(
      behavior: HitTestBehavior.translucent,
      onPointerDown: (_) => _listFocus.requestFocus(),
      child: body,
    );
    if (folder == null) return pane;
    return DragTarget<Object>(
      onWillAcceptWithDetails: (d) => d.data is LibraryFileDrop,
      onAcceptWithDetails: (d) => unawaited(widget.onFileDrop(d.data as LibraryFileDrop, folder)),
      builder: (context, candidates, _) => Container(
        color: candidates.isNotEmpty ? LibraryColors.of(context).selectedFill : null,
        child: pane,
      ),
    );
  }

  /// 줄을 누른 포인터인지(줄의 Listener가 먼저 받고, 바깥 빈 곳 Listener가 나중에 받는다).
  bool _rowHit = false;

  Widget _background({required Widget child}) => Listener(
        key: const ValueKey('libraryExplorerBackground'),
        behavior: HitTestBehavior.translucent,
        onPointerDown: (event) {
          final hitRow = _rowHit;
          _rowHit = false;
          if (hitRow || event.buttons != kPrimaryButton || _ctrl || _shift) return;
          if (widget.selectedIds.isNotEmpty) widget.onSelectionChanged(<String>{});
        },
        child: child,
      );

  /// 줄·아이콘 하나에 공통 조작(선택·더블클릭 열기·오른쪽 클릭 메뉴·끌기·폴더에 놓기)을 붙인다.
  Widget _interactive(_Entry entry, List<LibraryItem> ordered, Widget Function(bool selected, bool hovering) builder) {
    final item = entry.item;
    final selected = item != null ? _isSelected(item) : false;
    Widget wrap(bool hovering) => Listener(
          onPointerDown: (e) => _pointerDown(e, entry, ordered),
          onPointerUp: (e) => _pointerUp(e, entry, ordered),
          child: builder(selected, hovering),
        );
    if (item == null) return _folderDropTarget(entry.space!, wrap);
    return _draggable(item, ordered, wrap(false));
  }

  String _rowLabel(_Entry entry) {
    final item = entry.item;
    if (item == null) return '${widget.spaceNames[entry.space]} 폴더, 항목 ${_countIn(entry.space!)}개';
    final info = _c.info[item.id];
    return [
      item.title,
      libraryKindLabel(item),
      if (info?.size != null) formatLibrarySize(info!.size),
      if (info?.modified != null) '수정한 날짜 ${formatLibraryDate(info!.modified)}',
      if (info?.missing ?? false) item.managedPath?.isNotEmpty == true ? '복사본 없음' : '원본 파일 없음',
    ].join(', ');
  }

  Widget _nameCell(_Entry entry, double iconSize) {
    final item = entry.item;
    final missing = item != null && (_c.info[item.id]?.missing ?? false);
    final selected = item != null && _isSelected(item);
    final p = LibraryColors.of(context);
    return Row(children: [
      if (item == null)
        Icon(Icons.folder, size: iconSize, color: context.colors.warning)
      else
        Icon(selected ? Icons.check_circle : libraryKindIcon(item.kind), size: iconSize, color: selected ? p.selectedBorder : (missing ? p.disabledText : null)),
      const SizedBox(width: 6),
      Flexible(child: Text(
        item?.title ?? widget.spaceNames[entry.space]!,
        overflow: TextOverflow.ellipsis,
        style: TextStyle(color: missing ? p.disabledText : null),
      )),
      if (missing) ...[
        const SizedBox(width: 4),
          Tooltip(
            message: item.managedPath?.isNotEmpty == true
                ? '자료함의 복사본을 찾을 수 없습니다.'
                : '원본 파일을 찾을 수 없습니다(옮겨졌거나 삭제되었을 수 있습니다).',
          child: Icon(Icons.warning_amber_rounded, key: ValueKey('libraryExplorerMissing-${item.id}'), size: 16, color: p.missingIcon),
        ),
      ],
    ]);
  }

  String _cellText(_Entry entry, LibraryExplorerColumn column) {
    final item = entry.item;
    if (item == null) {
      return switch (column) {
        LibraryExplorerColumn.kind => '파일 폴더',
        LibraryExplorerColumn.size => '항목 ${_countIn(entry.space!)}개',
        _ => '',
      };
    }
    final info = _c.info[item.id];
    return switch (column) {
      LibraryExplorerColumn.name => item.title,
      LibraryExplorerColumn.kind => libraryKindLabel(item),
      LibraryExplorerColumn.size => formatLibrarySize(info?.size),
      LibraryExplorerColumn.modified => formatLibraryDate(info?.modified),
      LibraryExplorerColumn.source => item.source ?? '',
      LibraryExplorerColumn.space => widget.spaceNames[item.space] ?? '',
    };
  }

  Color? _rowColor(bool selected, bool focused, bool hovering) => selected
      ? LibraryColors.of(context).rowSelectedFill
      : hovering
          ? LibraryColors.of(context).hoverFill
          : null;

  Widget _buildDetails(List<_Entry> entries, List<LibraryItem> ordered) => LayoutBuilder(builder: (context, constraints) {
        final columns = LibraryExplorerColumn.values;
        final total = columns.fold<double>(0, (w, c) => w + _c.widths[c]!) + 12;
        final width = math.max(total, constraints.maxWidth);
        return Scrollbar(
          controller: _hScroll,
          notificationPredicate: (n) => n.depth == 0,
          child: SingleChildScrollView(
            controller: _hScroll,
            scrollDirection: Axis.horizontal,
            child: SizedBox(
              width: width,
              child: Column(children: [
                _buildHeader(columns),
                const Divider(height: 1),
                Expanded(child: _background(child: Scrollbar(
                  controller: _scroll,
                  child: ListView.builder(
                    key: const ValueKey('libraryExplorerList'),
                    controller: _scroll,
                    itemExtent: _rowHeight,
                    itemCount: entries.length,
                    itemBuilder: (context, index) {
                      final entry = entries[index];
                      final item = entry.item;
                      if (item != null) _wantInfo(item);
                      final focused = _c.focusId == entry.focusKey;
                      return Semantics(
                        selected: item != null && _isSelected(item),
                        label: _rowLabel(entry),
                        child: _interactive(entry, ordered, (selected, hovering) => Container(
                          key: ValueKey(item != null ? 'libraryExplorerRow-${item.id}' : 'libraryExplorerFolder-${entry.space!.name}'),
                          decoration: BoxDecoration(
                            color: _rowColor(selected, focused, hovering),
                            border: focused && _listFocus.hasFocus ? Border.all(color: LibraryColors.of(context).focusBorder) : null,
                          ),
                          padding: const EdgeInsets.only(left: 6),
                          child: Row(children: [
                            for (final column in columns)
                              SizedBox(
                                width: _c.widths[column],
                                child: Padding(
                                  padding: const EdgeInsets.only(right: 8),
                                  child: column == LibraryExplorerColumn.name
                                      ? _nameCell(entry, 18)
                                      : Text(
                                          _cellText(entry, column),
                                          key: item == null ? null : ValueKey('libraryExplorerCell-${item.id}-${column.name}'),
                                          overflow: TextOverflow.ellipsis,
                                          textAlign: column == LibraryExplorerColumn.size ? TextAlign.right : TextAlign.left,
                                          style: TextStyle(
                                            fontSize: 13,
                                            color: item != null && (_c.info[item.id]?.missing ?? false) ? LibraryColors.of(context).disabledText : LibraryColors.of(context).bodyText,
                                          ),
                                        ),
                                ),
                              ),
                          ]),
                        )),
                      );
                    },
                  ),
                ))),
              ]),
            ),
          ),
        );
      });

  Widget _buildHeader(List<LibraryExplorerColumn> columns) => Container(
        height: 30,
        color: LibraryColors.of(context).headerFill,
        padding: const EdgeInsets.only(left: 6),
        child: Row(children: [
          for (final column in columns)
            SizedBox(
              width: _c.widths[column],
              child: Row(children: [
                Expanded(child: Semantics(
                  header: true,
                  button: true,
                  label: '${libraryExplorerColumnNames[column]} 열 머리글'
                      '${_c.sortColumn == column ? (_c.ascending ? ', 오름차순 정렬됨' : ', 내림차순 정렬됨') : ''}',
                  excludeSemantics: true,
                  child: InkWell(
                    key: ValueKey('libraryExplorerHeader-${column.name}'),
                    onTap: () => _setSort(column),
                    child: Padding(
                      padding: const EdgeInsets.symmetric(vertical: 6),
                      child: Row(children: [
                        Flexible(child: Text(libraryExplorerColumnNames[column]!, overflow: TextOverflow.ellipsis, style: const TextStyle(fontSize: 13, fontWeight: FontWeight.w600))),
                        if (_c.sortColumn == column)
                          Icon(_c.ascending ? Icons.arrow_drop_up : Icons.arrow_drop_down, size: 18),
                      ]),
                    ),
                  ),
                )),
                // 머리글 오른쪽 끝을 끌어 열 폭을 바꾼다.
                MouseRegion(
                  cursor: SystemMouseCursors.resizeColumn,
                  child: Listener(
                    key: ValueKey('libraryExplorerResize-${column.name}'),
                    behavior: HitTestBehavior.opaque,
                    onPointerDown: (event) => _resizeFromPointer(event, column),
                    onPointerMove: (event) => _resizeFromPointer(event, column),
                    onPointerUp: (event) => _resizeFromPointer(event, column),
                    onPointerCancel: (event) => _resizeFromPointer(event, column),
                    child: const SizedBox(width: 8, height: 30, child: VerticalDivider(width: 8, indent: 6, endIndent: 6)),
                  ),
                ),
              ]),
            ),
        ]),
      );

  Widget _buildList(List<_Entry> entries, List<LibraryItem> ordered) => _background(child: Scrollbar(
        controller: _scroll,
        child: ListView.builder(
          key: const ValueKey('libraryExplorerList'),
          controller: _scroll,
          itemExtent: _listRowHeight,
          itemCount: entries.length,
          itemBuilder: (context, index) {
            final entry = entries[index];
            final item = entry.item;
            if (item != null) _wantInfo(item);
            final focused = _c.focusId == entry.focusKey;
            return Semantics(
              selected: item != null && _isSelected(item),
              label: _rowLabel(entry),
              child: _interactive(entry, ordered, (selected, hovering) => Container(
                key: ValueKey(item != null ? 'libraryExplorerRow-${item.id}' : 'libraryExplorerFolder-${entry.space!.name}'),
                color: _rowColor(selected, focused, hovering),
                padding: const EdgeInsets.symmetric(horizontal: 8),
                alignment: Alignment.centerLeft,
                child: _nameCell(entry, 16),
              )),
            );
          },
        ),
      ));

  Widget _buildIcons(List<_Entry> entries, List<LibraryItem> ordered) {
    final (extent, iconSize, ratio, horizontal) = switch (_c.style) {
      LibraryExplorerStyle.largeIcons => (132.0, 56.0, 132 / 120, false),
      LibraryExplorerStyle.mediumIcons => (100.0, 36.0, 100 / 88, false),
      _ => (220.0, 16.0, 220 / 28, true),
    };
    return _background(child: Scrollbar(
      controller: _scroll,
      child: GridView.builder(
        key: const ValueKey('libraryExplorerList'),
        controller: _scroll,
        padding: const EdgeInsets.all(6),
        gridDelegate: SliverGridDelegateWithMaxCrossAxisExtent(maxCrossAxisExtent: extent, childAspectRatio: ratio, mainAxisSpacing: 4, crossAxisSpacing: 4),
        itemCount: entries.length,
        itemBuilder: (context, index) {
          final entry = entries[index];
          final item = entry.item;
          if (item != null) _wantInfo(item);
          final focused = _c.focusId == entry.focusKey;
          return Semantics(
            selected: item != null && _isSelected(item),
            label: _rowLabel(entry),
            child: _interactive(entry, ordered, (selected, hovering) {
              final missing = item != null && (_c.info[item.id]?.missing ?? false);
              final p = LibraryColors.of(context);
              final icon = Icon(
                item == null ? Icons.folder : libraryKindIcon(item.kind),
                size: iconSize,
                color: item == null ? context.colors.warning : (missing ? p.disabledText : (selected ? p.selectedBorder : null)),
              );
              return Container(
                key: ValueKey(item != null ? 'libraryExplorerRow-${item.id}' : 'libraryExplorerFolder-${entry.space!.name}'),
                decoration: BoxDecoration(
                  color: _rowColor(selected, focused, hovering),
                  borderRadius: BorderRadius.circular(4),
                  border: selected ? Border.all(color: p.rowSelectedBorder) : null,
                ),
                padding: const EdgeInsets.all(4),
                child: horizontal
                    ? _nameCell(entry, iconSize)
                    : Column(children: [
                        Stack(clipBehavior: Clip.none, children: [
                          icon,
                          if (missing)
                            Positioned(right: -4, bottom: -4, child: Icon(Icons.warning_amber_rounded, key: ValueKey('libraryExplorerMissing-${item.id}'), size: 16, color: p.missingIcon)),
                        ]),
                        const SizedBox(height: 4),
                        Expanded(child: Text(
                          item?.title ?? widget.spaceNames[entry.space]!,
                          maxLines: 2,
                          textAlign: TextAlign.center,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(fontSize: 12, color: missing ? p.disabledText : null),
                        )),
                      ]),
              );
            }),
          );
        },
      ),
    ));
  }

  Widget _buildStatusBar(List<_Entry> entries, List<LibraryItem> ordered) {
    final selected = _selectedOf(ordered);
    final knownSize = selected.fold<int>(0, (sum, i) => sum + (_c.info[i.id]?.size ?? 0));
    final text = [
      '항목 ${entries.length}개',
      if (selected.isNotEmpty) '${selected.length}개 항목 선택함${knownSize > 0 ? ' ${formatLibrarySize(knownSize)}' : ''}',
      if (widget.busy) '작업 중…',
    ].join('    ');
    return Semantics(
      liveRegion: true,
      child: Container(
        width: double.infinity,
        color: LibraryColors.of(context).headerFill,
        padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 4),
        child: Text(text, key: const ValueKey('libraryExplorerStatus'), style: const TextStyle(fontSize: 12)),
      ),
    );
  }
}
