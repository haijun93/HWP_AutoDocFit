import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../core/api/abbreviation_api.dart' show ApiCall;
import '../../core/theme/app_theme.dart';
import 'outliner_history.dart';

class OutlinerWorkspace extends StatefulWidget {
  const OutlinerWorkspace({super.key, required this.api, this.recentPagesStore});
  final ApiCall api;
  final RecentPagesStore? recentPagesStore;

  @override
  State<OutlinerWorkspace> createState() => _OutlinerWorkspaceState();
}

abstract interface class RecentPagesStore {
  Future<List<String>> load();
  Future<void> save(List<String> pages);
}

class FileRecentPagesStore implements RecentPagesStore {
  FileRecentPagesStore(this.directoryProvider);
  final Future<Directory> Function() directoryProvider;

  Future<File> _file() async => File(
    '${(await directoryProvider()).path}${Platform.pathSeparator}outliner_recent.json',
  );

  @override
  Future<List<String>> load() async {
    final decoded = jsonDecode(await (await _file()).readAsString());
    return decoded is List ? decoded.whereType<String>().toList() : [];
  }

  @override
  Future<void> save(List<String> pages) async {
    final file = await _file();
    await file.parent.create(recursive: true);
    await file.writeAsString(jsonEncode(pages));
  }
}

class _OutlinerWorkspaceState extends State<OutlinerWorkspace> {
  List<Map> _pages = [];
  List<String> _recentPages = [];
  bool _recentLoaded = false;
  List<Map> _blocks = [];
  String? _page;
  String _markdown = '';
  String? _version;
  String _message = '';
  bool _busy = false;
  String _pageQuery = '';
  final _search = TextEditingController();
  final Map<Object, TextEditingController> _editors = {};
  final Map<Object, FocusNode> _focusNodes = {};
  String _linkQuery = '';
  final Set<Object?> _selectedIds = <Object?>{};
  Object? _selectionAnchor;
  Object? _selectionCurrent;
  String _selectionClipboard = '';

  /// 문서 단위 실행 취소/다시 실행 이력(OUTLINERUNDO1). 페이지를 바꾸거나 저장이 거절되면 비운다.
  final OutlinerHistory _history = OutlinerHistory();

  List<Map> get _visibleBlocks {
    final visible = <Map>[];
    var foldedAt = -1;
    for (final block in _blocks) {
      final indent = block['indent'] as int? ?? 0;
      if (foldedAt >= 0 && indent > foldedAt) continue;
      foldedAt = -1;
      visible.add(block);
      if (block['folded'] == true) foldedAt = indent;
    }
    return visible;
  }

  @override
  void initState() {
    super.initState();
    _refreshPages();
  }

  @override
  void dispose() {
    _search.dispose();
    for (final controller in _editors.values) {
      controller.dispose();
    }
    for (final node in _focusNodes.values) {
      node.dispose();
    }
    super.dispose();
  }

  Future<Map> _call(Map payload) async {
    final raw = await widget.api('outliner', [payload]);
    final envelope = raw is Map ? raw : const {};
    if (envelope['ok'] != true) {
      final error = envelope['error'];
      throw StateError(
        error is Map ? '${error['message'] ?? '요청 실패'}' : '요청 실패',
      );
    }
    return envelope['data'] is Map ? Map.from(envelope['data'] as Map) : {};
  }

  Future<void> _refreshPages() async {
    try {
      final result = await _call({'action': 'list'});
      if (mounted) {
        final pages = (result['pages'] as List? ?? [])
            .whereType<Map>()
            .toList();
        final available = pages
            .map((p) => p['name'])
            .whereType<String>()
            .toSet();
        if (!_recentLoaded) {
          _recentPages = await _loadRecentPages(available, result['recent']);
          _recentLoaded = true;
        } else {
          _recentPages = _recentPages.where(available.contains).take(20).toList();
        }
        setState(() {
          _pages = pages;
        });
      }
      if (_page == null) await _openToday();
    } catch (e) {
      if (mounted) setState(() => _message = '$e');
    }
  }

  Future<Directory> _defaultDataDirectory() async {
    final base = Platform.environment['LOCALAPPDATA'] ??
        Platform.environment['APPDATA'];
    if (base == null || base.isEmpty) {
      throw const FileSystemException('사용자 데이터 폴더를 찾을 수 없습니다.');
    }
    return Directory('$base${Platform.pathSeparator}HwpAutoDocFit');
  }

  Future<List<String>> _loadRecentPages(Set<String> available, Object? fallback) async {
    try {
      final store = widget.recentPagesStore ?? FileRecentPagesStore(_defaultDataDirectory);
      final stored = await store.load();
      final recent = stored.where(available.contains).toSet().take(20).toList();
      if (recent.isNotEmpty) return recent;
      return (fallback as List? ?? []).whereType<String>().where(available.contains).toSet().take(20).toList();
    } catch (_) {
      return (fallback as List? ?? []).whereType<String>().where(available.contains).toSet().take(20).toList();
    }
  }

  Future<void> _saveRecentPages() async {
    try {
      final store = widget.recentPagesStore ?? FileRecentPagesStore(_defaultDataDirectory);
      await store.save(_recentPages.take(20).toList());
    } catch (_) {}
  }

  Future<void> _rememberPage(String page) async {
    _recentPages = [page, ..._recentPages.where((name) => name != page)].take(20).toList();
    await _saveRecentPages();
  }

  Future<void> _openToday() async {
    await _run(() async {
      final result = await _call({'action': 'daily'});
      _applyDocument(result);
      if (_page != null) {
        _recentPages = [
          _page!,
          ..._recentPages.where((name) => name != _page),
        ].take(20).toList();
        await _saveRecentPages();
      }
      await _refreshPages();
    });
  }

  void _applyDocument(Map doc) {
    _history.clear();
    _selectedIds.clear();
    _selectionAnchor = null;
    _selectionCurrent = null;
    _page = doc['page'] as String?;
    _markdown = doc['markdown'] as String? ?? '';
    _version = doc['version'] as String?;
    _blocks = (doc['document']?['blocks'] as List? ?? [])
        .whereType<Map>()
        .toList();
  }

  Future<void> _open(String page) async => _run(() async {
    _applyDocument(await _call({'action': 'load', 'page': page}));
    await _rememberPage(page);
    await _refreshPages();
  });

  Future<void> _newPage() async {
    final name = await _pageNameDialog('새 페이지');
    if (name == null || name.isEmpty) return;
    await _run(() async {
      _applyDocument(await _call({'action': 'create', 'page': name}));
      await _refreshPages();
    });
  }

  Future<String?> _pageNameDialog(String title, [String initial = '']) async {
    final controller = TextEditingController(text: initial);
    final value = await showDialog<String>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(title),
        content: TextField(
          controller: controller,
          autofocus: true,
          decoration: const InputDecoration(labelText: '페이지 이름'),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('취소'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, controller.text.trim()),
            child: const Text('확인'),
          ),
        ],
      ),
    );
    controller.dispose();
    return value;
  }

  Future<void> _renamePage() async {
    if (_page == null) return;
    final name = await _pageNameDialog('페이지 이름 바꾸기', _page!);
    if (name == null || name.isEmpty || name == _page) return;
    await _run(() async {
      _applyDocument(
        await _call({'action': 'rename', 'page': _page, 'new_name': name}),
      );
      await _refreshPages();
    });
  }

  Future<void> _deletePage() async {
    if (_page == null) return;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('페이지 삭제'),
        content: Text('“$_page” 페이지를 삭제할까요?'),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: const Text('취소'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, true),
            child: const Text('삭제'),
          ),
        ],
      ),
    );
    if (confirmed != true) return;
    await _run(() async {
      await _call({'action': 'delete', 'page': _page});
      _page = null;
      _markdown = '';
      _blocks = [];
      _version = null;
      await _refreshPages();
      if (_pages.isNotEmpty) {
        _applyDocument(
          await _call({'action': 'load', 'page': _pages.first['name']}),
        );
      } else {
        await _openToday();
      }
    });
  }

  Future<void> _run(Future<void> Function() action) async {
    setState(() {
      _busy = true;
      _message = '';
    });
    try {
      await action();
      if (mounted) setState(() {});
    } catch (e) {
      if (mounted) setState(() => _message = '$e');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _edit(Map block, String operation, [String? text]) async {
    if (_page == null || _busy) return;
    await _run(() async {
      final before = _markdown;
      final result = await _call({
        'action': 'edit',
        'operation': operation,
        'page': _page,
        'markdown': _markdown,
        'block_id': block['id'],
        'text': ?text,
      });
      _markdown = result['markdown'] as String? ?? _markdown;
      _blocks = (result['document']?['blocks'] as List? ?? [])
          .whereType<Map>()
          .toList();
      await _saveRecording(before);
      if (_page != null) await _rememberPage(_page!);
    });
  }

  Future<void> _editSelection(String operation, [String? text]) async {
    if (_page == null || _busy || _selectedIds.isEmpty) return;
    final ids = _blocks.map((b) => b['id']).where(_selectedIds.contains).toList();
    await _run(() async {
      final before = _markdown;
      final result = await _call({
        'action': 'edit', 'operation': operation, 'page': _page,
        'markdown': _markdown, 'block_ids': ids, 'text': ?text,
      });
      _markdown = result['markdown'] as String? ?? _markdown;
      _blocks = (result['document']?['blocks'] as List? ?? []).whereType<Map>().toList();
      await _saveRecording(before);
      _selectedIds.removeWhere((id) => !_blocks.any((b) => b['id'] == id));
    });
  }

  void _extendSelection(Object? id) {
    final visible = _visibleBlocks;
    final index = visible.indexWhere((b) => b['id'] == id);
    if (index < 0) return;
    final anchor = _selectionAnchor ?? id;
    final anchorIndex = visible.indexWhere((b) => b['id'] == anchor);
    final base = anchorIndex < 0 ? index : anchorIndex;
    final lo = base < index ? base : index;
    final hi = base > index ? base : index;
    setState(() {
      _selectedIds
        ..clear()
        ..addAll(visible.sublist(lo, hi + 1).map((b) => b['id']));
      _selectionAnchor ??= id;
      _selectionCurrent = id;
    });
  }

  Future<void> _copySelection({required bool cut}) async {
    final chosen = _blocks.where((b) => _selectedIds.contains(b['id'])).toList();
    if (chosen.isEmpty) return;
    final value = chosen.map((b) {
      final status = b['status'] == null ? '' : '${b['status']} ';
      return '${' ' * (b['indent'] as int? ?? 0)}- $status${b['text'] ?? ''}';
    }).join('\n');
    _selectionClipboard = value;
    await Clipboard.setData(ClipboardData(text: value));
    if (cut) await _editSelection('delete');
  }

  Future<void> _pasteSelection() async {
    if (_selectedIds.isEmpty) return;
    final data = await Clipboard.getData(Clipboard.kTextPlain);
    final value = data?.text ?? _selectionClipboard;
    if (value.isNotEmpty) await _editSelection('paste', value);
  }

  /// 줄 추가 뒤 입력 위치를 새 블록으로 옮긴다. 새로 생긴 블록(없으면 바로 다음 블록)을 찾는다.
  Object? _newBlockIdAfter(Map block, Set<Object?> beforeIds) {
    for (final b in _blocks) {
      if (!beforeIds.contains(b['id'])) return b['id'];
    }
    final index = _blocks.indexWhere((b) => b['id'] == block['id']);
    if (index >= 0 && index + 1 < _blocks.length) return _blocks[index + 1]['id'];
    return null;
  }

  void _focusBlockAfterBuild(Object? id) {
    if (id == null) return;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      final node = _focusNodes[id];
      if (node != null && node.canRequestFocus) {
        node.requestFocus();
        final controller = _editors[id];
        if (controller != null) {
          controller.selection =
              TextSelection.collapsed(offset: controller.text.length);
        }
      }
    });
  }

  Future<void> _commitAndInsert(Map block, String text) async {
    if (_page == null || _busy) return;
    final beforeIds = _blocks.map((b) => b['id']).toSet();
    Object? focusId;
    await _run(() async {
      final before = _markdown;
      var result = await _call({
        'action': 'edit',
        'operation': 'set_text',
        'page': _page,
        'markdown': _markdown,
        'block_id': block['id'],
        'text': text,
      });
      result = await _call({
        'action': 'edit',
        'operation': 'insert',
        'page': _page,
        'markdown': result['markdown'],
        'block_id': block['id'],
        'text': '',
      });
      _markdown = result['markdown'] as String? ?? _markdown;
      _blocks = (result['document']?['blocks'] as List? ?? [])
          .whereType<Map>()
          .toList();
      focusId = _newBlockIdAfter(block, beforeIds);
      await _saveRecording(before);
      if (_page != null) await _rememberPage(_page!);
    });
    _focusBlockAfterBuild(focusId);
  }

  /// 저장하고, 성공하면 이번 편집 묶음(직전 마크다운 [before])을 한 단계로 기록한다.
  /// 저장이 거절되면(충돌 등 외부 변경) 이력의 기준이 어긋났으므로 비운다.
  Future<void> _saveRecording(String before) async {
    try {
      await _save();
    } catch (_) {
      _history.clear();
      rethrow;
    }
    if (_markdown != before) _history.record(before);
  }

  /// 입력 칸에 아직 확정(Enter·Tab 등)하지 않은 글이 있는 블록. 있으면 칸 자체의 글자 되돌리기가 먼저다.
  Map? _pendingBlock() {
    for (final block in _blocks) {
      final controller = _editors[block['id']];
      final focus = _focusNodes[block['id']];
      // 포커스가 없는 칸은 다음 그리기에서 문서 글로 맞춰지므로 '미확정 입력'이 아니다.
      if (controller != null &&
          focus != null &&
          focus.hasFocus &&
          controller.text != '${block['text'] ?? ''}') {
        return block;
      }
    }
    return null;
  }

  void _syncEditorsToBlocks() {
    for (final block in _blocks) {
      final controller = _editors[block['id']];
      final text = '${block['text'] ?? ''}';
      if (controller != null && controller.text != text) {
        controller.value = TextEditingValue(
          text: text,
          selection: TextSelection.collapsed(offset: text.length),
        );
      }
    }
  }

  /// 문서 단위 되돌리기/다시 실행. 확정하지 않은 입력이 있으면 그 입력만 되돌리고(칸을 확정 상태로 복원) 문서는 건드리지 않는다.
  Future<void> _stepHistory({required bool undo}) async {
    if (_page == null || _busy) return;
    if (_pendingBlock() != null) {
      if (undo) {
        _syncEditorsToBlocks();
        setState(() {});
      }
      return;
    }
    if (undo ? !_history.canUndo : !_history.canRedo) return;
    await _run(() async {
      try {
        final target = undo ? _history.undo(_markdown) : _history.redo(_markdown);
        if (target == null) return;
        final parsed = await _call({
          'action': 'parse',
          'page': _page,
          'markdown': target,
        });
        _markdown = target;
        _blocks = (parsed['blocks'] as List? ?? []).whereType<Map>().toList();
        _syncEditorsToBlocks();
        await _save();
      } catch (_) {
        _history.clear();
        rethrow;
      }
    });
  }

  Future<void> _save() async {
    if (_page == null || _version == null) return;
    final result = await _call({
      'action': 'save',
      'page': _page,
      'markdown': _markdown,
      'version': _version,
    });
    _version = result['version'] as String?;
  }

  Future<void> _setTextAndEdit(
    Map block,
    String operation,
    String value,
  ) async {
    if (_page == null || _busy) return;
    await _run(() async {
      final before = _markdown;
      var result = await _call({
        'action': 'edit',
        'operation': 'set_text',
        'page': _page,
        'markdown': _markdown,
        'block_id': block['id'],
        'text': value,
      });
      if (operation.isNotEmpty) {
        result = await _call({
          'action': 'edit',
          'operation': operation,
          'page': _page,
          'markdown': result['markdown'],
          'block_id': block['id'],
        });
      }
      _markdown = result['markdown'] as String? ?? _markdown;
      _blocks = (result['document']?['blocks'] as List? ?? [])
          .whereType<Map>()
          .toList();
      await _saveRecording(before);
      if (_page != null) await _rememberPage(_page!);
    });
  }

  Future<void> _searchBlocks(String query) async {
    if (query.trim().runes.length < 2 || _page == null) return;
    await _run(() async {
      final result = await _call({
        'action': 'search',
        'page': _page,
        'markdown': _markdown,
        'query': query,
      });
      final ids = (result['block_ids'] as List? ?? []).toSet();
      setState(
        () => _blocks = _blocks.where((b) => ids.contains(b['id'])).toList(),
      );
    });
  }

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return Row(
      children: [
        SizedBox(
          width: 248,
          child: Material(
            color: c.surface,
            child: Column(
              children: [
                Padding(
                  padding: const EdgeInsets.all(12),
                  child: TextField(
                    controller: _search,
                    decoration: const InputDecoration(
                      prefixIcon: Icon(Icons.search),
                      hintText: '페이지·블록 검색',
                      border: OutlineInputBorder(),
                    ),
                    onChanged: (value) =>
                        setState(() => _pageQuery = value.trim().toLowerCase()),
                    onSubmitted: _searchBlocks,
                  ),
                ),
                ListTile(
                  leading: const Icon(Icons.today_outlined),
                  title: const Text('오늘의 일지'),
                  onTap: _busy ? null : _openToday,
                ),
                Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 12),
                  child: Row(
                    children: [
                      Expanded(
                        child: Text(
                          '최근 페이지',
                          style: Theme.of(
                            context,
                          ).textTheme.labelLarge?.copyWith(color: c.muted),
                        ),
                      ),
                      IconButton(
                        tooltip: '새 페이지',
                        onPressed: _busy ? null : _newPage,
                        icon: const Icon(Icons.add),
                      ),
                    ],
                  ),
                ),
                for (final name in _recentPages)
                  ListTile(
                    dense: true,
                    leading: const Icon(Icons.history, size: 18),
                    title: Text(name),
                    selected: name == _page,
                    onTap: _busy ? null : () => _open(name),
                  ),
                const Divider(height: 1),
                Expanded(
                  child: ListView(
                    children: [
                      for (final p in _pages.where(
                        (p) =>
                            '${p['name']}'.toLowerCase().contains(_pageQuery),
                      ))
                        ListTile(
                          selected: p['name'] == _page,
                          leading: const Icon(Icons.description_outlined),
                          title: Text('${p['name']}'),
                          onTap: _busy ? null : () => _open('${p['name']}'),
                        ),
                    ],
                  ),
                ),
              ],
            ),
          ),
        ),
        VerticalDivider(width: 1, color: c.line),
        Expanded(
          // 입력 칸에 포커스가 없을 때(단추 등)의 Ctrl+Z/Ctrl+Y. 칸에 포커스가 있으면 칸의 처리가 먼저다.
          child: Focus(
            canRequestFocus: false,
            skipTraversal: true,
            onKeyEvent: (node, event) {
              if (event is! KeyDownEvent) return KeyEventResult.ignored;
              final mods = HardwareKeyboard.instance;
              final key = event.logicalKey;
              if (!(mods.isControlPressed || mods.isMetaPressed) ||
                  mods.isAltPressed ||
                  (key != LogicalKeyboardKey.keyZ &&
                      key != LogicalKeyboardKey.keyY)) {
                return KeyEventResult.ignored;
              }
              if (_focusNodes.values.any((n) => n.hasFocus)) {
                return KeyEventResult.ignored;
              }
              _stepHistory(
                undo: key == LogicalKeyboardKey.keyZ && !mods.isShiftPressed,
              );
              return KeyEventResult.handled;
            },
            child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Padding(
                padding: const EdgeInsets.fromLTRB(24, 16, 24, 10),
                child: Row(
                  children: [
                    Expanded(
                      child: Text(
                        _page ?? '아웃라이너',
                        style: Theme.of(context).textTheme.headlineSmall
                            ?.copyWith(
                              color: c.ink,
                              fontWeight: FontWeight.w700,
                            ),
                      ),
                    ),
                    const Tooltip(
                      message: '블록 선택 Alt+↑/↓ · 범위 Shift+↑/↓ · 해제 Esc · 이동 Alt+Shift+↑/↓ · 복사/잘라내기/붙여넣기 Ctrl+C/X/V',
                      child: Icon(Icons.keyboard_outlined, size: 18),
                    ),
                    // 단추가 포커스를 가져가면 입력 중이던 칸이 확정 전 글을 잃으므로 포커스를 받지 않게 한다.
                    ExcludeFocus(
                      child: IconButton(
                      key: const ValueKey('outliner-undo'),
                      tooltip: '실행 취소 (Ctrl+Z)',
                      onPressed:
                          _page == null ||
                              _busy ||
                              !(_history.canUndo || _pendingBlock() != null)
                          ? null
                          : () => _stepHistory(undo: true),
                      icon: const Icon(Icons.undo),
                    ),
                    ),
                    ExcludeFocus(
                      child: IconButton(
                      key: const ValueKey('outliner-redo'),
                      tooltip: '다시 실행 (Ctrl+Y)',
                      onPressed:
                          _page == null ||
                              _busy ||
                              !_history.canRedo ||
                              _pendingBlock() != null
                          ? null
                          : () => _stepHistory(undo: false),
                      icon: const Icon(Icons.redo),
                    ),
                    ),
                    IconButton(
                      tooltip: '이름 바꾸기',
                      onPressed: _page == null || _busy ? null : _renamePage,
                      icon: const Icon(Icons.drive_file_rename_outline),
                    ),
                    IconButton(
                      tooltip: '페이지 삭제',
                      onPressed: _page == null || _busy ? null : _deletePage,
                      icon: const Icon(Icons.delete_outline),
                    ),
                    IconButton(
                      tooltip: '새 블록',
                      onPressed: _blocks.isEmpty || _busy
                          ? null
                          : () => _edit(_blocks.last, 'insert', ''),
                      icon: const Icon(Icons.add),
                    ),
                  ],
                ),
              ),
              if (_selectedIds.isNotEmpty)
                Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 24),
                  child: Wrap(
                    crossAxisAlignment: WrapCrossAlignment.center,
                    children: [
                      Text('${_selectedIds.length}개 선택'),
                      IconButton(tooltip: '선택 블록 삭제 (Delete)', onPressed: _busy ? null : () => _editSelection('delete'), icon: const Icon(Icons.delete_outline)),
                      IconButton(tooltip: '선택 블록 들여쓰기 (Tab)', onPressed: _busy ? null : () => _editSelection('indent'), icon: const Icon(Icons.format_indent_increase)),
                      IconButton(tooltip: '선택 블록 내어쓰기 (Shift+Tab)', onPressed: _busy ? null : () => _editSelection('outdent'), icon: const Icon(Icons.format_indent_decrease)),
                      IconButton(tooltip: '선택 블록 접기', onPressed: _busy ? null : () => _editSelection('fold'), icon: const Icon(Icons.unfold_less)),
                    ],
                  ),
                ),
              if (_message.isNotEmpty)
                Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 24),
                  child: Text(
                    _message,
                    style: TextStyle(
                      color: Theme.of(context).colorScheme.error,
                    ),
                  ),
                ),
              Expanded(
                child: ListView(
                  padding: const EdgeInsets.fromLTRB(24, 8, 24, 24),
                  children: [
                    for (final block in _visibleBlocks)
                      Padding(
                        padding: EdgeInsets.only(
                          left: ((block['indent'] as int? ?? 0) * 24)
                              .toDouble(),
                        ),
                        child: Container(
                          decoration: BoxDecoration(
                            color: _selectedIds.contains(block['id'])
                                ? Theme.of(context).colorScheme.primaryContainer
                                : null,
                            borderRadius: BorderRadius.circular(6),
                          ),
                          child: Row(
                          children: [
                            IconButton(
                              tooltip: '접기/펼치기',
                              visualDensity: VisualDensity.compact,
                              onPressed: _busy
                                  ? null
                                  : () => _edit(
                                      block,
                                      block['folded'] == true
                                          ? 'unfold'
                                          : 'fold',
                                    ),
                              icon: Icon(
                                block['folded'] == true
                                    ? Icons.chevron_right
                                    : Icons.expand_more,
                              ),
                            ),
                            Expanded(
                              child: Builder(
                                builder: (context) {
                                  final id = block['id'];
                                  final controller = _editors.putIfAbsent(
                                    id,
                                    () => TextEditingController(
                                      text: '${block['text'] ?? ''}',
                                    ),
                                  );
                                  final focus = _focusNodes.putIfAbsent(
                                    id,
                                    FocusNode.new,
                                  );
                                  if (!focus.hasFocus &&
                                      controller.text !=
                                          '${block['text'] ?? ''}') {
                                    controller.text = '${block['text'] ?? ''}';
                                  }
                                  return Focus(
                                    onKeyEvent: (node, event) {
                                      if (event is! KeyDownEvent) {
                                        return KeyEventResult.ignored;
                                      }
                                      if (_busy) return KeyEventResult.handled;
                                      final key = event.logicalKey;
                                      final value = controller.text;
                                      final selection = controller.selection;
                                      final mods = HardwareKeyboard.instance;
                                      final composingNow = controller.value.composing;
                                      final composing = composingNow.isValid && !composingNow.isCollapsed;
                                      final selectionSafe = !composing && selection.isCollapsed && value == '${block['text'] ?? ''}';
                                      if (selectionSafe && mods.isAltPressed && !mods.isControlPressed &&
                                          (key == LogicalKeyboardKey.arrowUp || key == LogicalKeyboardKey.arrowDown)) {
                                        final visible = _visibleBlocks;
                                        final at = visible.indexWhere((b) => b['id'] == (_selectionCurrent ?? id));
                                        if (at >= 0) {
                                          final next = (at + (key == LogicalKeyboardKey.arrowUp ? -1 : 1)).clamp(0, visible.length - 1);
                                          if (mods.isShiftPressed) {
                                            _selectionAnchor ??= id;
                                            _editSelection(key == LogicalKeyboardKey.arrowUp ? 'up' : 'down');
                                            _selectionCurrent = visible[next]['id'];
                                          } else {
                                            _selectionAnchor ??= id;
                                            _extendSelection(visible[next]['id']);
                                          }
                                        }
                                        return KeyEventResult.handled;
                                      }
                                      if (selectionSafe && mods.isShiftPressed && !mods.isAltPressed && !mods.isControlPressed &&
                                          (key == LogicalKeyboardKey.arrowUp || key == LogicalKeyboardKey.arrowDown)) {
                                        final visible = _visibleBlocks;
                                        final at = visible.indexWhere((b) => b['id'] == (_selectionCurrent ?? id));
                                        if (at >= 0) {
                                          _selectionAnchor ??= id;
                                          final next = (at + (key == LogicalKeyboardKey.arrowUp ? -1 : 1)).clamp(0, visible.length - 1);
                                          _extendSelection(visible[next]['id']);
                                        }
                                        return KeyEventResult.handled;
                                      }
                                      if (selectionSafe && key == LogicalKeyboardKey.escape && _selectedIds.isNotEmpty) {
                                        setState(() { _selectedIds.clear(); _selectionAnchor = null; _selectionCurrent = null; });
                                        return KeyEventResult.handled;
                                      }
                                      if (selectionSafe && (mods.isControlPressed || mods.isMetaPressed) &&
                                          (key == LogicalKeyboardKey.keyC || key == LogicalKeyboardKey.keyX || key == LogicalKeyboardKey.keyV) &&
                                          _selectedIds.isNotEmpty) {
                                        if (key == LogicalKeyboardKey.keyV) { _pasteSelection(); }
                                        else { _copySelection(cut: key == LogicalKeyboardKey.keyX); }
                                        return KeyEventResult.handled;
                                      }
                                      if (selectionSafe && _selectedIds.isNotEmpty &&
                                          (key == LogicalKeyboardKey.delete || key == LogicalKeyboardKey.backspace)) {
                                        _editSelection('delete');
                                        return KeyEventResult.handled;
                                      }
                                      // Ctrl+Z / Ctrl+Y / Ctrl+Shift+Z: 확정하지 않은 글이 있거나 한글 조합 중이면
                                      // 칸 자체의 글자 되돌리기에 맡기고(ignored), 없으면 문서 단위 이력을 쓴다.
                                      if ((mods.isControlPressed || mods.isMetaPressed) &&
                                          !mods.isAltPressed &&
                                          (key == LogicalKeyboardKey.keyZ ||
                                              key == LogicalKeyboardKey.keyY)) {
                                        final composing = controller.value.composing;
                                        final pending =
                                            value != '${block['text'] ?? ''}';
                                        if (pending ||
                                            (composing.isValid &&
                                                !composing.isCollapsed)) {
                                          return KeyEventResult.ignored;
                                        }
                                        final redo =
                                            key == LogicalKeyboardKey.keyY ||
                                            mods.isShiftPressed;
                                        _stepHistory(undo: !redo);
                                        return KeyEventResult.handled;
                                      }
                                      if (key == LogicalKeyboardKey.tab) {
                                        if (_selectedIds.isNotEmpty && selectionSafe) {
                                          _editSelection(mods.isShiftPressed ? 'outdent' : 'indent');
                                        } else {
                                          _setTextAndEdit(
                                            block,
                                            mods.isShiftPressed ? 'outdent' : 'indent',
                                            value,
                                          );
                                        }
                                        return KeyEventResult.handled;
                                      }
                                      if (key == LogicalKeyboardKey.enter &&
                                          !mods.isControlPressed &&
                                          !mods.isAltPressed &&
                                          !mods.isShiftPressed) {
                                        _commitAndInsert(block, value);
                                        return KeyEventResult.handled;
                                      }
                                      if (mods.isControlPressed &&
                                          key == LogicalKeyboardKey.enter) {
                                        _edit(
                                          block,
                                          block['folded'] == true
                                              ? 'unfold'
                                              : 'fold',
                                        );
                                        return KeyEventResult.handled;
                                      }
                                      if (key == LogicalKeyboardKey.backspace &&
                                          selection.isCollapsed) {
                                        if (value.isEmpty) {
                                          _setTextAndEdit(
                                            block,
                                            'delete',
                                            value,
                                          );
                                          return KeyEventResult.handled;
                                        }
                                        if (selection.baseOffset == 0) {
                                          final index = _blocks.indexOf(block);
                                          if (index > 0) {
                                            final previous = _blocks[index - 1];
                                            final before =
                                                _editors[previous['id']]
                                                    ?.text ??
                                                '${previous['text'] ?? ''}';
                                            _run(() async {
                                              final mergeBefore = _markdown;
                                              final result = await _call({
                                                'action': 'edit',
                                                'operation': 'set_text',
                                                'page': _page,
                                                'markdown': _markdown,
                                                'block_id': previous['id'],
                                                'text': '$before$value',
                                              });
                                              final merged = await _call({
                                                'action': 'edit',
                                                'operation': 'delete',
                                                'page': _page,
                                                'markdown': result['markdown'],
                                                'block_id': id,
                                              });
                                              _markdown = merged['markdown'];
                                              _blocks =
                                                  (merged['document']['blocks']
                                                          as List)
                                                      .whereType<Map>()
                                                      .toList();
                                              await _saveRecording(mergeBefore);
                                              if (_page != null) await _rememberPage(_page!);
                                            });
                                          }
                                          return KeyEventResult.handled;
                                        }
                                      }
                                      return KeyEventResult.ignored;
                                    },
                                    child: TextField(
                                      key: ValueKey('outliner-block-$id'),
                                      controller: controller,
                                      focusNode: focus,
                                      onEditingComplete: () {},
                                      onChanged: (value) {
                                        final match = RegExp(
                                          r'\[\[([^\]]*)$',
                                        ).firstMatch(value);
                                        setState(
                                          () => _linkQuery =
                                              match?.group(1) ?? '',
                                        );
                                      },
                                      decoration: const InputDecoration(
                                        border: InputBorder.none,
                                        isDense: true,
                                      ),
                                      onSubmitted: null,
                                    ),
                                  );
                                },
                              ),
                            ),
                            IconButton(
                              tooltip: block['status'] == 'DONE'
                                  ? '완료 해제'
                                  : '완료 표시',
                              onPressed: _busy
                                  ? null
                                  : () => _setTextAndEdit(
                                      block,
                                      'toggle_status',
                                      _editors[block['id']]?.text ??
                                          '${block['text'] ?? ''}',
                                    ),
                              icon: Icon(
                                block['status'] == 'DONE'
                                    ? Icons.check_box
                                    : block['status'] == 'TODO'
                                    ? Icons.check_box_outline_blank
                                    : Icons.task_alt,
                                size: 18,
                              ),
                            ),
                            IconButton(
                              tooltip: '들여쓰기',
                              onPressed: _busy
                                  ? null
                                  : () => _edit(block, 'indent'),
                              icon: const Icon(Icons.keyboard_tab, size: 18),
                            ),
                            IconButton(
                              tooltip: '내어쓰기',
                              onPressed: _busy
                                  ? null
                                  : () => _edit(block, 'outdent'),
                              icon: const Icon(
                                Icons.format_indent_decrease,
                                size: 18,
                              ),
                            ),
                          ],
                          ),
                        ),
                      ),
                    if (_linkQuery.isNotEmpty) ...[
                      for (final p
                          in _pages
                              .where(
                                (p) => '${p['name']}'.toLowerCase().contains(
                                  _linkQuery.toLowerCase(),
                                ),
                              )
                              .take(6))
                        ListTile(
                          dense: true,
                          leading: const Icon(Icons.link),
                          title: Text('${p['name']}'),
                          onTap: () {
                            final focused = _focusNodes.entries
                                .where((e) => e.value.hasFocus)
                                .firstOrNull;
                            if (focused != null) {
                              final controller = _editors[focused.key]!;
                              final start = controller.text.lastIndexOf('[[');
                              if (start >= 0) {
                                controller.text =
                                    '${controller.text.substring(0, start)}[[${p['name']}]]';
                                controller.selection = TextSelection.collapsed(
                                  offset: controller.text.length,
                                );
                              }
                            }
                            setState(() => _linkQuery = '');
                          },
                        ),
                    ],
                    for (final block in _blocks)
                      if ((block['page_links'] as List? ?? []).isNotEmpty)
                        Padding(
                          padding: EdgeInsets.only(
                            left: ((block['indent'] as int? ?? 0) * 24 + 42)
                                .toDouble(),
                          ),
                          child: Wrap(
                            spacing: 6,
                            children: [
                              for (final link in block['page_links'] as List)
                                ActionChip(
                                  label: Text('[[$link]]'),
                                  onPressed: _busy
                                      ? null
                                      : () => _open('$link'),
                                ),
                            ],
                          ),
                        ),
                    if (_blocks.isEmpty)
                      Text(
                        '블록이 없습니다. +를 눌러 첫 블록을 추가하세요.',
                        style: TextStyle(color: c.muted),
                      ),
                  ],
                ),
              ),
            ],
          ),
          ),
        ),
        SizedBox(
          width: 220,
          child: _BacklinksPanel(
            api: widget.api,
            page: _page,
            markdown: _markdown,
            onOpen: (name) => _open(name),
          ),
        ),
      ],
    );
  }
}

class _BacklinksPanel extends StatefulWidget {
  const _BacklinksPanel({
    required this.api,
    required this.page,
    required this.markdown,
    required this.onOpen,
  });
  final ApiCall api;
  final String? page;
  final String markdown;
  final ValueChanged<String> onOpen;
  @override
  State<_BacklinksPanel> createState() => _BacklinksPanelState();
}

class _BacklinksPanelState extends State<_BacklinksPanel> {
  List<dynamic> _ids = [];
  @override
  void didUpdateWidget(covariant _BacklinksPanel oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.page != widget.page ||
        oldWidget.markdown != widget.markdown) {
      _load();
    }
  }

  Future<void> _load() async {
    if (widget.page == null) return;
    try {
      final raw = await widget.api('outliner', [
        {
          'action': 'backlinks',
          'page': widget.page,
          'markdown': widget.markdown,
          'target': widget.page,
        },
      ]);
      final data = raw is Map ? raw['data'] : null;
      if (mounted) {
        setState(
          () => _ids = data is Map ? (data['block_ids'] as List? ?? []) : [],
        );
      }
    } catch (_) {}
  }

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.all(12),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text('역링크', style: Theme.of(context).textTheme.titleMedium),
        const SizedBox(height: 8),
        if (_ids.isEmpty)
          const Text('참조가 없습니다.')
        else
          for (final id in _ids) Text('블록 $id'),
      ],
    ),
  );
}
