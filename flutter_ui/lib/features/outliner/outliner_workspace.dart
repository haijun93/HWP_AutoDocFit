import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../core/api/abbreviation_api.dart' show ApiCall;
import '../../core/theme/app_theme.dart';
import 'outliner_history.dart';
import 'outliner_typing.dart';

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
  /// 열려 있는 자동완성: 여는 괄호(`[[`·`((`·`(((`, 없으면 빈 글)·질의·고른 후보.
  String _popupOpen = '';
  String _popupQuery = '';
  int _popupIndex = 0;
  Object? _popupBlock;
  final Set<Object?> _selectedIds = <Object?>{};
  Object? _selectionAnchor;
  Object? _selectionCurrent;
  String _selectionClipboard = '';
  final _pairFormatter = OutlinerPairFormatter();

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
    _closePopup();
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
      final res = await _call({'action': 'rename', 'page': _page, 'new_name': name});
      _applyDocument(res);
      await _refreshPages();
      if (mounted) {
        final count = res['updated_links'] ?? 0;
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text('페이지 이름을 "$name"(으)로 변경했습니다. (링크 $count개 갱신)'),
            action: SnackBarAction(
              label: '실행 취소',
              onPressed: () => _undoRename(),
            ),
          ),
        );
      }
    });
  }

  Future<void> _undoRename() async {
    await _run(() async {
      final res = await _call({'action': 'undo_rename'});
      _applyDocument(res);
      await _refreshPages();
    });
  }

  Future<void> _deletePage() async {
    if (_page == null) return;
    List<dynamic> referring = [];
    try {
      final check = await _call({'action': 'check_delete', 'page': _page});
      referring = check['referring_pages'] as List? ?? [];
    } catch (_) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(content: Text('페이지 참조를 확인하지 못해 삭제를 취소했습니다.')),
        );
      }
      return;
    }
    if (!mounted) return;

    final contentWidget = referring.isEmpty
        ? Text('“$_page” 페이지를 삭제할까요?')
        : Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text('“$_page” 페이지를 삭제할까요?'),
              const SizedBox(height: 8),
              Text(
                '⚠ 이 페이지를 참조하는 페이지가 있습니다:\n${referring.map((p) => '• $p').join('\n')}',
                style: TextStyle(
                  color: Theme.of(context).colorScheme.error,
                  fontSize: 13,
                ),
              ),
            ],
          );

    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('페이지 삭제'),
        content: contentWidget,
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
    final deletedPage = _page;
    await _run(() async {
      await _call({'action': 'delete', 'page': _page, 'force': true});
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
      if (mounted && deletedPage != null) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text('"$deletedPage" 페이지를 삭제했습니다.'),
            action: SnackBarAction(
              label: '실행 취소(복원)',
              onPressed: () => _restorePage(),
            ),
          ),
        );
      }
    });
  }

  Future<void> _restorePage() async {
    await _run(() async {
      final res = await _call({'action': 'restore'});
      _applyDocument(res);
      await _refreshPages();
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

  // ---------- core 키 규칙(OUTLINEKEY1) ----------

  List<int> _indicesWhere(bool Function(Map) test) => [
    for (final (i, b) in _blocks.indexed)
      if (test(b)) i,
  ];

  /// 편집(또는 선택) 명령 하나를 엔진 규칙에 보내고 결과·커서·선택을 반영한다.
  Future<void> _key(
    String operation, {
    Map? block,
    String? paste,
    bool selection = false,
  }) async {
    if (_page == null || _busy) return;
    final controller = block == null ? null : _editors[block['id']];
    final index = block == null ? null : _blocks.indexOf(block);
    final selected = selection
        ? _indicesWhere((b) => _selectedIds.contains(b['id']))
        : const <int>[];
    if (selection && selected.isEmpty) return;
    final text = controller?.text;
    var start = 0;
    var end = 0;
    if (controller != null) {
      final s = controller.selection;
      start = s.isValid ? s.start : controller.text.length;
      end = s.isValid ? s.end : controller.text.length;
      // 조합 중이던 글자는 칸의 글에 이미 들어 있다 — 확정 표시만 지워 두 번 들어가지 않게 한다.
      controller.clearComposing();
    }
    final forward = _selectionForward();
    _closePopup();
    await _run(() async {
      final before = _markdown;
      final result = await _call({
        'action': 'key',
        'operation': operation,
        'page': _page,
        'markdown': _markdown,
        'block': ?index,
        'start': start,
        'end': end,
        'text': ?text,
        'selection': selected,
        'folded': _indicesWhere((b) => b['folded'] == true),
        'paste': ?paste,
      });
      if (result['handled'] == false) return;
      final clipboard = result['clipboard'] as String?;
      if (clipboard != null) {
        _selectionClipboard = clipboard;
        await Clipboard.setData(ClipboardData(text: clipboard));
      }
      _markdown = result['markdown'] as String? ?? _markdown;
      _blocks = (result['document']?['blocks'] as List? ?? [])
          .whereType<Map>()
          .toList();
      _message = result['note'] as String? ?? '';
      _syncEditorsToBlocks();
      final chosen = [
        for (final i in result['selection'] as List? ?? const [])
          if (i is int && i < _blocks.length) _blocks[i]['id'],
      ];
      _selectedIds
        ..clear()
        ..addAll(chosen);
      if (chosen.isEmpty) {
        _selectionAnchor = null;
        _selectionCurrent = null;
      } else {
        _selectionAnchor = forward ? chosen.first : chosen.last;
        _selectionCurrent = forward ? chosen.last : chosen.first;
      }
      // 저장을 기다리기 전에 커서를 옮겨, 이어 치는 글이 새 위치로 들어가게 한다.
      final cursor = result['cursor'];
      if (mounted) setState(() {});
      if (cursor is Map) {
        final at = cursor['block'] as int? ?? 0;
        if (at < _blocks.length) {
          _focusBlockAfterBuild(
            _blocks[at]['id'],
            cursor['start'] as int?,
            cursor['end'] as int?,
          );
        }
      } else if (chosen.isNotEmpty) {
        _focusBlockAfterBuild(_selectionCurrent);
      }
      if (_markdown != before) {
        await _saveRecording(before);
        if (_page != null) await _rememberPage(_page!);
      }
    });
  }

  bool _selectionForward() {
    final anchor = _blocks.indexWhere((b) => b['id'] == _selectionAnchor);
    final current = _blocks.indexWhere((b) => b['id'] == _selectionCurrent);
    return anchor <= current;
  }

  Future<void> _selectionKey(String operation, [String? paste]) =>
      _key(operation, selection: true, paste: paste);

  Future<void> _pasteSelection() async {
    final data = await Clipboard.getData(Clipboard.kTextPlain);
    final value = data?.text ?? _selectionClipboard;
    if (value.isNotEmpty) await _selectionKey('paste', value);
  }

  /// 편집 중 Ctrl+V: 여러 줄·`- ` 목록은 블록 트리로, 한 줄은 칸에 넣는다. Ctrl+Shift+V 는 늘 칸에.
  Future<void> _pasteEditing(Map block, {required bool plain}) async {
    final controller = _editors[block['id']];
    final data = await Clipboard.getData(Clipboard.kTextPlain);
    final value = (data?.text ?? '').replaceAll('\r\n', '\n');
    if (!mounted || controller == null || value.isEmpty) return;
    final trimmed = value.replaceAll(RegExp(r'\n+$'), '');
    if (!plain &&
        (trimmed.contains('\n') || trimmed.trimLeft().startsWith('- '))) {
      await _key('paste', block: block, paste: value);
      return;
    }
    final insert = plain ? value : trimmed;
    final s = controller.selection;
    final start = s.isValid ? s.start : controller.text.length;
    final end = s.isValid ? s.end : controller.text.length;
    controller.value = TextEditingValue(
      text: controller.text.replaceRange(start, end, insert),
      selection: TextSelection.collapsed(offset: start + insert.length),
    );
    if (insert.contains('\n')) await _key('commit', block: block);
  }

  void _focusBlockAfterBuild(Object? id, [int? start, int? end]) {
    if (id == null) return;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      final node = _focusNodes[id];
      if (node != null && node.canRequestFocus) {
        node.requestFocus();
        final controller = _editors[id];
        if (controller != null) {
          final length = controller.text.length;
          final base = (start ?? length).clamp(0, length);
          controller.selection = TextSelection(
            baseOffset: base,
            extentOffset: (end ?? base).clamp(0, length),
          );
        }
      }
    });
    WidgetsBinding.instance.scheduleFrame();
  }

  /// 다른 블록 칸으로 옮겨 갈 때, 벗어난 칸의 확정하지 않은 글을 저장한다.
  void _commitOthers(Object? focused) {
    if (_page == null || _busy) return;
    for (final (i, block) in _blocks.indexed) {
      final controller = _editors[block['id']];
      if (block['id'] == focused ||
          controller == null ||
          controller.text == '${block['text'] ?? ''}') {
        continue;
      }
      final text = controller.text;
      final folded = _indicesWhere((b) => b['folded'] == true);
      _run(() async {
        final before = _markdown;
        final result = await _call({
          'action': 'key',
          'operation': 'commit',
          'page': _page,
          'markdown': _markdown,
          'block': i,
          'start': 0,
          'end': 0,
          'text': text,
          'selection': const <int>[],
          'folded': folded,
        });
        _markdown = result['markdown'] as String? ?? _markdown;
        _blocks = (result['document']?['blocks'] as List? ?? [])
            .whereType<Map>()
            .toList();
        if (_markdown != before) await _saveRecording(before);
      });
      return;
    }
  }

  // ---------- 블록 선택 ----------

  void _selectBlock(Object? id) {
    setState(() {
      _selectedIds
        ..clear()
        ..add(id);
      _selectionAnchor = id;
      _selectionCurrent = id;
    });
  }

  void _clearSelection() {
    setState(() {
      _selectedIds.clear();
      _selectionAnchor = null;
      _selectionCurrent = null;
    });
  }

  void _selectAll() {
    final visible = _visibleBlocks;
    if (visible.isEmpty) return;
    setState(() {
      _selectedIds
        ..clear()
        ..addAll(visible.map((b) => b['id']));
      _selectionAnchor = visible.first['id'];
      _selectionCurrent = visible.last['id'];
    });
  }

  void _selectParent() {
    final at = _blocks.indexWhere((b) => b['id'] == _selectionCurrent);
    if (at < 0) return;
    final indent = _blocks[at]['indent'] as int? ?? 0;
    for (var i = at - 1; i >= 0; i--) {
      if ((_blocks[i]['indent'] as int? ?? 0) < indent) {
        _selectBlock(_blocks[i]['id']);
        return;
      }
    }
    _selectAll();
  }

  /// 선택 끝(anchor 는 그대로)을 위/아래로 한 칸. [single] 이면 한 블록만 선택해 옮긴다.
  void _moveSelection(Object? from, bool up, {bool single = false}) {
    final visible = _visibleBlocks;
    final at = visible.indexWhere((b) => b['id'] == (_selectionCurrent ?? from));
    if (at < 0) return;
    final next = (at + (up ? -1 : 1)).clamp(0, visible.length - 1);
    final id = visible[next]['id'];
    if (single) {
      _selectBlock(id);
    } else {
      _extendSelection(id);
    }
  }

  /// 선택한 블록과 그 하위(화면 강조용).
  Set<Object?> _highlighted() {
    final out = <Object?>{};
    var depth = -1;
    for (final block in _blocks) {
      final indent = block['indent'] as int? ?? 0;
      if (depth >= 0 && indent > depth) {
        out.add(block['id']);
        continue;
      }
      depth = -1;
      if (_selectedIds.contains(block['id'])) {
        out.add(block['id']);
        depth = indent;
      }
    }
    return out;
  }

  // ---------- 자동완성 ----------

  void _closePopup() {
    _popupOpen = '';
    _popupQuery = '';
    _popupIndex = 0;
    _popupBlock = null;
  }

  void _updatePopup(Object? id, TextEditingController controller) {
    final caret = controller.selection.isValid
        ? controller.selection.start
        : controller.text.length;
    final before = controller.text.substring(0, caret);
    final link = RegExp(r'\[\[([^\[\]\n]*)$').firstMatch(before);
    final ref = RegExp(r'(\({2,3})([^()\n]*)$').firstMatch(before);
    setState(() {
      final open = link != null ? '[[' : ref?.group(1) ?? '';
      final query = link?.group(1) ?? ref?.group(2) ?? '';
      if (open != _popupOpen || query != _popupQuery) _popupIndex = 0;
      _popupOpen = open;
      _popupQuery = query;
      _popupBlock = open.isEmpty ? null : id;
    });
  }

  /// 후보: (보이는 이름, 괄호 안에 넣을 값).
  List<(String, String)> _candidates() {
    if (_popupOpen.isEmpty) return const [];
    final query = _popupQuery.toLowerCase();
    if (_popupOpen == '[[') {
      return [
        for (final p in _pages)
          if ('${p['name']}'.toLowerCase().contains(query))
            ('${p['name']}', '${p['name']}'),
      ].take(6).toList();
    }
    return [
      for (final b in _blocks)
        if (b['id'] != _popupBlock &&
            (query.isEmpty ||
                '${b['text'] ?? ''}'.toLowerCase().contains(query) ||
                '${b['id']}'.contains(query)))
          ('블록 ${b['id']}: ${b['text'] ?? ''}', '${b['id']}'),
    ].take(6).toList();
  }

  void _acceptPopup(String value) {
    final controller = _editors[_popupBlock];
    if (controller != null) {
      final caret = controller.selection.isValid
          ? controller.selection.start
          : controller.text.length;
      final start = controller.text.substring(0, caret).lastIndexOf(_popupOpen);
      if (start >= 0) {
        final close = _popupOpen == '[['
            ? ']]'
            : _popupOpen == '((('
            ? ')))'
            : '))';
        final end = controller.text.startsWith(close, caret)
            ? caret + close.length
            : caret;
        final inserted = '$_popupOpen$value$close';
        controller.value = TextEditingValue(
          text: controller.text.replaceRange(start, end, inserted),
          selection: TextSelection.collapsed(offset: start + inserted.length),
        );
      }
    }
    setState(_closePopup);
  }

  // ---------- 키 ----------

  /// 커서가 화면의 첫 줄(`up`) / 마지막 줄에 있는가(긴 줄의 자동 줄바꿈 포함).
  bool _onEdgeRow(TextEditingController controller, FocusNode focus, bool up) {
    final selection = controller.selection;
    if (!selection.isValid) return true;
    final offset = selection.extentOffset;
    final editable = focus.context
        ?.findAncestorStateOfType<EditableTextState>()
        ?.renderEditable;
    if (editable == null) {
      final text = controller.text;
      return up
          ? !text.substring(0, offset).contains('\n')
          : !text.substring(offset).contains('\n');
    }
    double top(int at) =>
        editable.getLocalRectForCaret(TextPosition(offset: at)).top;
    return up
        ? top(offset) <= top(0) + 1
        : top(offset) >= top(controller.text.length) - 1;
  }

  KeyEventResult _handleKey(
    Map block,
    TextEditingController controller,
    FocusNode focus,
    KeyEvent event,
  ) {
    if (event is KeyUpEvent) return KeyEventResult.ignored;
    if (_busy) return KeyEventResult.handled;
    final key = event.logicalKey;
    final mods = HardwareKeyboard.instance;
    final ctrl = mods.isControlPressed || mods.isMetaPressed;
    final alt = mods.isAltPressed;
    final shift = mods.isShiftPressed;
    final up = key == LogicalKeyboardKey.arrowUp;
    final vertical = up || key == LogicalKeyboardKey.arrowDown;
    final enter =
        key == LogicalKeyboardKey.enter ||
        key == LogicalKeyboardKey.numpadEnter;
    final tab = key == LogicalKeyboardKey.tab;
    final escape = key == LogicalKeyboardKey.escape;
    final composingRange = controller.value.composing;
    final composing = composingRange.isValid && !composingRange.isCollapsed;
    // 조합 중에는 Enter·Tab(확정 후 실행)만 가로챈다. 나머지는 IME 몫.
    if (composing && !enter && !tab) return KeyEventResult.ignored;
    final id = block['id'];

    // Ctrl+Z / Ctrl+Y / Ctrl+Shift+Z: 확정하지 않은 글이 있으면 칸 자체의 글자 되돌리기에 맡긴다.
    if (ctrl &&
        !alt &&
        (key == LogicalKeyboardKey.keyZ || key == LogicalKeyboardKey.keyY)) {
      if (controller.text != '${block['text'] ?? ''}') {
        return KeyEventResult.ignored;
      }
      _stepHistory(undo: !(key == LogicalKeyboardKey.keyY || shift));
      return KeyEventResult.handled;
    }

    if (_selectedIds.isNotEmpty) {
      if (escape) {
        _clearSelection();
      } else if (vertical && alt && shift) {
        _selectionKey(up ? 'move_up' : 'move_down');
      } else if (vertical && ctrl) {
        _selectionKey(up ? 'collapse' : 'expand');
      } else if (vertical) {
        _moveSelection(id, up, single: !(alt || shift));
      } else if (ctrl && key == LogicalKeyboardKey.semicolon) {
        _selectionKey('toggle_collapse');
      } else if (enter && ctrl) {
        _selectionKey('cycle_todo');
      } else if (enter) {
        final current = _selectionCurrent ?? id;
        _clearSelection();
        _focusBlockAfterBuild(current);
      } else if (tab) {
        _selectionKey(shift ? 'outdent' : 'indent');
      } else if (key == LogicalKeyboardKey.delete ||
          key == LogicalKeyboardKey.backspace) {
        _selectionKey('delete_selection');
      } else if (ctrl && key == LogicalKeyboardKey.keyC) {
        _selectionKey('copy');
      } else if (ctrl && key == LogicalKeyboardKey.keyX) {
        _selectionKey('cut');
      } else if (ctrl && key == LogicalKeyboardKey.keyV) {
        _pasteSelection();
      } else if (ctrl && shift && key == LogicalKeyboardKey.keyA) {
        _selectAll();
      } else if (ctrl && key == LogicalKeyboardKey.keyA) {
        _selectParent();
      } else {
        return KeyEventResult.ignored;
      }
      return KeyEventResult.handled;
    }

    final candidates = _popupBlock == id ? _candidates() : const [];
    if (candidates.isNotEmpty && !ctrl && !alt && !shift) {
      if (vertical) {
        setState(
          () => _popupIndex =
              (_popupIndex + (up ? -1 : 1)) % candidates.length,
        );
        return KeyEventResult.handled;
      }
      if ((enter || tab) && !composing) {
        _acceptPopup(
          candidates[_popupIndex.clamp(0, candidates.length - 1)].$2,
        );
        return KeyEventResult.handled;
      }
      if (escape) {
        setState(_closePopup);
        return KeyEventResult.handled;
      }
    }

    KeyEventResult run(String operation) {
      _key(operation, block: block);
      return KeyEventResult.handled;
    }

    // 확정하지 않은 글이 있으면 저장한 뒤 블록을 선택한다.
    KeyEventResult select() {
      if (controller.text == '${block['text'] ?? ''}') {
        setState(_closePopup);
        _selectBlock(id);
      } else {
        _key('commit', block: block).then((_) {
          if (mounted && _blocks.any((b) => b['id'] == id)) _selectBlock(id);
        });
      }
      return KeyEventResult.handled;
    }

    final selection = controller.selection;
    final collapsed = selection.isValid && selection.isCollapsed;
    final atStart = collapsed && selection.start == 0;
    final atEnd = collapsed && selection.start == controller.text.length;

    if (tab) return run(shift ? 'outdent' : 'indent');
    if (enter) {
      if (alt) return KeyEventResult.handled;
      if (ctrl) return run('cycle_todo');
      return run(shift ? 'newline' : 'enter');
    }
    if (escape) return select();
    if (vertical) {
      if (alt) {
        if (ctrl) return KeyEventResult.ignored;
        return shift ? run(up ? 'move_up' : 'move_down') : select();
      }
      if (ctrl) return run(up ? 'collapse' : 'expand');
      if (!_onEdgeRow(controller, focus, up)) return KeyEventResult.ignored;
      return shift ? select() : run(up ? 'up' : 'down');
    }
    if (ctrl && key == LogicalKeyboardKey.semicolon) {
      return run('toggle_collapse');
    }
    if (ctrl && !alt && key == LogicalKeyboardKey.keyA) {
      if (shift) {
        _selectAll();
        return KeyEventResult.handled;
      }
      final all =
          selection.start == 0 && selection.end == controller.text.length;
      return all ? select() : KeyEventResult.ignored;
    }
    if (ctrl && !alt && key == LogicalKeyboardKey.keyV) {
      _pasteEditing(block, plain: shift);
      return KeyEventResult.handled;
    }
    if (ctrl || alt || shift) return KeyEventResult.ignored;
    if (key == LogicalKeyboardKey.backspace && atStart) return run('backspace');
    if (key == LogicalKeyboardKey.delete && atEnd) return run('delete');
    if (key == LogicalKeyboardKey.arrowLeft && atStart) return run('left');
    if (key == LogicalKeyboardKey.arrowRight && atEnd) return run('right');
    return KeyEventResult.ignored;
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
      final folded = _indicesWhere((b) => b['folded'] == true).toSet();
      _markdown = result['markdown'] as String? ?? _markdown;
      _blocks = [
        for (final (i, b)
            in (result['document']?['blocks'] as List? ?? [])
                .whereType<Map>()
                .indexed)
          folded.contains(i) ? {...b, 'folded': true} : b,
      ];
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
    final highlighted = _highlighted();
    final candidates = _candidates();
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
                      message:
                          '새 블록 Enter(빈 블록은 내어쓰기) · 줄바꿈 Shift+Enter · 들여/내어쓰기 Tab/Shift+Tab\n'
                          '블록 선택 Esc·Alt+↑/↓ · 범위 Shift+↑/↓ · 이동 Alt+Shift+↑/↓\n'
                          '접기/펼치기 Ctrl+↑/↓·Ctrl+; · TODO Ctrl+Enter · 한 블록에 붙여넣기 Ctrl+Shift+V',
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
                      IconButton(tooltip: '선택 블록 삭제 (Delete)', onPressed: _busy ? null : () => _selectionKey('delete_selection'), icon: const Icon(Icons.delete_outline)),
                      IconButton(tooltip: '선택 블록 들여쓰기 (Tab)', onPressed: _busy ? null : () => _selectionKey('indent'), icon: const Icon(Icons.format_indent_increase)),
                      IconButton(tooltip: '선택 블록 내어쓰기 (Shift+Tab)', onPressed: _busy ? null : () => _selectionKey('outdent'), icon: const Icon(Icons.format_indent_decrease)),
                      IconButton(tooltip: '선택 블록 접기', onPressed: _busy ? null : () => _selectionKey('collapse'), icon: const Icon(Icons.unfold_less)),
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
                            color: highlighted.contains(block['id'])
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
                                  : () => _key(
                                      'toggle_collapse',
                                      block: block,
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
                                  final focus = _focusNodes.putIfAbsent(id, () {
                                    final node = FocusNode();
                                    node.addListener(() {
                                      if (node.hasFocus) _commitOthers(id);
                                    });
                                    return node;
                                  });
                                  if (!focus.hasFocus &&
                                      controller.text !=
                                          '${block['text'] ?? ''}') {
                                    controller.text = '${block['text'] ?? ''}';
                                  }
                                  return Focus(
                                    onKeyEvent: (node, event) => _handleKey(
                                      block,
                                      controller,
                                      focus,
                                      event,
                                    ),
                                    child: TextField(
                                      key: ValueKey('outliner-block-$id'),
                                      controller: controller,
                                      focusNode: focus,
                                      maxLines: null,
                                      keyboardType: TextInputType.multiline,
                                      readOnly: _selectedIds.isNotEmpty,
                                      inputFormatters: [_pairFormatter],
                                      onTap: () {
                                        if (_selectedIds.isNotEmpty) {
                                          _clearSelection();
                                        }
                                      },
                                      onChanged: (_) =>
                                          _updatePopup(id, controller),
                                      decoration: const InputDecoration(
                                        border: InputBorder.none,
                                        isDense: true,
                                      ),
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
                                  : () => _key('indent', block: block),
                              icon: const Icon(Icons.keyboard_tab, size: 18),
                            ),
                            IconButton(
                              tooltip: '내어쓰기',
                              onPressed: _busy
                                  ? null
                                  : () => _key('outdent', block: block),
                              icon: const Icon(
                                Icons.format_indent_decrease,
                                size: 18,
                              ),
                            ),
                          ],
                          ),
                        ),
                      ),
                    for (final (i, candidate) in candidates.indexed)
                      ListTile(
                        dense: true,
                        selected: i == _popupIndex,
                        leading: Icon(
                          _popupOpen == '[[' ? Icons.link : Icons.tag,
                        ),
                        title: Text(candidate.$1),
                        onTap: () => _acceptPopup(candidate.$2),
                      ),
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
            selectedBlockId: _selectedIds.firstOrNull,
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
    this.selectedBlockId,
    required this.onOpen,
  });
  final ApiCall api;
  final String? page;
  final String markdown;
  final Object? selectedBlockId;
  final ValueChanged<String> onOpen;
  @override
  State<_BacklinksPanel> createState() => _BacklinksPanelState();
}

class _BacklinksPanelState extends State<_BacklinksPanel> {
  List<dynamic> _ids = [];
  List<dynamic> _blockRefIds = [];

  @override
  void didUpdateWidget(covariant _BacklinksPanel oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.page != widget.page ||
        oldWidget.markdown != widget.markdown ||
        oldWidget.selectedBlockId != widget.selectedBlockId) {
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

    if (widget.selectedBlockId != null) {
      try {
        final raw = await widget.api('outliner', [
          {
            'action': 'block_backlinks',
            'page': widget.page,
            'markdown': widget.markdown,
            'target': '${widget.selectedBlockId}',
          },
        ]);
        final data = raw is Map ? raw['data'] : null;
        if (mounted) {
          setState(
            () => _blockRefIds =
                data is Map ? (data['backlinks'] as List? ?? []) : [],
          );
        }
      } catch (_) {}
    } else {
      if (mounted) setState(() => _blockRefIds = []);
    }
  }

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.all(12),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text('역링크', style: Theme.of(context).textTheme.titleSmall),
        const SizedBox(height: 6),
        if (_ids.isEmpty)
          const Text('참조가 없습니다.', style: TextStyle(fontSize: 12))
        else
          for (final id in _ids) Text('블록 $id', style: const TextStyle(fontSize: 12)),
        const SizedBox(height: 16),
        Text('역참조 (선택 블록)', style: Theme.of(context).textTheme.titleSmall),
        const SizedBox(height: 6),
        if (widget.selectedBlockId == null)
          const Text('선택된 블록이 없습니다.', style: TextStyle(fontSize: 12))
        else if (_blockRefIds.isEmpty)
          const Text('이 블록을 참조하는 블록이 없습니다.', style: TextStyle(fontSize: 12))
        else
          for (final id in _blockRefIds)
            Text('블록 $id 에서 참조', style: const TextStyle(fontSize: 12)),
      ],
    ),
  );
}
