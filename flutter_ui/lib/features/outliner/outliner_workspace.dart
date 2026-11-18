import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../core/api/abbreviation_api.dart' show ApiCall;
import '../../core/theme/app_theme.dart';

class OutlinerWorkspace extends StatefulWidget {
  const OutlinerWorkspace({super.key, required this.api});
  final ApiCall api;

  @override
  State<OutlinerWorkspace> createState() => _OutlinerWorkspaceState();
}

class _OutlinerWorkspaceState extends State<OutlinerWorkspace> {
  List<Map> _pages = [];
  List<Map> _blocks = [];
  String? _page;
  String _markdown = '';
  String? _version;
  String _message = '';
  bool _busy = false;
  String _pageQuery = '';
  final _search = TextEditingController();

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
        setState(
          () => _pages = (result['pages'] as List? ?? [])
              .whereType<Map>()
              .toList(),
        );
      }
      if (_page == null) await _openToday();
    } catch (e) {
      if (mounted) setState(() => _message = '$e');
    }
  }

  Future<void> _openToday() async {
    await _run(() async {
      final result = await _call({'action': 'daily'});
      _applyDocument(result);
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
  });

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
                              child: Focus(
                                onKeyEvent: (node, event) {
                                  if (event is! KeyDownEvent ||
                                      event.logicalKey !=
                                          LogicalKeyboardKey.tab) {
                                    return KeyEventResult.ignored;
                                  }
                                  _edit(
                                    block,
                                    HardwareKeyboard.instance.isShiftPressed
                                        ? 'outdent'
                                        : 'indent',
                                  );
                                  return KeyEventResult.handled;
                                },
                                child: TextFormField(
                                  key: ValueKey(
                                    'outliner-block-${block['id']}',
                                  ),
                                  initialValue: '${block['text'] ?? ''}',
                                  decoration: const InputDecoration(
                                    border: InputBorder.none,
                                    isDense: true,
                                  ),
                                  onFieldSubmitted: _busy
                                      ? null
                                      : (value) =>
                                            _commitAndInsert(block, value),
                                ),
                              ),
                            ),
                            if ((block['status']) == 'TODO')
                              const Icon(
                                Icons.check_box_outline_blank,
                                size: 16,
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
