import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../core/api/abbreviation_api.dart' show ApiCall;
import '../../core/theme/app_theme.dart';

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
      await _save();
      if (_page != null) await _rememberPage(_page!);
    });
  }

  Future<void> _commitAndInsert(Map block, String text) async {
    if (_page == null || _busy) return;
    await _run(() async {
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
      await _save();
      if (_page != null) await _rememberPage(_page!);
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
      await _save();
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
                                      if (key == LogicalKeyboardKey.tab) {
                                        _edit(
                                          block,
                                          mods.isShiftPressed
                                              ? 'outdent'
                                              : 'indent',
                                        );
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
                                      if (mods.isAltPressed &&
                                          (key == LogicalKeyboardKey.arrowUp ||
                                              key ==
                                                  LogicalKeyboardKey
                                                      .arrowDown)) {
                                        _setTextAndEdit(
                                          block,
                                          key == LogicalKeyboardKey.arrowUp
                                              ? 'up'
                                              : 'down',
                                          value,
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
                                              await _save();
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
