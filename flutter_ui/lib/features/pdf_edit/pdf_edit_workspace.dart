import 'dart:convert';
import 'package:flutter/services.dart';

import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';

import '../../core/api/abbreviation_api.dart' show ApiCall;
import '../../core/drop_events.dart';
import '../../core/theme/app_theme.dart';

class PdfEditWorkspace extends StatefulWidget {
  const PdfEditWorkspace({super.key, required this.api});
  final ApiCall api;

  @override
  State<PdfEditWorkspace> createState() => _PdfEditWorkspaceState();
}

class _PdfEditWorkspaceState extends State<PdfEditWorkspace> {
  static const _pdfType = XTypeGroup(label: 'PDF', extensions: ['pdf']);
  static const _tools = [
    ('delete', '페이지 삭제', Icons.delete_outline),
    ('rotate', '회전', Icons.rotate_right),
    ('split', '분할', Icons.call_split),
    ('merge', '합치기', Icons.call_merge),
    ('reorder', '순서 변경', Icons.swap_vert),
  ];
  final _pages = TextEditingController();
  final List<String> _mergeSources = [];
  String? _source;
  String? _output;
  String _action = 'delete';
  int _degrees = 90;
  String _message = '';
  bool _busy = false;
  int? _pageCount;
  final Map<int, Uint8List> _thumbnails = {};
  final Set<int> _thumbnailRequests = {};
  final List<int> _pageOrder = [];
  final Set<int> _selectedPages = {};
  final List<_EditOperation> _operations = [];
  final List<_EditOperation> _redo = [];
  int? _lastSelected;
  List<int> _initialOrder = [];
  String? _completedPath;
  bool _dragActive = false;
  void Function()? _stopDropWatch;

  @override
  void initState() {
    super.initState();
    _stopDropWatch = watchDocumentDrops(_onDocumentDrop);
  }

  @override
  void dispose() {
    _stopDropWatch?.call();
    _pages.dispose();
    super.dispose();
  }

  void _onDocumentDrop() {
    if (mounted && !_busy) _pickSource();
  }

  String _outputFor(String path, [String suffix = '_편집']) =>
      path.replaceFirst(RegExp(r'\.pdf$', caseSensitive: false), '$suffix.pdf');

  List<int>? _parsePages() {
    final pages = <int>[];
    for (final token in _pages.text.split(',')) {
      final range = token.trim().split('-');
      final start = int.tryParse(range.first.trim());
      if (start == null || start < 1 || range.length > 2) return null;
      if (range.length == 1) {
        pages.add(start);
        continue;
      }
      final end = int.tryParse(range.last.trim());
      if (end == null || end < start) return null;
      pages.addAll(List<int>.generate(end - start + 1, (i) => start + i));
    }
    final unique = pages.toSet().toList()..sort();
    return unique.isEmpty ? null : unique;
  }

  Future<void> _pickSource() async {
    final file = await openFile(acceptedTypeGroups: [_pdfType]);
    if (file != null) _addFile(file.path);
  }

  void _addFile(String path) {
    if (!path.toLowerCase().endsWith('.pdf')) return;
    setState(() {
      _source = path;
      _pages.clear();
      if (_action == 'merge' && !_mergeSources.contains(path)) {
        _mergeSources.add(path);
      }
      _output = _outputFor(path, _action == 'merge' ? '_합본' : '_편집');
      _message = '';
      _pageCount = null;
      _pageOrder.clear();
      _selectedPages.clear();
      _operations.clear();
      _redo.clear();
      _initialOrder = [];
      _completedPath = null;
      _thumbnails.clear();
      _thumbnailRequests.clear();
      _dragActive = false;
    });
    _loadPages(path);
  }

  Future<void> _loadPages(String path) async {
    try {
      final response = await widget.api('pdf_edit', [
        {'action': 'info', 'source': path},
      ]);
      final envelope = response is Map ? response : const {};
      if (envelope['ok'] != true) return;
      final data = envelope['data'];
      final pages = data is Map && data['pages'] is List
          ? data['pages'] as List
          : const [];
      if (!mounted || _source != path) return;
      setState(() {
        _pageCount = pages.length;
        _pageOrder
          ..clear()
          ..addAll(List<int>.generate(pages.length, (i) => i + 1));
        _initialOrder = List.of(_pageOrder);
      });
    } catch (_) {
      /* Keep numbered cards when metadata is unavailable. */
    }
  }

  Future<void> _loadThumbnail(String path, int page) async {
    if (!_thumbnailRequests.add(page)) return;
    try {
      final response = await widget.api('pdf_edit', [
        {
          'action': 'thumbnail',
          'source': path,
          'pages': [page],
        },
      ]);
      final envelope = response is Map ? response : const {};
      final data = envelope['data'];
      if (data is! Map ||
          data['data'] is! String ||
          !mounted ||
          _source != path) {
        return;
      }
      setState(() => _thumbnails[page] = base64Decode(data['data'] as String));
    } catch (_) {
      /* Keep the numbered card when rendering is unavailable. */
    } finally {
      _thumbnailRequests.remove(page);
    }
  }

  Future<void> _pickMergeSources() async {
    final files = await openFiles(acceptedTypeGroups: [_pdfType]);
    if (files.length < 2 || !mounted) return;
    setState(() {
      _mergeSources
        ..clear()
        ..addAll(files.map((file) => file.path));
      _source = files.first.path;
      _output = _outputFor(files.first.path, '_합본');
      _message = '';
    });
  }

  Future<void> _pickOutput() async {
    if (_action != 'merge' && _source == null) return;
    final location = await getSaveLocation(
      suggestedName: _output?.split(RegExp(r'[/\\]')).last ?? '편집본.pdf',
      acceptedTypeGroups: [_pdfType],
    );
    if (location != null && mounted) setState(() => _output = location.path);
  }

  bool get _canRun =>
      !_busy &&
      _output != null &&
      (_action == 'merge'
          ? _mergeSources.length >= 2
          : _source != null &&
                (_action == 'split'
                    ? _parsePages() != null
                    : _operations.isNotEmpty ||
                          _selectedPages.isNotEmpty ||
                          _parsePages() != null));

  Future<void> _run() async {
    if (!_canRun) return;
    final entered = _parsePages();
    final pages = _selectedPages.isNotEmpty && _action != 'split'
        ? (_selectedPages.toList()..sort())
        : entered ?? <int>[];
    if (_action != 'merge' &&
        (_action == 'split' && entered == null ||
            _action != 'split' &&
                _operations.isEmpty &&
                _selectedPages.isEmpty &&
                entered == null)) {
      setState(() => _message = '페이지 번호를 쉼표로 구분해 입력해 주세요.');
      return;
    }
    setState(() {
      _busy = true;
      _message = '';
      _completedPath = null;
    });
    final intermediates = <String>[];
    try {
      var current = _source!;
      Object? response;
      final queue = _action == 'merge'
          ? <_EditOperation>[]
          : List<_EditOperation>.of(_operations);
      if (_action == 'merge') {
        response = await widget.api('pdf_edit', [
          {'action': 'merge', 'sources': _mergeSources, 'output': _output},
        ]);
        final env = response is Map ? response : const {};
        if (env['ok'] != true) {
          throw Exception(
            (env['error'] as Map?)?['message'] ?? 'PDF 합치기에 실패했습니다.',
          );
        }
      } else {
        if (_action == 'split') {
          queue.add(_EditOperation('split', pages));
        } else if (_operations.isEmpty || _selectedPages.isNotEmpty) {
          final selected = _selectedPages.isNotEmpty
              ? (_selectedPages.toList()..sort())
              : pages;
          if (_action == 'delete') {
            queue.add(_EditOperation('delete', selected));
          }
          if (_action == 'rotate') {
            queue.add(_EditOperation('rotate', selected, degrees: _degrees));
          }
          if (_action == 'reorder') {
            queue.add(_EditOperation('reorder', List<int>.of(_pageOrder)));
          }
        }
        var currentIds = List<int>.of(_initialOrder);
        for (var i = 0; i < queue.length; i++) {
          final op = queue[i];
          final last = i == queue.length - 1;
          final finalOutput = _action == 'split'
              ? _output!.replaceFirst(
                  RegExp(r'\.pdf$', caseSensitive: false),
                  '',
                )
              : _output!;
          final target = last ? finalOutput : _intermediatePath(i);
          if (!last) intermediates.add(target);
          List<int> enginePages;
          if (op.action == 'reorder') {
            final reorderedIds = op.pages;
            if (reorderedIds.length != currentIds.length ||
                reorderedIds.toSet().length != currentIds.length ||
                !reorderedIds.toSet().containsAll(currentIds)) {
              throw Exception('쪽 순서 변경 목록이 현재 문서와 일치하지 않습니다.');
            }
            enginePages = reorderedIds
                .map((id) => currentIds.indexOf(id) + 1)
                .toList();
            currentIds = List<int>.of(reorderedIds);
          } else {
            enginePages =
                op.pages
                    .map((id) => currentIds.indexOf(id) + 1)
                    .where((page) => page > 0)
                    .toList()
                  ..sort();
            if (enginePages.isEmpty) throw Exception('작업 대상 쪽이 현재 문서에 없습니다.');
            if (op.action == 'delete') {
              currentIds.removeWhere(op.pages.toSet().contains);
            }
          }
          response = await widget.api('pdf_edit', [
            {
              'action': op.action,
              'source': current,
              'output': target,
              'pages': enginePages,
              'degrees': op.degrees,
            },
          ]);
          final env = response is Map ? response : const {};
          if (env['ok'] != true) {
            throw Exception(
              (env['error'] as Map?)?['message'] ?? 'PDF 편집에 실패했습니다.',
            );
          }
          current = target;
        }
        if (queue.isEmpty && _action != 'split') {
          throw Exception('저장할 변경 사항이 없습니다.');
        }
      }
      if (!mounted) return;
      final envelope = response is Map ? response : const {};
      final error = envelope['error'];
      final data = envelope['data'];
      setState(() {
        _completedPath = data is Map ? '${data['output'] ?? _output}' : _output;
        _message = envelope['ok'] == true
            ? '완료: $_completedPath'
            : (error is Map
                  ? '${error['message'] ?? 'PDF 편집에 실패했습니다.'}'
                  : 'PDF 편집에 실패했습니다.');
      });
    } catch (error) {
      if (mounted) setState(() => _message = 'PDF 편집 요청에 실패했습니다: $error');
    } finally {
      if (intermediates.isNotEmpty) {
        try {
          await widget.api('pdf_edit', [
            {'action': 'cleanup', 'sources': intermediates},
          ]);
        } catch (_) {
          /* Keep cleanup errors from replacing the edit result. */
        }
      }
      if (mounted) setState(() => _busy = false);
    }
  }

  String _intermediatePath(int index) {
    final path = _output!;
    final separator = path.lastIndexOf('/') > path.lastIndexOf('\\')
        ? '/'
        : '\\';
    final split = path.lastIndexOf(separator);
    return '${split < 0 ? '' : path.substring(0, split + 1)}.__pdfui_${DateTime.now().microsecondsSinceEpoch}_$index.pdf';
  }

  void _record(_EditOperation op) {
    setState(() {
      _operations.add(op);
      _redo.clear();
      _completedPath = null;
      _message = '';
    });
  }

  void _rotateSelected(int degrees) {
    if (_selectedPages.isNotEmpty) {
      _record(
        _EditOperation(
          'rotate',
          _selectedPages.toList()..sort(),
          degrees: degrees,
        ),
      );
    }
  }

  void _deleteSelected() {
    if (_selectedPages.isEmpty) return;
    final pages = _selectedPages.toList()..sort();
    _record(_EditOperation('delete', pages));
    setState(() {
      _pageOrder.removeWhere(pages.contains);
      _selectedPages.clear();
    });
  }

  void _rebuildPageOrder() {
    _pageOrder
      ..clear()
      ..addAll(_initialOrder);
    for (final op in _operations) {
      if (op.action == 'delete') {
        _pageOrder.removeWhere(op.pages.toSet().contains);
      }
      if (op.action == 'reorder') {
        _pageOrder
          ..clear()
          ..addAll(op.pages);
      }
    }
    _selectedPages.removeWhere((page) => !_pageOrder.contains(page));
  }

  void _undo() {
    if (_operations.isEmpty) return;
    setState(() {
      _redo.add(_operations.removeLast());
      _rebuildPageOrder();
    });
  }

  void _redoEdit() {
    if (_redo.isEmpty) return;
    setState(() {
      _operations.add(_redo.removeLast());
      _rebuildPageOrder();
    });
  }

  void _selectPage(int page, {bool shift = false, bool control = false}) {
    setState(() {
      if (shift && _lastSelected != null) {
        final a = _pageOrder.indexOf(_lastSelected!),
            b = _pageOrder.indexOf(page);
        if (a >= 0 && b >= 0) {
          _selectedPages.addAll(
            _pageOrder.sublist(a < b ? a : b, (a > b ? a : b) + 1),
          );
        }
      } else if (control) {
        _selectedPages.contains(page)
            ? _selectedPages.remove(page)
            : _selectedPages.add(page);
      } else {
        _selectedPages
          ..clear()
          ..add(page);
      }
      _lastSelected = page;
    });
  }

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return LayoutBuilder(
      builder: (context, constraints) {
        final compact = constraints.maxWidth < 900;
        return Padding(
          padding: EdgeInsets.all(compact ? 16 : 24),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          'PDF 편집',
                          style: Theme.of(context).textTheme.headlineSmall
                              ?.copyWith(
                                color: c.ink,
                                fontWeight: FontWeight.w700,
                              ),
                        ),
                        const SizedBox(height: 4),
                        Text(
                          '페이지를 정리하고 새 PDF로 저장합니다.',
                          style: TextStyle(color: c.muted),
                        ),
                      ],
                    ),
                  ),
                  OutlinedButton.icon(
                    key: const Key('pdfEditPickSource'),
                    onPressed: _busy ? null : _pickSource,
                    icon: const Icon(Icons.add),
                    label: const Text('PDF 추가'),
                  ),
                ],
              ),
              const SizedBox(height: 20),
              Wrap(
                spacing: 8,
                runSpacing: 8,
                children: [
                  for (final tool in _tools)
                    _ToolChip(
                      label: tool.$2,
                      icon: tool.$3,
                      selected: _action == tool.$1,
                      onTap: _busy
                          ? null
                          : () => setState(() {
                              _action = tool.$1;
                              if (tool.$1 == 'merge') {
                                _output = _source == null
                                    ? null
                                    : _outputFor(_source!, '_합본');
                              } else if (tool.$1 == 'split' &&
                                  _source != null) {
                                _output = _outputFor(_source!, '_분할');
                              } else if (_source != null) {
                                _output = _outputFor(_source!, '_편집');
                              }
                            }),
                    ),
                ],
              ),
              const SizedBox(height: 16),
              Expanded(
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    Expanded(
                      child: _source == null ? _dropZone(c) : _editor(c),
                    ),
                    if (!compact) ...[
                      const SizedBox(width: 16),
                      SizedBox(width: 284, child: _sidePanel(c)),
                    ],
                  ],
                ),
              ),
              if (compact && _source != null) ...[
                const SizedBox(height: 12),
                _sidePanel(c),
              ],
            ],
          ),
        );
      },
    );
  }

  Widget _dropZone(AppColors c) => DragTarget<String>(
    onWillAcceptWithDetails: (_) {
      setState(() => _dragActive = true);
      return true;
    },
    onLeave: (_) => setState(() => _dragActive = false),
    onAcceptWithDetails: (details) => _addFile(details.data),
    builder: (context, candidates, rejected) => Center(
      child: InkWell(
        key: const Key('pdfEditDropZone'),
        onTap: _pickSource,
        borderRadius: BorderRadius.circular(8),
        child: AnimatedContainer(
          duration: const Duration(milliseconds: 140),
          width: double.infinity,
          constraints: const BoxConstraints(maxWidth: 720, maxHeight: 340),
          decoration: BoxDecoration(
            color: _dragActive ? c.accentSoft : c.surface,
            borderRadius: BorderRadius.circular(8),
            border: Border.all(
              color: _dragActive ? c.accent : c.lineStrong,
              width: _dragActive ? 2 : 1,
            ),
          ),
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              Icon(Icons.upload_file_outlined, size: 38, color: c.accent),
              const SizedBox(height: 14),
              Text(
                'PDF를 여기로 끌어다 놓거나 클릭해서 선택',
                textAlign: TextAlign.center,
                style: TextStyle(
                  color: c.textStrong,
                  fontSize: 17,
                  fontWeight: FontWeight.w600,
                ),
              ),
              const SizedBox(height: 6),
              Text('PDF 파일 한 개를 추가할 수 있습니다.', style: TextStyle(color: c.muted)),
              const SizedBox(height: 18),
              OutlinedButton.icon(
                onPressed: _pickSource,
                icon: const Icon(Icons.folder_open_outlined),
                label: const Text('파일 선택'),
              ),
            ],
          ),
        ),
      ),
    ),
  );

  Widget _editor(AppColors c) => Column(
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      _fileCard(c),
      const SizedBox(height: 14),
      Expanded(
        child: DecoratedBox(
          decoration: BoxDecoration(
            color: c.surface,
            border: Border.all(color: c.line),
            borderRadius: BorderRadius.circular(8),
          ),
          child: _action == 'merge' ? _mergeEditor(c) : _pageGrid(c),
        ),
      ),
    ],
  );

  Widget _fileCard(AppColors c) => Container(
    key: const Key('pdfEditFileCard'),
    padding: const EdgeInsets.all(14),
    decoration: BoxDecoration(
      color: c.surface,
      border: Border.all(color: c.line),
      borderRadius: BorderRadius.circular(8),
    ),
    child: Row(
      children: [
        Icon(Icons.picture_as_pdf_outlined, color: c.accent),
        const SizedBox(width: 12),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                _source!.split(RegExp(r'[/\\]')).last,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                  color: c.textStrong,
                  fontWeight: FontWeight.w600,
                ),
              ),
              Text(
                '쪽 수는 파일을 실행할 때 확인됩니다.',
                style: TextStyle(color: c.muted, fontSize: 12),
              ),
            ],
          ),
        ),
        IconButton(
          tooltip: '파일 제거',
          onPressed: _busy
              ? null
              : () => setState(() {
                  _source = null;
                  _output = null;
                  _mergeSources.clear();
                }),
          icon: const Icon(Icons.close),
          color: c.muted,
        ),
      ],
    ),
  );

  Widget _pageGrid(AppColors c) => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      Padding(
        padding: const EdgeInsets.fromLTRB(16, 14, 16, 8),
        child: Row(
          children: [
            Text(
              '쪽 미리보기',
              style: TextStyle(
                color: c.textStrong,
                fontWeight: FontWeight.w600,
              ),
            ),
            const SizedBox(width: 8),
            Expanded(
              child: Text(
                '페이지 번호를 입력해 작업할 쪽을 지정하세요.',
                style: TextStyle(color: c.muted, fontSize: 12),
              ),
            ),
            TextButton(
              onPressed: _busy
                  ? null
                  : () => setState(() {
                      if (_pageOrder.isNotEmpty &&
                          _selectedPages.length == _pageOrder.length) {
                        _selectedPages.clear();
                      } else {
                        _selectedPages
                          ..clear()
                          ..addAll(_pageOrder);
                      }
                    }),
              child: Text(
                _selectedPages.length == _pageOrder.length &&
                        _pageOrder.isNotEmpty
                    ? '선택 해제'
                    : '전체 선택',
              ),
            ),
          ],
        ),
      ),
      Expanded(
        child: GridView.builder(
          key: const Key('pdfEditThumbnailGrid'),
          padding: const EdgeInsets.all(16),
          gridDelegate: const SliverGridDelegateWithMaxCrossAxisExtent(
            maxCrossAxisExtent: 150,
            mainAxisExtent: 178,
            crossAxisSpacing: 12,
            mainAxisSpacing: 12,
          ),
          itemCount: _pageCount == null ? 12 : _pageOrder.length,
          itemBuilder: (context, i) {
            final number = i < _pageOrder.length ? _pageOrder[i] : i + 1;
            if (_pageCount != null &&
                _source != null &&
                !_thumbnails.containsKey(number)) {
              _loadThumbnail(_source!, number);
            }
            return _PageCard(
              key: ValueKey(number),
              number: number,
              c: c,
              thumbnail: _thumbnails[number],
              selected: _selectedPages.contains(number),
              onTap: (shift, control) =>
                  _selectPage(number, shift: shift, control: control),
              onRotateLeft: () => _rotateSelected(-90),
              onRotateRight: () => _rotateSelected(90),
              onDelete: _deleteSelected,
              onMove: (target) {
                final from = _pageOrder.indexOf(number),
                    to = _pageOrder.indexOf(target);
                if (from < 0 || to < 0) return;
                final order = List<int>.of(_pageOrder)
                  ..removeAt(from)
                  ..insert(to, number);
                _record(_EditOperation('reorder', order));
                setState(() {
                  _pageOrder
                    ..clear()
                    ..addAll(order);
                });
              },
              onKey: (delta) {
                final from = _pageOrder.indexOf(number), to = from + delta;
                if (from < 0 || to < 0 || to >= _pageOrder.length) return;
                final order = List<int>.of(_pageOrder)
                  ..removeAt(from)
                  ..insert(to, number);
                _record(_EditOperation('reorder', order));
                setState(() {
                  _pageOrder
                    ..clear()
                    ..addAll(order);
                });
              },
            );
          },
        ),
      ),
      Padding(
        padding: const EdgeInsets.fromLTRB(16, 0, 16, 14),
        child: Row(
          children: [
            Expanded(
              child: TextField(
                key: const Key('pdfEditPages'),
                controller: _pages,
                decoration: InputDecoration(
                  isDense: true,
                  labelText: _action == 'split' ? '분할할 페이지' : '페이지 번호',
                  helperText: _action == 'split'
                      ? '예: 1-3, 5 — 선택한 쪽을 별도 PDF로 저장'
                      : '쉼표로 구분 · 순서 변경은 카드를 끌어 놓으세요',
                ),
              ),
            ),
            if (_action == 'rotate') ...[
              const SizedBox(width: 12),
              DropdownButton<int>(
                value: _degrees,
                items: const [90, 180, 270]
                    .map((n) => DropdownMenuItem(value: n, child: Text('$n°')))
                    .toList(),
                onChanged: _busy
                    ? null
                    : (v) => setState(() => _degrees = v ?? 90),
              ),
            ],
          ],
        ),
      ),
    ],
  );

  Widget _mergeEditor(AppColors c) => Padding(
    padding: const EdgeInsets.all(16),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          '합칠 PDF',
          style: TextStyle(color: c.textStrong, fontWeight: FontWeight.w600),
        ),
        const SizedBox(height: 8),
        Text(
          '두 개 이상의 PDF를 선택하고 합칠 순서를 확인하세요.',
          style: TextStyle(color: c.muted),
        ),
        const SizedBox(height: 12),
        OutlinedButton.icon(
          key: const Key('pdfEditPickMergeSources'),
          onPressed: _busy ? null : _pickMergeSources,
          icon: const Icon(Icons.library_add_outlined),
          label: const Text('PDF 여러 개 선택'),
        ),
        const SizedBox(height: 8),
        Expanded(
          child: ReorderableListView(
            children: [
              for (var i = 0; i < _mergeSources.length; i++)
                ListTile(
                  key: ValueKey(_mergeSources[i]),
                  dense: true,
                  leading: Text('${i + 1}'),
                  title: Text(
                    _mergeSources[i].split(RegExp(r'[/\\]')).last,
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
            ],
            onReorder: (a, b) => setState(() {
              if (b > a) b--;
              final item = _mergeSources.removeAt(a);
              _mergeSources.insert(b, item);
            }),
          ),
        ),
      ],
    ),
  );

  Widget _sidePanel(AppColors c) => Container(
    padding: const EdgeInsets.all(16),
    decoration: BoxDecoration(
      color: c.surface,
      border: Border.all(color: c.line),
      borderRadius: BorderRadius.circular(8),
    ),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      children: [
        Text(
          '작업 설정',
          style: TextStyle(color: c.textStrong, fontWeight: FontWeight.w600),
        ),
        const SizedBox(height: 12),
        Text(
          _action == 'merge' ? '입력 파일' : '대상 페이지',
          style: TextStyle(color: c.muted, fontSize: 12),
        ),
        Text(
          _action == 'merge'
              ? '${_mergeSources.length}개 선택'
              : _pages.text.isEmpty
              ? '선택 없음'
              : _pages.text,
          style: TextStyle(color: c.text),
        ),
        const SizedBox(height: 14),
        OutlinedButton(
          key: const Key('pdfEditPickOutput'),
          onPressed: _busy || (_action != 'merge' && _source == null)
              ? null
              : _pickOutput,
          child: const Text('저장 위치 변경'),
        ),
        if (_output != null)
          Padding(
            padding: const EdgeInsets.only(top: 8),
            child: Text(
              _output!,
              key: const Key('pdfEditOutputPath'),
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(color: c.muted, fontSize: 12),
            ),
          ),
        const SizedBox(height: 12),
        if (_busy) const LinearProgressIndicator(minHeight: 2),
        if (_action != 'merge') ...[
          Row(
            children: [
              Text(
                '작업 목록 ${_operations.length}개',
                style: TextStyle(color: c.textSecondary),
              ),
              const Spacer(),
              IconButton(
                tooltip: '실행 취소',
                onPressed: _operations.isEmpty || _busy ? null : _undo,
                icon: const Icon(Icons.undo),
              ),
              IconButton(
                tooltip: '다시 실행',
                onPressed: _redo.isEmpty || _busy ? null : _redoEdit,
                icon: const Icon(Icons.redo),
              ),
            ],
          ),
          for (final op in _operations.take(4))
            Text(
              '${op.action == 'delete'
                  ? '삭제'
                  : op.action == 'rotate'
                  ? '회전 ${op.degrees}°'
                  : '순서 변경'} · ${op.pages.join(', ')}',
              style: TextStyle(color: c.muted, fontSize: 11),
            ),
        ],
        FilledButton(
          key: const Key('pdfEditRun'),
          onPressed: _canRun ? _run : null,
          child: Text(_busy ? '처리 중…' : '저장'),
        ),
        if (_message.isNotEmpty) ...[
          const SizedBox(height: 10),
          SelectableText(
            _message,
            key: const Key('pdfEditMessage'),
            style: TextStyle(
              color: _message.startsWith('완료') ? c.success : c.danger,
            ),
          ),
        ],
        const SizedBox(height: 8),
        if (_completedPath != null)
          Card(
            child: Padding(
              padding: const EdgeInsets.all(10),
              child: Row(
                children: [
                  Icon(Icons.check_circle_outline, color: c.success),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      _completedPath!,
                      maxLines: 2,
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
                  IconButton(
                    tooltip: '파일 열기',
                    onPressed: () => widget.api('doc_grid', [
                      'open',
                      {'path': _completedPath},
                    ]),
                    icon: const Icon(Icons.open_in_new),
                  ),
                ],
              ),
            ),
          ),
        Text(
          '원본은 유지되고 결과는 새 파일로 저장됩니다.',
          style: TextStyle(color: c.muted, fontSize: 11),
        ),
      ],
    ),
  );
}

class _ToolChip extends StatelessWidget {
  const _ToolChip({
    required this.label,
    required this.icon,
    required this.selected,
    required this.onTap,
  });
  final String label;
  final IconData icon;
  final bool selected;
  final VoidCallback? onTap;
  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return Material(
      color: selected ? c.accentSoft : c.surface,
      borderRadius: BorderRadius.circular(8),
      child: InkWell(
        key: Key('pdfEditTool_${label.replaceAll(' ', '')}'),
        onTap: onTap,
        borderRadius: BorderRadius.circular(8),
        child: Container(
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 9),
          decoration: BoxDecoration(
            borderRadius: BorderRadius.circular(8),
            border: Border.all(color: selected ? c.accent : c.line),
          ),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(
                icon,
                size: 17,
                color: selected ? c.accent : c.textSecondary,
              ),
              const SizedBox(width: 7),
              Text(
                label,
                style: TextStyle(
                  color: selected ? c.accent : c.textSecondary,
                  fontWeight: selected ? FontWeight.w600 : FontWeight.w500,
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _PageCard extends StatelessWidget {
  const _PageCard({
    super.key,
    required this.number,
    required this.c,
    this.thumbnail,
    required this.selected,
    required this.onTap,
    required this.onRotateLeft,
    required this.onRotateRight,
    required this.onDelete,
    required this.onMove,
    required this.onKey,
  });
  final int number;
  final AppColors c;
  final Uint8List? thumbnail;
  final bool selected;
  final void Function(bool, bool) onTap;
  final VoidCallback onRotateLeft, onRotateRight, onDelete;
  final ValueChanged<int> onMove, onKey;
  @override
  Widget build(BuildContext context) => DragTarget<int>(
    onAcceptWithDetails: (d) => onMove(d.data),
    builder: (context, _, _) => Draggable<int>(
      data: number,
      feedback: Material(
        color: Colors.transparent,
        child: SizedBox(
          width: 110,
          height: 140,
          child: Center(child: Text('쪽 $number')),
        ),
      ),
      childWhenDragging: const SizedBox.shrink(),
      child: FocusableActionDetector(
        autofocus: selected,
        shortcuts: const {
          SingleActivator(LogicalKeyboardKey.arrowLeft): _MoveIntent(-1),
          SingleActivator(LogicalKeyboardKey.arrowRight): _MoveIntent(1),
        },
        actions: {_MoveIntent: _MoveAction(onKey)},
        child: Material(
          color: selected ? c.accentSoft : c.surfaceMuted,
          borderRadius: BorderRadius.circular(8),
          child: InkWell(
            onTap: () => onTap(
              HardwareKeyboard.instance.isShiftPressed,
              HardwareKeyboard.instance.isControlPressed ||
                  HardwareKeyboard.instance.isMetaPressed,
            ),
            onLongPress: () => onTap(false, true),
            onSecondaryTap: () => onTap(false, true),
            child: Container(
              decoration: BoxDecoration(
                border: Border.all(
                  color: selected ? c.accent : c.line,
                  width: selected ? 2 : 1,
                ),
                borderRadius: BorderRadius.circular(8),
              ),
              child: Stack(
                children: [
                  Center(
                    child: Column(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        if (thumbnail != null)
                          Image.memory(
                            thumbnail!,
                            fit: BoxFit.contain,
                            height: 118,
                          )
                        else
                          Icon(
                            Icons.description_outlined,
                            size: 54,
                            color: c.lineStrong,
                          ),
                        const SizedBox(height: 8),
                        Text(
                          '쪽 $number',
                          style: TextStyle(color: c.muted, fontSize: 12),
                        ),
                      ],
                    ),
                  ),
                  Positioned(
                    left: 8,
                    top: 8,
                    child: Container(
                      padding: const EdgeInsets.symmetric(
                        horizontal: 7,
                        vertical: 3,
                      ),
                      decoration: BoxDecoration(
                        color: c.surface,
                        borderRadius: BorderRadius.circular(8),
                        border: Border.all(color: c.line),
                      ),
                      child: Text(
                        '$number',
                        style: TextStyle(color: c.textSecondary, fontSize: 11),
                      ),
                    ),
                  ),
                  if (selected)
                    Positioned(
                      right: 4,
                      top: 4,
                      child: Row(
                        children: [
                          IconButton(
                            tooltip: '왼쪽 90° 회전',
                            onPressed: onRotateLeft,
                            icon: const Icon(Icons.rotate_left, size: 18),
                          ),
                          IconButton(
                            tooltip: '오른쪽 90° 회전',
                            onPressed: onRotateRight,
                            icon: const Icon(Icons.rotate_right, size: 18),
                          ),
                          IconButton(
                            tooltip: '쪽 삭제',
                            onPressed: onDelete,
                            icon: const Icon(Icons.delete_outline, size: 18),
                          ),
                        ],
                      ),
                    ),
                ],
              ),
            ),
          ),
        ),
      ),
    ),
  );
}

class _EditOperation {
  const _EditOperation(this.action, this.pages, {this.degrees});
  final String action;
  final List<int> pages;
  final int? degrees;
}

class _MoveIntent extends Intent {
  const _MoveIntent(this.delta);
  final int delta;
}

class _MoveAction extends Action<_MoveIntent> {
  _MoveAction(this.move);
  final ValueChanged<int> move;
  @override
  Object? invoke(_MoveIntent intent) {
    move(intent.delta);
    return null;
  }
}
