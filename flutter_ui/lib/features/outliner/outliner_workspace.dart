import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../core/api/abbreviation_api.dart' show ApiCall;
import '../../core/theme/app_theme.dart';
import 'outliner_history.dart';
import 'outliner_typeahead.dart';
import 'outliner_typing.dart';

class OutlinerWorkspace extends StatefulWidget {
  const OutlinerWorkspace({
    super.key,
    required this.api,
    this.recentPagesStore,
    this.panelStore,
    this.headerActions = const [],
    this.notice = '',
  });
  final ApiCall api;
  final RecentPagesStore? recentPagesStore;

  /// 접은 패널을 기억하는 곳. 없으면 모두 펼친 채 시작하고 기억하지 않는다
  /// (앱에서는 `OutlinerUiGate` 가 파일 저장소를 넣는다).
  final OutlinerPanelStore? panelStore;

  /// 제목 줄 오른쪽 끝에 덧붙이는 단추.
  final List<Widget> headerActions;

  /// 제목 줄 아래에 보일 바깥(문지기)의 안내 문구.
  final String notice;

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

/// 아웃라이너 데이터 폴더(최근 페이지·접은 패널 파일).
Future<Directory> outlinerDataDirectory() async {
  final base =
      Platform.environment['LOCALAPPDATA'] ?? Platform.environment['APPDATA'];
  if (base == null || base.isEmpty) {
    throw const FileSystemException('사용자 데이터 폴더를 찾을 수 없습니다.');
  }
  return Directory('$base${Platform.pathSeparator}HwpAutoDocFit');
}

const outlinerPanelKeys = {'left', 'right', 'backlinks', 'blockRefs'};

/// 접은 상태. 키는 [outlinerPanelKeys], 값 true = 접음. 사용자가 누른 적 없는 키는 없다.
abstract interface class OutlinerPanelStore {
  Future<Map<String, bool>> load();
  Future<void> save(Map<String, bool> collapsed);
}

class FileOutlinerPanelStore implements OutlinerPanelStore {
  FileOutlinerPanelStore(this.directoryProvider);
  final Future<Directory> Function() directoryProvider;

  Future<File> _file() async => File(
    '${(await directoryProvider()).path}${Platform.pathSeparator}outliner_panels.json',
  );

  @override
  Future<Map<String, bool>> load() async {
    final decoded = jsonDecode(await (await _file()).readAsString());
    return {
      if (decoded is Map)
        for (final key in outlinerPanelKeys)
          if (decoded[key] is bool) key: decoded[key] as bool,
    };
  }

  @override
  Future<void> save(Map<String, bool> collapsed) async {
    final file = await _file();
    await file.parent.create(recursive: true);
    await file.writeAsString(jsonEncode(collapsed));
  }
}

const _leftWidth = 248.0;
const _rightWidth = 220.0;
const _stripWidth = 36.0;
const _minBodyWidth = 420.0;
const _minRowForIndentButtons = 250.0;

/// 이 안의 단추를 눌러도 편집 중인 칸의 포커스·커서·확정 전 글이 그대로이게 한다.
/// (칸 바깥 클릭으로 치지 않고, 단추가 포커스를 가져가지도 않는다.)
Widget keepEditorFocus(Widget child) =>
    TextFieldTapRegion(child: ExcludeFocus(child: child));

class _OutlinerWorkspaceState extends State<OutlinerWorkspace> {
  // 패널 접힘. 한 키의 상태 = 사용자가 누른 값(_panels, 저장됨) → 없으면 처음 값.
  // 처음 값은 펼침. 오른쪽만, 본문이 좁으면 접음(저장하지 않는다). 저장 값을 읽기 전에는
  // 지금 폭으로 따지고, 읽은 뒤 첫 그리기에서 한 번 정해(_narrowStart) 창 크기를 따라 바뀌지 않는다.
  final Map<String, bool> _panels = {};
  bool _panelsRead = false;
  bool? _narrowStart;
  Future<void> _panelsLoaded = Future.value();
  Future<void> _panelsSaved = Future.value();
  List<Map> _pages = [];
  List<String> _recentPages = [];
  bool _recentLoaded = false;
  List<Map> _blocks = [];
  String? _page;
  String _markdown = '';
  String? _version;
  String _message = '';
  bool _busy = false;
  // 명령 처리 중에 누른 Enter·Tab 은 버리지 않고 끝난 뒤 차례로 적용한다(OUTFLASH1·OUTKEYS2).
  // [before] 는 앞 명령과 이 키 사이에 친 글이다.
  final List<({String operation, TextEditingValue before, String? page})>
  _queued = [];
  static const _queueLimit = 64;
  bool _editing = false; // 처리 중인 명령이 블록 편집이라 이어 치는 글을 받는다
  // 칸을 벗어나며 남긴 글을 저장하는 중. 누른 이동·단추는 이 저장을 기다렸다가 한다.
  bool _leaveSave = false;
  Future<bool>? _leaving;
  // 칸마다 마지막으로 맞춘 문서 글. 문서 글이 그대로인데 칸이 다르면 저장하지 않은 입력이다.
  final Map<Object?, String> _shown = {};
  // 블록 검색으로 거른 목록과, 거르기 전의 블록 번호 순서.
  List<Map>? _filtered;
  List<Object?> _unfiltered = const [];
  // 마지막 명령이 커서를 둔 블록과 그때의 칸(친 글을 뺀 상태).
  ({Object? id, TextEditingValue value})? _at;
  // 입력 중인 칸 = 이 값 + 마지막 예약 키 뒤에 친 글.
  ({Object? id, TextEditingValue value})? _base;
  Object? _pendingFocus; // 다음 그리기 뒤에 커서를 옮길 블록
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
    _panelsLoaded = _loadPanels();
    _refreshPages();
  }

  Future<void> _loadPanels() async {
    final store = widget.panelStore;
    if (store == null) return;
    var stored = const <String, bool>{};
    try {
      stored = await store.load();
    } catch (_) {}
    if (!mounted) return;
    setState(() {
      // 읽는 동안 사용자가 누른 값이 먼저다.
      for (final MapEntry(:key, :value) in stored.entries) {
        _panels.putIfAbsent(key, () => value);
      }
      _panelsRead = true;
    });
  }

  bool _collapsed(String key) => _panels[key] ?? false;

  void _setCollapsed(String key, bool value) {
    setState(() => _panels[key] = value);
    final store = widget.panelStore;
    if (store == null) return;
    // 다 읽은 뒤에 차례로 쓴다(저장해 둔 다른 키를 지우거나 겹쳐 쓰지 않게).
    _panelsSaved = _panelsSaved.then((_) async {
      await _panelsLoaded;
      try {
        await store.save(Map.of(_panels));
      } catch (_) {}
    });
  }

  bool _rightCollapsed(double width) {
    final chosen = _panels['right'];
    if (chosen != null) return chosen;
    if (widget.panelStore == null) return false;
    final left = _collapsed('left') ? _stripWidth : _leftWidth;
    final narrow = width - left - _rightWidth - 2 < _minBodyWidth;
    return _panelsRead ? _narrowStart ??= narrow : narrow;
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

  Future<List<String>> _loadRecentPages(Set<String> available, Object? fallback) async {
    try {
      final store = widget.recentPagesStore ?? FileRecentPagesStore(outlinerDataDirectory);
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
      final store = widget.recentPagesStore ?? FileRecentPagesStore(outlinerDataDirectory);
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
    _shown.clear();
    _syncEditorsToBlocks(); // 블록 번호는 페이지마다 다시 쓰인다.
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
              onPressed: () => _after(_undoRename),
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
              onPressed: () => _after(_restorePage),
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

  /// [editing] 이면 처리 중에 친 글·Enter·Tab 을 받는다. [typed] 는 명령에 보낸 칸의 상태로,
  /// 결과가 칸의 글을 바꾸는 명령만 준다.
  Future<bool> _run(
    Future<void> Function() action, {
    bool editing = false,
    ({Object? id, TextEditingValue value})? typed,
  }) async {
    _editing = editing;
    _at = _base = typed;
    setState(() {
      _busy = true;
      _message = '';
    });
    var failed = false;
    try {
      await action();
      if (mounted) setState(() {});
    } catch (e) {
      failed = true;
      if (mounted) setState(() => _message = '$e');
    } finally {
      if (mounted) {
        setState(() => _busy = false);
        if (failed) {
          _abortQueued();
        } else if (_queued.isNotEmpty) {
          _runQueued();
        } else {
          _at = _base = null;
        }
        // 처리 중에 칸을 벗어났다면 남은 글을 이어서 저장한다(실패 직후에는 되풀이하지 않는다).
        if (!failed && !_blockHasFocus && _pendingFocus == null) {
          _saveLeft();
        }
      }
    }
    return !failed;
  }

  bool get _blockHasFocus => _focusNodes.values.any((n) => n.hasFocus);

  /// 이동·단추를 막아야 하는 처리 중.
  bool get _locked => _busy && !_leaveSave;

  /// 아직 저장하지 않은 입력이 있는 칸인가.
  bool _dirty(Map block) {
    final controller = _editors[block['id']];
    final text = '${block['text'] ?? ''}';
    return controller != null &&
        controller.text != text &&
        _shown[block['id']] == text;
  }

  /// [except] 밖의 칸에 남은 글을 저장한다. 저장할 것이 없거나 지금 못 하면 null.
  Future<bool>? _saveLeft({Object? except}) {
    if (_page == null || _busy || _selectedIds.isNotEmpty) return null;
    final block = _blocks
        .where((b) => b['id'] != except && _dirty(b))
        .firstOrNull;
    if (block == null) return null;
    // 블록 검색으로 걸러진 목록이면 문서 안의 자리를 보낸다.
    final index = identical(_filtered, _blocks)
        ? _unfiltered.indexOf(block['id'])
        : _blocks.indexOf(block);
    final sent = {
      for (final MapEntry(:key, :value) in _editors.entries) key: value.text,
    };
    final folded = _indicesWhere((b) => b['folded'] == true);
    _leaveSave = true;
    return _leaving = _run(() async {
      try {
        final before = _markdown;
        final result = await _call({
          'action': 'key',
          'operation': 'commit',
          'page': _page,
          'markdown': _markdown,
          'block': index,
          'start': 0,
          'end': 0,
          'text': sent[block['id']],
          'selection': const <int>[],
          'folded': folded,
        });
        _markdown = result['markdown'] as String? ?? _markdown;
        _blocks = (result['document']?['blocks'] as List? ?? [])
            .whereType<Map>()
            .toList();
        // 보낸 뒤에 더 친 칸은 덮지 않는다.
        for (final b in _blocks) {
          final id = b['id'];
          final controller = _editors[id];
          if (controller == null) continue;
          _shown[id] = '${b['text'] ?? ''}';
          if (controller.text == sent[id]) _syncEditor(b);
        }
        if (_markdown != before) await _saveRecording(before);
      } finally {
        _leaveSave = false;
      }
    }, editing: true);
  }

  /// 남은 글을 모두 저장한다. false 면 하려던 이동을 하지 않는다(글은 칸에 남는다).
  Future<bool> _settle() async {
    if (_busy) {
      if (!_leaveSave || !await _leaving!) return false;
    }
    while (mounted && !_busy) {
      final save = _saveLeft();
      if (save == null) return true;
      if (!await save) return false;
    }
    return false;
  }

  /// 남은 글을 저장한 뒤 [action] 을 한다.
  Future<void> _after(void Function() action) async {
    if (await _settle() && mounted) action();
  }

  /// 블록 줄의 단추: 저장하면 문서가 새로 읽히므로 같은 번호의 블록을 다시 찾는다.
  Future<void> _onRow(Object? id, void Function(Map block) action) =>
      _after(() {
        final block = _blocks.where((b) => b['id'] == id).firstOrNull;
        if (block != null) action(block);
      });

  /// 예약한 키를 적용하지 않는다. 친 글은 이미 칸에 있으므로 칸을 지키기만 한다.
  void _abortQueued() {
    final count = _queued.length;
    _queued.clear();
    final id = _base?.id;
    _at = _base = null;
    final block = _blocks.where((b) => b['id'] == id).firstOrNull;
    final controller = _editors[id];
    // 포커스 없는 칸은 다음 그리기에서 문서 글로 되돌아가므로 커서를 돌려놓는다.
    if (block != null &&
        controller != null &&
        controller.text != '${block['text'] ?? ''}') {
      _focusNodes[id]?.requestFocus();
    }
    if (count > 0) {
      setState(
        () => _message = [
          if (_message.isNotEmpty) _message,
          '처리 중에 누른 Enter·Tab $count개는 적용하지 않았습니다. 친 글은 칸에 남아 있습니다.',
        ].join('\n'),
      );
    }
  }

  void _runQueued() {
    final next = _queued.first;
    final cursor = _at;
    final base = _base;
    final controller = _editors[cursor?.id];
    final at = _blocks.indexWhere((b) => b['id'] == cursor?.id);
    if (cursor == null ||
        base == null ||
        controller == null ||
        at < 0 ||
        next.page != _page ||
        _selectedIds.isNotEmpty) {
      _abortQueued();
      return;
    }
    final live = typedSince(base.value, controller.value);
    _queued.removeAt(0);
    final value = insertTyped(cursor.value, next.before);
    _key(next.operation, block: _blocks[at], value: value);
    // 아직 보내지 않은 글은 계속 칸에 보인다.
    final rest = _queued.fold(value, (v, q) => insertTyped(v, q.before));
    _base = (id: cursor.id, value: rest);
    _setValue(controller, insertTyped(rest, live));
  }

  void _setValue(TextEditingController controller, TextEditingValue value) {
    if (controller.text != value.text ||
        controller.selection != value.selection) {
      controller.value = value;
    }
  }

  /// 처리 중에 누른 Enter·Tab 을 예약한다.
  void _enqueue(String operation, Object? pressedId) {
    if (_queued.length >= _queueLimit) return;
    final id = _base?.id ?? pressedId;
    final controller = _editors[id];
    if (controller == null) return;
    controller.clearComposing(); // 조합 중이던 글자는 칸의 글에 이미 들어 있다.
    final now = controller.value;
    final base = _base;
    // 칸의 글을 바꾸지 않는 명령(다른 칸 저장 등) 중: 지금 칸이 그대로 기준이다.
    if (base == null) _at = (id: id, value: now);
    _queued.add((
      operation: operation,
      before: base == null
          ? TextEditingValue.empty
          : typedSince(base.value, now),
      page: _page,
    ));
    _base = (id: id, value: now);
  }

  /// 결과를 칸에 반영하면서, 처리 중에 친 글을 커서가 옮겨 간 블록으로 옮긴다.
  void _placeCursor(Object? cursor) {
    final base = _base;
    if (base == null) {
      _syncEditorsToBlocks();
      return;
    }
    final old = _editors[base.id];
    final live = old == null
        ? TextEditingValue.empty
        : typedSince(base.value, old.value);
    final ahead = _queued.isNotEmpty || live.text.isNotEmpty;
    var index = cursor is Map ? cursor['block'] as int? ?? 0 : -1;
    if (index < 0 || index >= _blocks.length) {
      index = _blocks.indexWhere((b) => b['id'] == base.id);
      if (index < 0) index = _blocks.length - 1;
    }
    if (index < 0) {
      _syncEditorsToBlocks();
      return;
    }
    final id = _blocks[index]['id'];
    final text = '${_blocks[index]['text'] ?? ''}';
    final controller = _editors.putIfAbsent(
      id,
      () => TextEditingController(text: text),
    );
    var selection = TextSelection.collapsed(offset: text.length);
    if (cursor is Map) {
      final start = (cursor['start'] as int? ?? text.length).clamp(
        0,
        text.length,
      );
      selection = TextSelection(
        baseOffset: start,
        extentOffset: (cursor['end'] as int? ?? start).clamp(0, text.length),
      );
    } else if (id == base.id && controller.text == text) {
      selection = controller.selection;
    } else if (id == base.id && base.value.text == text) {
      selection = base.value.selection;
    }
    _syncEditorsToBlocks(except: id);
    _shown[id] = text;
    final clean = TextEditingValue(text: text, selection: selection);
    final rest = _queued.fold(clean, (v, q) => insertTyped(v, q.before));
    _at = (id: id, value: clean);
    _base = (id: id, value: rest);
    _setValue(controller, insertTyped(rest, live));
    if (cursor is Map || ahead) _focusBlockAfterBuild(id, keep: true);
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
    }, editing: true);
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
    TextEditingValue? value,
  }) async {
    if (_page == null || _busy) return;
    // 저장하지 못한 글이 다른 칸에 남아 있으면 명령이 덮으므로 저장부터 다시 한다.
    if (block != null &&
        _blocks.any((b) => b['id'] != block['id'] && _dirty(b))) {
      _saveLeft(except: block['id']);
      return;
    }
    final controller = block == null ? null : _editors[block['id']];
    final index = block == null ? null : _blocks.indexOf(block);
    final selected = selection
        ? _indicesWhere((b) => _selectedIds.contains(b['id']))
        : const <int>[];
    if (selection && selected.isEmpty) return;
    final field = value ?? controller?.value;
    final text = field?.text;
    var start = 0;
    var end = 0;
    if (field != null) {
      final s = field.selection;
      start = s.isValid ? s.start : field.text.length;
      end = s.isValid ? s.end : field.text.length;
      // 조합 중이던 글자는 칸의 글에 이미 들어 있다 — 확정 표시만 지워 두 번 들어가지 않게 한다.
      if (value == null) controller!.clearComposing();
    }
    final typed = block == null || text == null
        ? null
        : (
            id: block['id'] as Object?,
            value: TextEditingValue(
              text: text,
              selection: TextSelection(baseOffset: start, extentOffset: end),
            ),
          );
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
      final chosen = [
        for (final i in result['selection'] as List? ?? const [])
          if (i is int && i < _blocks.length) _blocks[i]['id'],
      ];
      // 그 사이에 친 글이 있으면 블록 선택 대신 커서를 두어 글을 지킨다.
      if (typed != null &&
          (_queued.isNotEmpty ||
              (controller != null && controller.text != _base?.value.text))) {
        chosen.clear();
      }
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
      if (mounted && typed != null && chosen.isEmpty) {
        _placeCursor(result['cursor']);
      } else if (mounted) {
        _syncEditorsToBlocks();
        if (chosen.isNotEmpty) _focusBlockAfterBuild(_selectionCurrent);
      }
      if (mounted) setState(() {});
      if (_markdown != before) {
        await _saveRecording(before);
        if (_page != null) await _rememberPage(_page!);
      }
    }, editing: typed != null, typed: typed);
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

  /// [keep] 이면 칸에 이미 정해 둔 커서를 그대로 둔다.
  void _focusBlockAfterBuild(Object? id, {bool keep = false}) {
    if (id == null) return;
    _pendingFocus = id;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted || _pendingFocus != id) return;
      _pendingFocus = null;
      final node = _focusNodes[id];
      if (node != null && node.canRequestFocus) {
        final controller = _editors[id];
        if (keep && controller != null) _adoptStrayInput(id, controller);
        node.requestFocus();
        if (!keep && controller != null) {
          controller.selection = TextSelection.collapsed(
            offset: controller.text.length,
          );
        }
      }
    });
    WidgetsBinding.instance.scheduleFrame();
  }

  /// 결과를 반영한 뒤 커서가 옮겨 가기 전에 옛 칸에 들어온 글을 새 칸으로 가져온다.
  void _adoptStrayInput(Object? to, TextEditingController target) {
    for (final block in _blocks) {
      final id = block['id'];
      final controller = _editors[id];
      final text = '${block['text'] ?? ''}';
      if (id == to ||
          controller == null ||
          _focusNodes[id]?.hasFocus != true ||
          controller.text == text) {
        continue;
      }
      final stray = typedSince(
        TextEditingValue(
          text: text,
          selection: TextSelection.collapsed(offset: text.length),
        ),
        controller.value,
      );
      target.value = insertTyped(target.value, stray);
      controller.text = text;
    }
  }

  /// 칸의 포커스 변화: 다른 블록으로 가면 벗어난 칸을, 블록 밖으로 나가면 남은 글을 저장한다.
  void _focusChanged(Object? id, FocusNode node) {
    if (!mounted) return;
    if (node.hasFocus) {
      _saveLeft(except: id);
    } else if (!_blockHasFocus && _pendingFocus == null) {
      _saveLeft();
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
    if (_busy && (!_editing || _selectedIds.isNotEmpty)) {
      return KeyEventResult.handled;
    }
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

    if (_busy) return _handleBusyKey(id, controller, event, composing);

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
        _editing = false; // 곧 블록 선택이 되므로 이어 누른 키는 받지 않는다.
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

  /// 편집 명령 처리 중의 키. 글자는 입력 칸에 맡기고 Enter·Tab 은 예약한다.
  /// 커서를 옮기는 키와 단축키는 친 글의 자리를 알 수 없게 하므로 받지 않는다.
  KeyEventResult _handleBusyKey(
    Object? id,
    TextEditingController controller,
    KeyEvent event,
    bool composing,
  ) {
    final key = event.logicalKey;
    final mods = HardwareKeyboard.instance;
    final ctrl = mods.isControlPressed || mods.isMetaPressed;
    final alt = mods.isAltPressed;
    final shift = mods.isShiftPressed;
    final enter =
        key == LogicalKeyboardKey.enter ||
        key == LogicalKeyboardKey.numpadEnter;
    final tab = key == LogicalKeyboardKey.tab;
    if (enter || tab) {
      if (event is KeyDownEvent && !(enter && alt)) {
        _enqueue(
          tab
              ? (shift ? 'outdent' : 'indent')
              : ctrl
              ? 'cycle_todo'
              : (shift ? 'newline' : 'enter'),
          id,
        );
      }
      return KeyEventResult.handled;
    }
    if (composing) return KeyEventResult.ignored; // 나머지는 IME 몫
    if (key == LogicalKeyboardKey.backspace ||
        key == LogicalKeyboardKey.delete) {
      // 처리 중에 친 글 안에서만 지운다. 명령에 보낸 글은 건드리지 않는다.
      final s = controller.selection;
      if (ctrl || alt || !s.isValid) return KeyEventResult.handled;
      final base = _base?.value;
      if (base == null) return KeyEventResult.ignored;
      final head = base.selection.isValid
          ? base.selection.start
          : base.text.length;
      final tail = base.selection.isValid
          ? base.text.length - base.selection.end
          : 0;
      final end = controller.text.length - tail;
      final back = key == LogicalKeyboardKey.backspace;
      final inside =
          s.start >= head &&
          s.end <= end &&
          (!s.isCollapsed || (back ? s.start > head : s.end < end));
      return inside ? KeyEventResult.ignored : KeyEventResult.handled;
    }
    final blocked =
        key == LogicalKeyboardKey.escape ||
        key == LogicalKeyboardKey.arrowUp ||
        key == LogicalKeyboardKey.arrowDown ||
        key == LogicalKeyboardKey.arrowLeft ||
        key == LogicalKeyboardKey.arrowRight ||
        key == LogicalKeyboardKey.home ||
        key == LogicalKeyboardKey.end ||
        key == LogicalKeyboardKey.pageUp ||
        key == LogicalKeyboardKey.pageDown ||
        ctrl != alt; // AltGr(Ctrl+Alt)은 글자 입력이다.
    return blocked ? KeyEventResult.handled : KeyEventResult.ignored;
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
  Map? _pendingBlock() => _blocks
      .where(
        (b) =>
            _dirty(b) ||
            ((_focusNodes[b['id']]?.hasFocus ?? false) &&
                _editors[b['id']]?.text != '${b['text'] ?? ''}'),
      )
      .firstOrNull;

  void _syncEditorsToBlocks({Object? except}) {
    for (final block in _blocks) {
      if (except != null && block['id'] == except) continue;
      _syncEditor(block);
    }
  }

  void _syncEditor(Map block) {
    final controller = _editors[block['id']];
    if (controller == null) return;
    final text = '${block['text'] ?? ''}';
    _shown[block['id']] = text;
    if (controller.text != text) {
      controller.value = TextEditingValue(
        text: text,
        selection: TextSelection.collapsed(offset: text.length),
      );
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
    final focused = _blocks
        .where((b) => _focusNodes[b['id']]?.hasFocus ?? false)
        .firstOrNull;
    final controller = _editors[focused?['id']];
    final typed = controller == null
        ? null
        : (id: focused!['id'] as Object?, value: controller.value);
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
        if (mounted && typed != null) {
          _placeCursor(null);
          setState(() {});
        } else {
          _syncEditorsToBlocks();
        }
        await _save();
      } catch (_) {
        _history.clear();
        rethrow;
      }
    }, editing: true, typed: typed);
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
    }, editing: true);
  }

  Future<void> _searchBlocks(String query) async {
    if (query.trim().runes.length < 2 || !await _settle()) return;
    if (!mounted || _page == null) return;
    await _run(() async {
      final result = await _call({
        'action': 'search',
        'page': _page,
        'markdown': _markdown,
        'query': query,
      });
      final ids = (result['block_ids'] as List? ?? []).toSet();
      setState(() {
        if (!identical(_filtered, _blocks)) {
          _unfiltered = [for (final b in _blocks) b['id']];
        }
        _blocks = _filtered = _blocks
            .where((b) => ids.contains(b['id']))
            .toList();
      });
    });
  }

  @override
  Widget build(BuildContext context) =>
      LayoutBuilder(builder: (_, size) => _buildPanels(size.maxWidth));

  /// 패널 전체를 접고 펴는 단추. [open] 은 펼 때의 화살표 방향.
  Widget _panelToggle(String key, String name, bool collapsed, bool right) =>
      keepEditorFocus(
        IconButton(
          key: ValueKey('outliner-$key-toggle'),
          tooltip: collapsed ? '$name 패널 펴기' : '$name 패널 접기',
          visualDensity: VisualDensity.compact,
          padding: EdgeInsets.zero,
          onPressed: () => _setCollapsed(key, !collapsed),
          icon: Icon(
            collapsed == right ? Icons.chevron_left : Icons.chevron_right,
          ),
        ),
      );

  /// 접은 패널의 좁은 세로 띠.
  Widget _strip(Widget toggle) => Align(
    alignment: Alignment.topCenter,
    child: Padding(padding: const EdgeInsets.only(top: 12), child: toggle),
  );

  /// 제목 줄. 좁으면 단추를 제목 아래 줄로 내려 넘치지 않게 한다.
  Widget _header(AppColors c) {
    final title = Text(
      _page ?? '아웃라이너',
      style: Theme.of(context).textTheme.headlineSmall?.copyWith(
        color: c.ink,
        fontWeight: FontWeight.w700,
      ),
    );
    final actions = <Widget>[
      const Tooltip(
        message:
            '새 블록 Enter(빈 블록은 내어쓰기) · 줄바꿈 Shift+Enter · 들여/내어쓰기 Tab/Shift+Tab\n'
            '블록 선택 Esc·Alt+↑/↓ · 범위 Shift+↑/↓ · 이동 Alt+Shift+↑/↓\n'
            '접기/펼치기 Ctrl+↑/↓·Ctrl+; · TODO Ctrl+Enter · 한 블록에 붙여넣기 Ctrl+Shift+V',
        child: Icon(Icons.keyboard_outlined, size: 18),
      ),
      // 확정 전 글만 되돌려야 하므로 누를 때 칸을 벗어나 저장되지 않게 한다.
      keepEditorFocus(
        IconButton(
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
      keepEditorFocus(
        IconButton(
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
        onPressed: _page == null || _locked
            ? null
            : () => _after(_renamePage),
        icon: const Icon(Icons.drive_file_rename_outline),
      ),
      IconButton(
        tooltip: '페이지 삭제',
        onPressed: _page == null || _locked
            ? null
            : () => _after(_deletePage),
        icon: const Icon(Icons.delete_outline),
      ),
      IconButton(
        tooltip: '새 블록',
        onPressed: _blocks.isEmpty || _locked
            ? null
            : () => _after(() {
                if (_blocks.isNotEmpty) _edit(_blocks.last, 'insert', '');
              }),
        icon: const Icon(Icons.add),
      ),
      ...widget.headerActions,
    ];
    return LayoutBuilder(
      builder: (_, size) => size.maxWidth < 460
          ? Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                title,
                Wrap(
                  alignment: WrapAlignment.end,
                  crossAxisAlignment: WrapCrossAlignment.center,
                  children: actions,
                ),
              ],
            )
          : Row(children: [Expanded(child: title), ...actions]),
    );
  }

  Widget _buildPanels(double width) {
    final c = context.colors;
    final highlighted = _highlighted();
    final candidates = _candidates();
    final leftCollapsed = _collapsed('left');
    final rightCollapsed = _rightCollapsed(width);
    // 블록 줄이 쓸 수 있는 폭(들여쓰기 전): 본문 폭에서 목록 좌우 여백을 뺀다.
    final rowSpace =
        width -
        (leftCollapsed ? _stripWidth : _leftWidth) -
        (rightCollapsed ? _stripWidth : _rightWidth) -
        2 -
        48;
    return Row(
      children: [
        SizedBox(
          width: leftCollapsed ? _stripWidth : _leftWidth,
          child: Material(
            color: c.surface,
            child: leftCollapsed
                ? _strip(_panelToggle('left', '왼쪽', true, false))
                : Column(
              children: [
                Padding(
                  padding: const EdgeInsets.fromLTRB(8, 12, 2, 12),
                  child: Row(
                    children: [
                      Expanded(
                        child: TextField(
                          controller: _search,
                          decoration: const InputDecoration(
                            prefixIcon: Icon(Icons.search, size: 20),
                            // 접기 단추 자리만큼 좁아진 칸에도 안내 문구가 다 보이게 한다.
                            prefixIconConstraints: BoxConstraints(minWidth: 32),
                            contentPadding: EdgeInsets.symmetric(
                              horizontal: 4,
                              vertical: 14,
                            ),
                            hintText: '페이지·블록 검색',
                            border: OutlineInputBorder(),
                          ),
                          onChanged: (value) => setState(
                            () => _pageQuery = value.trim().toLowerCase(),
                          ),
                          onSubmitted: _searchBlocks,
                        ),
                      ),
                      _panelToggle('left', '왼쪽', false, false),
                    ],
                  ),
                ),
                ListTile(
                  leading: const Icon(Icons.today_outlined),
                  title: const Text('오늘의 일지'),
                  onTap: _locked ? null : () => _after(_openToday),
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
                        onPressed: _locked ? null : () => _after(_newPage),
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
                    onTap: _locked ? null : () => _after(() => _open(name)),
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
                          onTap: _locked
                              ? null
                              : () => _after(() => _open('${p['name']}')),
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
          key: const ValueKey('outliner-body'),
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
                child: _header(c),
              ),
              if (widget.notice.isNotEmpty)
                Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 24),
                  child: Text(
                    widget.notice,
                    style: TextStyle(
                      color: Theme.of(context).colorScheme.error,
                    ),
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
                              onPressed: _locked
                                  ? null
                                  : () => _onRow(
                                      block['id'],
                                      (b) => _key('toggle_collapse', block: b),
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
                                  final text = '${block['text'] ?? ''}';
                                  final controller = _editors.putIfAbsent(id, () {
                                    _shown[id] = text;
                                    return TextEditingController(text: text);
                                  });
                                  final focus = _focusNodes.putIfAbsent(id, () {
                                    final node = FocusNode();
                                    node.addListener(
                                      () => _focusChanged(id, node),
                                    );
                                    return node;
                                  });
                                  // 문서 글이 바뀐 칸만 맞춘다. 저장하지 않은 입력,
                                  // 처리 중에 친 글, 곧 커서가 올 칸은 덮지 않는다.
                                  if (controller.text == text) {
                                    _shown[id] = text;
                                  } else if (!focus.hasFocus &&
                                      id != _pendingFocus &&
                                      !(_busy && id == _base?.id) &&
                                      _shown[id] != text) {
                                    _shown[id] = text;
                                    controller.text = text;
                                  }
                                  return Focus(
                                    onKeyEvent: (node, event) => _handleKey(
                                      block,
                                      controller,
                                      focus,
                                      event,
                                    ),
                                    // 처리 중의 클릭은 커서를 옮겨 친 글의 자리를 흐트러뜨린다.
                                    // 무시한 클릭이 '칸 바깥 클릭'이 되어 포커스가 풀리지 않게 영역은 남긴다.
                                    child: TapRegion(
                                      groupId: EditableText,
                                      behavior: HitTestBehavior.opaque,
                                      child: IgnorePointer(
                                        ignoring: _busy,
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
                              onPressed: _locked
                                  ? null
                                  : () => _onRow(
                                      block['id'],
                                      (b) => _setTextAndEdit(
                                        b,
                                        'toggle_status',
                                        _editors[b['id']]?.text ??
                                            '${b['text'] ?? ''}',
                                      ),
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
                            // 아주 좁은 줄에서는 글 칸을 남긴다(Tab·Shift+Tab 은 그대로).
                            if (rowSpace - (block['indent'] as int? ?? 0) * 24 >=
                                _minRowForIndentButtons) ...[
                            IconButton(
                              tooltip: '들여쓰기',
                              onPressed: _locked
                                  ? null
                                  : () => _onRow(
                                      block['id'],
                                      (b) => _key('indent', block: b),
                                    ),
                              icon: const Icon(Icons.keyboard_tab, size: 18),
                            ),
                            IconButton(
                              tooltip: '내어쓰기',
                              onPressed: _locked
                                  ? null
                                  : () => _onRow(
                                      block['id'],
                                      (b) => _key('outdent', block: b),
                                    ),
                              icon: const Icon(
                                Icons.format_indent_decrease,
                                size: 18,
                              ),
                            ),
                            ],
                          ],
                          ),
                        ),
                      ),
                    for (final (i, candidate) in candidates.indexed)
                      keepEditorFocus(
                        ListTile(
                          dense: true,
                          selected: i == _popupIndex,
                          leading: Icon(
                            _popupOpen == '[[' ? Icons.link : Icons.tag,
                          ),
                          title: Text(candidate.$1),
                          onTap: () => _acceptPopup(candidate.$2),
                        ),
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
                                  onPressed: _locked
                                      ? null
                                      : () => _after(() => _open('$link')),
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
        VerticalDivider(width: 1, color: c.line),
        SizedBox(
          width: rightCollapsed ? _stripWidth : _rightWidth,
          child: Stack(
            fit: StackFit.expand,
            children: [
              // 접어도 트리에 남긴다. 조회 시점은 펼쳤을 때와 같고, 펴면 바로 보인다.
              Offstage(
                offstage: rightCollapsed,
                // 띠 폭으로 눌러 배치하면 넘치므로 숨긴 동안에도 원래 폭으로 둔다.
                child: OverflowBox(
                  alignment: Alignment.topLeft,
                  minWidth: _rightWidth,
                  maxWidth: _rightWidth,
                  child: _BacklinksPanel(
                    api: widget.api,
                    page: _page,
                    markdown: _markdown,
                    selectedBlockId: _selectedIds.firstOrNull,
                    onOpen: (name) => _after(() => _open(name)),
                    toggle: _panelToggle('right', '오른쪽', false, true),
                    backlinksCollapsed: _collapsed('backlinks'),
                    blockRefsCollapsed: _collapsed('blockRefs'),
                    onCollapsed: _setCollapsed,
                  ),
                ),
              ),
              if (rightCollapsed)
                _strip(_panelToggle('right', '오른쪽', true, true)),
            ],
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
    required this.toggle,
    required this.backlinksCollapsed,
    required this.blockRefsCollapsed,
    required this.onCollapsed,
  });
  final ApiCall api;
  final String? page;
  final String markdown;
  final Object? selectedBlockId;
  final ValueChanged<String> onOpen;
  final Widget toggle; // 패널 전체 접기 단추
  final bool backlinksCollapsed;
  final bool blockRefsCollapsed;
  final void Function(String key, bool collapsed) onCollapsed;
  @override
  State<_BacklinksPanel> createState() => _BacklinksPanelState();
}

class _BacklinksPanelState extends State<_BacklinksPanel> {
  List<dynamic> _ids = [];
  List<dynamic> _blockRefIds = [];
  // 위 목록을 읽어 온 대상. 그 대상을 실제로 읽었을 때만 제목에 개수를 보인다.
  String? _idsPage;
  Object? _refsBlock;

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
    final page = widget.page;
    final block = widget.selectedBlockId;
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
        setState(() {
          _ids = data is Map ? (data['block_ids'] as List? ?? []) : [];
          _idsPage = data is Map ? page : null;
        });
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
          setState(() {
            _blockRefIds =
                data is Map ? (data['backlinks'] as List? ?? []) : [];
            _refsBlock = data is Map ? block : null;
          });
        }
      } catch (_) {}
    } else {
      if (mounted) {
        setState(() {
          _blockRefIds = [];
          _refsBlock = null;
        });
      }
    }
  }

  /// 눌러서 접고 펴는 제목. [count] 는 이미 읽은 값이 있을 때만 준다.
  Widget _heading(String key, String title, bool collapsed, int? count) {
    final style = Theme.of(context).textTheme.titleSmall;
    return keepEditorFocus(
      InkWell(
        key: ValueKey('outliner-$key-header'),
        onTap: () => widget.onCollapsed(key, !collapsed),
        child: Padding(
          padding: const EdgeInsets.symmetric(vertical: 4),
          child: Row(
            children: [
              Icon(
                collapsed ? Icons.chevron_right : Icons.expand_more,
                size: 18,
              ),
              Flexible(child: Text(title, style: style)),
              if (count != null) ...[
                const SizedBox(width: 6),
                Text(
                  '$count',
                  key: ValueKey('outliner-$key-count'),
                  style: style?.copyWith(color: context.colors.muted),
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final selected = widget.selectedBlockId;
    return SingleChildScrollView(
      padding: const EdgeInsets.all(12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Align(alignment: Alignment.centerRight, child: widget.toggle),
          _heading(
            'backlinks',
            '역링크',
            widget.backlinksCollapsed,
            widget.page != null && _idsPage == widget.page ? _ids.length : null,
          ),
          if (!widget.backlinksCollapsed) ...[
            const SizedBox(height: 6),
            if (_ids.isEmpty)
              const Text('참조가 없습니다.', style: TextStyle(fontSize: 12))
            else
              for (final id in _ids)
                Text('블록 $id', style: const TextStyle(fontSize: 12)),
          ],
          const SizedBox(height: 16),
          _heading(
            'blockRefs',
            '역참조 (선택 블록)',
            widget.blockRefsCollapsed,
            selected != null && _refsBlock == selected
                ? _blockRefIds.length
                : null,
          ),
          if (!widget.blockRefsCollapsed) ...[
            const SizedBox(height: 6),
            if (selected == null)
              const Text('선택된 블록이 없습니다.', style: TextStyle(fontSize: 12))
            else if (_blockRefIds.isEmpty)
              const Text(
                '이 블록을 참조하는 블록이 없습니다.',
                style: TextStyle(fontSize: 12),
              )
            else
              for (final id in _blockRefIds)
                Text('블록 $id 에서 참조', style: const TextStyle(fontSize: 12)),
          ],
        ],
      ),
    );
  }
}
