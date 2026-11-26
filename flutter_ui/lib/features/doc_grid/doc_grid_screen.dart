import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';

import 'package:flutter/gestures.dart' show kPrimaryButton;
import 'package:flutter/material.dart';
import 'package:file_selector/file_selector.dart';

import '../../core/api/abbreviation_api.dart' show ApiCall;
import '../common/file_handoff.dart';
import '../common/file_ops_area.dart';
import '../common/file_selection.dart';
import '../common/thumb_size_slider.dart';

/// 문서 확장자별 대표 테마 색상.
Color _docExtColor(String ext) {
  switch (ext.toLowerCase()) {
    case 'hwp':
    case 'hwpx':
      return const Color(0xFF2563EB); // 파랑
    case 'xls':
    case 'xlsx':
      return const Color(0xFF16A34A); // 초록
    case 'ppt':
    case 'pptx':
      return const Color(0xFFDC2626); // 빨강
    case 'pdf':
      return const Color(0xFFEA580C); // 주황
    default:
      return const Color(0xFF64748B); // 슬레이트
  }
}

/// 파일 크기 문자열 형식화.
String _formatBytes(int bytes) {
  if (bytes < 1024) return '$bytes B';
  if (bytes < 1024 * 1024) return '${(bytes / 1024).toStringAsFixed(1)} KB';
  return '${(bytes / (1024 * 1024)).toStringAsFixed(1)} MB';
}

/// 문서뷰 화면 (DVF3).
/// 폴더를 선택하여 문서(HWP·HWPX·XLS·XLSX·PPT·PPTX·PDF)를 썸네일 격자로 탐색하고,
/// 화면에 보일 때만 썸네일을 요청하며, 더블클릭 시 시스템 연결 프로그램으로 연다.
class DocGridScreen extends StatefulWidget {
  const DocGridScreen({
    super.key,
    required this.api,
    this.directoryPicker,
    this.initialFolder,
  });

  final ApiCall api;
  final Future<String?> Function()? directoryPicker;
  final String? initialFolder;

  @override
  State<DocGridScreen> createState() => _DocGridScreenState();
}

class _DocGridScreenState extends State<DocGridScreen> {
  Timer? _scanUiTimer;
  DateTime _scanNow = DateTime.now();
  String get _scanElapsed {
    final seconds = _scanNow.difference(_scanStartedAt ?? _scanNow).inSeconds;
    return '${seconds ~/ 60}분 ${seconds % 60}초 경과';
  }
  String? _currentFolder;
  final FileSelection _selection = FileSelection();
  final GlobalKey<FileOpsAreaState> _fileOps = GlobalKey<FileOpsAreaState>();
  List<Map<String, Object?>> _documents = [];
  bool _loading = false;
  String? _errorMessage;
  bool _scanRunning = false;
  // 검색 중에 찾은 항목을 이미 보여 주고 있는지.
  bool _streaming = false;
  int _scanVisited = 0;
  String _scanFolder = '';
  DateTime? _scanStartedAt;
  DateTime? _scanLastProgressAt;

  // 썸네일 바이트 메모리 캐시 (경로 -> 바이트)
  final Map<String, Uint8List> _thumbCache = {};

  // 썸네일 크기 (100 ~ 240, 기본 190 = 기존 고정값). 설정에 저장·복원한다.
  double _thumbSize = 190.0;

  @override
  void initState() {
    super.initState();
    _restoreThumbSize();
    _loadDefault();
  }

  @override
  void dispose() {
    _scanUiTimer?.cancel();
    super.dispose();
  }

  Future<void> _restoreThumbSize() async {
    final saved = await loadThumbSize(widget.api, 'doc_grid');
    if (saved != null && mounted) setState(() => _thumbSize = saved);
  }

  Future<void> _loadDefault() async {
    try {
      _selection.clear();
      setState(() { _loading = true; _streaming = false; _scanRunning = true; _scanVisited = 0; _scanFolder = ''; _scanStartedAt = DateTime.now(); _scanLastProgressAt = _scanStartedAt; _scanNow = DateTime.now(); _scanUiTimer?.cancel(); _scanUiTimer = Timer.periodic(const Duration(seconds: 1), (_) { if (mounted && _scanRunning) setState(() => _scanNow = DateTime.now()); }); _errorMessage = null; });
      final res = await widget.api('doc_grid', ['list_default', {}]);
      if (!mounted) return;
      // 화면을 다시 열었을 때 이미 도는 스캔이 있으면 그 스캔을 이어서 보여 준다.
      if (res is Map && (res['ok'] == true || res['code'] == 'SCAN_RUNNING')) {
        Object? result = res;
        var cached = false;
        if (res['state'] == 'cached' && res['result'] is Map) {
          final cachedResult = res['result'] as Map;
          final cachedItems = cachedResult['documents'];
          if (cachedItems is List) {
            cached = true;
            setState(() {
              _documents = cachedItems.whereType<Map>().map((e) => Map<String, Object?>.from(e)).toList();
              _loading = false;
              _currentFolder = '모든 고정 드라이브';
            });
          }
        }
        // 저장된 목록이 없으면 찾는 대로 새 행만 받아 이어 붙인다.
        var streamed = false;
        if (res['task_id'] != null) {
          var state = 'running';
          var next = 0;
          var backlog = false;
          while (state == 'running' || state == 'cancelling' || backlog) {
            if (!backlog) await Future<void>.delayed(const Duration(milliseconds: 120));
            if (!mounted) return;
            final progress = await widget.api('doc_grid', ['scan_progress', cached ? {} : {'since': next}]);
            if (!mounted) return;
            if (progress is! Map || progress['ok'] != true) break;
            state = '${progress['state'] ?? 'running'}';
            final rows = progress['rows'];
            final added = <Map<String, Object?>>[];
            if (!cached && rows is List) {
              if (!streamed) _documents = [];
              streamed = true;
              for (final item in rows) {
                if (item is Map) added.add(Map<String, Object?>.from(item));
              }
              final following = (progress['next'] as num?)?.toInt() ?? next;
              backlog = following > next && following < ((progress['total'] as num?)?.toInt() ?? 0);
              next = following;
            } else {
              backlog = false;
            }
            setState(() {
              _scanVisited = (progress['visited'] as num?)?.toInt() ?? 0; _scanFolder = '${progress['current_folder'] ?? ''}'; _scanStartedAt ??= DateTime.now(); _scanLastProgressAt = DateTime.now();
              if (added.isNotEmpty) {
                _documents.addAll(added);
                _loading = false;
                _streaming = true;
                _currentFolder = '모든 고정 드라이브';
              }
            });
            if (state == 'complete' || state == 'cancelled') result = progress['result'];
            if (state == 'failed') throw StateError('${progress['error'] ?? '스캔에 실패했습니다.'}');
          }
        }
        final List<Map<String, Object?>> docs;
        if (streamed) {
          // 정렬은 검색이 끝났을 때 한 번만 한다(묶음마다 다시 정렬하지 않는다).
          docs = _sortedByName(_documents);
        } else {
          final scanResult = result is Map ? result : res;
          docs = <Map<String, Object?>>[];
          if (scanResult['documents'] is List) {
            for (final item in scanResult['documents'] as List) {
              if (item is Map) docs.add(Map<String, Object?>.from(item));
            }
          }
        }
        setState(() { _documents = docs; _loading = false; _scanRunning = false; _streaming = false; _currentFolder = '모든 고정 드라이브'; });
      } else {
        setState(() { _errorMessage = '${res is Map ? res['message'] : '문서 목록을 불러오지 못했습니다.'}'; _loading = false; _scanRunning = false; _streaming = false; });
      }
    } catch (e) {
      if (mounted) {
        setState(() {
          _errorMessage = '문서 목록을 불러오는 중 오류가 발생했습니다: $e';
          _scanRunning = false;
          _streaming = false;
          _loading = false;
        });
      }
    }
  }

  static List<Map<String, Object?>> _sortedByName(List<Map<String, Object?>> docs) {
    final keyed = [for (final doc in docs) ('${doc['name']}'.toLowerCase(), doc)]
      ..sort((a, b) => a.$1.compareTo(b.$1));
    return [for (final entry in keyed) entry.$2];
  }

  Future<void> _cancelScan() async {
    await widget.api('doc_grid', ['scan_cancel', {}]);
  }

  Future<void> _editExcludedFolders() async {
    final response = await widget.api('doc_grid', ['excluded_folders_get', {}]);
    var folders = response is Map && response['folders'] is List
        ? (response['folders'] as List).whereType<String>().toList()
        : <String>[];
    if (!mounted) return;
    await showDialog<void>(
      context: context,
      builder: (dialogContext) => StatefulBuilder(
        builder: (context, setDialogState) => AlertDialog(
          key: const ValueKey('docGridExcludedFoldersDialog'),
          title: const Text('제외할 폴더'),
          content: SizedBox(width: 440, child: folders.isEmpty
              ? const Text('제외한 폴더가 없습니다.')
              : ListView(shrinkWrap: true, children: [for (final folder in folders)
                  ListTile(title: Text(folder), trailing: IconButton(
                    key: ValueKey('docGridRemoveExcluded-${folders.indexOf(folder)}'),
                    tooltip: '제외 폴더 삭제', icon: const Icon(Icons.delete_outline),
                    onPressed: () async {
                      final result = await widget.api('doc_grid', ['excluded_folder_remove', {'folder': folder}]);
                      if (result is Map && result['folders'] is List) setDialogState(() { folders = (result['folders'] as List).whereType<String>().toList(); });
                    },
                  )),])),
          actions: [
            TextButton(onPressed: () async {
              final selected = widget.directoryPicker != null
                  ? await widget.directoryPicker!()
                  : await getDirectoryPath();
              if (selected == null || selected.isEmpty) return;
              final result = await widget.api('doc_grid', ['excluded_folder_add', {'folder': selected}]);
              if (result is Map && result['folders'] is List) setDialogState(() { folders = (result['folders'] as List).whereType<String>().toList(); });
            }, child: const Text('폴더 추가')),
            TextButton(onPressed: () => Navigator.pop(dialogContext), child: const Text('닫기')),
          ],
        ),
      ),
    );
  }

  Future<void> _openDocument(String path) async {
    try {
      final res = await widget.api('doc_grid', [
        'open',
        {'path': path},
      ]);
      if (res is Map && res['ok'] != true && mounted) {
        final msg = res['message'] ?? '문서를 열지 못했습니다.';
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('$msg')),
        );
      }
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('문서 열기 실패: $e')),
        );
      }
    }
  }

  /// 오른쪽 클릭 메뉴의 연결 항목.
  List<FileMenuEntry> _menuEntries(List<String> targets) {
    final handoff = FileHandoff.maybeOf(context);
    if (handoff == null) return const [];
    FileMenuEntry entry(FileTarget target, String label, IconData icon) =>
        handoffEntry(context, handoff, target, targets, label: label, icon: icon);
    return [
      entry(FileTarget.documentEditor, '문서편집', Icons.description_outlined),
      entry(FileTarget.virtualFolder, '가상 폴더에 추가', Icons.dashboard_customize_outlined),
      entry(FileTarget.notebook, '업무자료함에 넣기', Icons.menu_book_outlined),
      entry(FileTarget.conversion, '파일 변환', Icons.transform),
      entry(FileTarget.pdfEdit, 'PDF 편집', Icons.picture_as_pdf_outlined),
    ];
  }

  void _showContextMenu(Offset position, String path) =>
      _fileOps.currentState?.showContextMenu(
        position,
        path,
        onOpen: _openDocument,
        extraEntries: _menuEntries,
      );

  List<String> get _order => [for (final doc in _documents) '${doc['path'] ?? ''}'];

  /// 파일 작업 결과를 목록과 썸네일 메모리 캐시에 반영한다.
  void _applyFileResults(String op, List<Map<String, Object?>> results) {
    if (!mounted || op == 'clipboard') return;
    setState(() {
      _documents = applyFileResults(_documents, op, results, (doc, path, name) {
        final thumb = _thumbCache['${doc['path']}'];
        if (thumb != null) _thumbCache[path] = thumb;
        final dot = name.lastIndexOf('.');
        return {...doc, 'path': path, 'name': name, 'stem': dot > 0 ? name.substring(0, dot) : name};
      })..sort((a, b) => '${a['name']}'.toLowerCase().compareTo('${b['name']}'.toLowerCase()));
      if (op != 'copy') {
        for (final result in results) {
          if (result['ok'] == true) _thumbCache.remove('${result['path']}');
        }
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        final content = Scaffold(
          appBar: PreferredSize(
            preferredSize: const Size.fromHeight(56),
            child: Container(
              padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
              decoration: BoxDecoration(
                color: Theme.of(context).colorScheme.surface,
                border: Border(
                  bottom: BorderSide(
                    color: Theme.of(context).dividerColor.withValues(alpha: 0.2),
                  ),
                ),
              ),
              child: Row(
                children: [
                  if (_currentFolder != null) ...[
                    Expanded(
                      child: Row(
                        children: [
                          const Icon(Icons.folder_outlined, size: 18, color: Colors.grey),
                          const SizedBox(width: 6),
                          Flexible(
                            child: Text(
                              _currentFolder!,
                              overflow: TextOverflow.ellipsis,
                              style: const TextStyle(fontWeight: FontWeight.w500),
                            ),
                          ),
                          const SizedBox(width: 8),
                          Container(
                            padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
                            decoration: BoxDecoration(
                              color: Theme.of(context).colorScheme.primaryContainer,
                              borderRadius: BorderRadius.circular(12),
                            ),
                            child: Text(
                              '${_documents.length}개 문서',
                              style: TextStyle(
                                fontSize: 12,
                                fontWeight: FontWeight.bold,
                                color: Theme.of(context).colorScheme.onPrimaryContainer,
                              ),
                            ),
                          ),
                        ],
                      ),
                    ),
                    ThumbSizeSlider(
                      sliderKey: const ValueKey('docGridThumbSizeSlider'),
                      value: _thumbSize,
                      onChanged: (val) => setState(() => _thumbSize = val),
                      onChangeEnd: (val) =>
                          saveThumbSize(widget.api, 'doc_grid', val),
                    ),
                    IconButton(
                      key: const ValueKey('docGridExcludedFoldersButton'),
                      tooltip: '제외할 폴더',
                      icon: const Icon(Icons.folder_off_outlined, size: 20),
                      onPressed: _editExcludedFolders,
                    ),
                    IconButton(
                      tooltip: '새로고침',
                      icon: const Icon(Icons.refresh, size: 20),
                      onPressed: _loading || _scanRunning ? null : _loadDefault,
                    ),
                  ] else
                    const Expanded(
                      child: Text(
                        '폴더를 선택하면 문서(HWP·HWPX·XLS·XLSX·PPT·PPTX·PDF) 목록과 썸네일이 격자로 표시됩니다.',
                        style: TextStyle(color: Colors.grey, fontSize: 13),
                        overflow: TextOverflow.ellipsis,
                      ),
                    ),
                ],
              ),
            ),
          ),
          body: _buildBody(),
        );

        if (!constraints.hasBoundedHeight) {
          return SizedBox(
            height: 600,
            child: content,
          );
        }
        return content;
      },
    );
  }

  /// 목록을 보여 주면서 검색이 계속될 때의 진행 띠.
  Widget _buildScanBanner() => Padding(
    key: const ValueKey('docGridScanBanner'),
    padding: const EdgeInsets.symmetric(horizontal: 16),
    child: Row(
      children: [
        const SizedBox(width: 120, child: LinearProgressIndicator()),
        const SizedBox(width: 12),
        Expanded(
          child: Text(
            '문서 검색 중 · ${_documents.length}개 표시 · $_scanElapsed',
            overflow: TextOverflow.ellipsis,
            style: const TextStyle(fontSize: 12),
          ),
        ),
        TextButton.icon(
          key: const ValueKey('docGridCancelScanButton'),
          onPressed: _cancelScan,
          icon: const Icon(Icons.cancel_outlined, size: 16),
          label: const Text('스캔 취소'),
        ),
      ],
    ),
  );

  Widget _buildBody() {
    if (_loading) {
      if (_scanRunning) {
        return Center(child: Column(mainAxisSize: MainAxisSize.min, children: [
          const SizedBox(width: 320, child: LinearProgressIndicator()),
          const SizedBox(height: 12), Text('문서 검색 중 · 훑은 항목 $_scanVisited개 · $_scanElapsed'),
          if (_scanFolder.isNotEmpty) Text('현재 폴더: $_scanFolder', maxLines: 2, overflow: TextOverflow.ellipsis),
          if (_scanLastProgressAt != null && _scanNow.difference(_scanLastProgressAt!) > const Duration(seconds: 15)) const Text('15초 동안 새 항목이 없습니다. 현재 폴더 응답을 기다리는 중입니다. 계속되면 스캔을 취소할 수 있습니다.'),
          const SizedBox(height: 8), OutlinedButton.icon(
            key: const ValueKey('docGridCancelScanButton'), onPressed: _cancelScan,
            icon: const Icon(Icons.cancel_outlined), label: const Text('스캔 취소'),
          ),
        ]));
      }
      return const Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            CircularProgressIndicator(),
            SizedBox(height: 16),
            Text('문서 목록을 불러오는 중...'),
          ],
        ),
      );
    }

    if (_errorMessage != null) {
      return Center(
        child: Padding(
          padding: const EdgeInsets.all(24),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              const Icon(Icons.error_outline, size: 48, color: Colors.redAccent),
              const SizedBox(height: 12),
              Text(
                _errorMessage!,
                textAlign: TextAlign.center,
                style: const TextStyle(color: Colors.redAccent, fontSize: 14),
              ),
              const SizedBox(height: 16),
            ],
          ),
        ),
      );
    }

    if (_currentFolder == null) {
      return Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(
              Icons.grid_view_outlined,
              size: 64,
              color: Theme.of(context).colorScheme.outline.withValues(alpha: 0.5),
            ),
            const SizedBox(height: 16),
            const Text(
              '모든 고정 드라이브의 문서를 확인합니다.',
              style: TextStyle(fontSize: 16, fontWeight: FontWeight.w600),
            ),
            const SizedBox(height: 8),
            const Text(
              '지원 문서: HWP, HWPX, XLS, XLSX, PPT, PPTX, PDF',
              style: TextStyle(color: Colors.grey, fontSize: 13),
            ),
            const SizedBox(height: 20),
          ],
        ),
      );
    }

    if (_documents.isEmpty) {
      return Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(
              Icons.folder_off_outlined,
              size: 56,
              color: Theme.of(context).colorScheme.outline.withValues(alpha: 0.5),
            ),
            const SizedBox(height: 16),
            const Text(
              '지원하는 문서가 없습니다.',
              key: ValueKey('docGridEmptyText'),
              style: TextStyle(fontSize: 15, fontWeight: FontWeight.w600),
            ),
            const SizedBox(height: 6),
            const Text(
              '폴더에 HWP, HWPX, XLS, XLSX, PPT, PPTX, PDF 파일이 없습니다.',
              style: TextStyle(color: Colors.grey, fontSize: 12),
            ),
            const SizedBox(height: 16),
          ],
        ),
      );
    }

    return LayoutBuilder(
      builder: (context, constraints) {
        final grid = Scrollbar(
          child: GridView.builder(
            key: const ValueKey('docGridView'),
            padding: const EdgeInsets.all(16),
            gridDelegate: SliverGridDelegateWithMaxCrossAxisExtent(
              maxCrossAxisExtent: _thumbSize,
              childAspectRatio: 0.74,
              crossAxisSpacing: 14,
              mainAxisSpacing: 14,
            ),
            itemCount: _documents.length,
            itemBuilder: (context, index) {
              final doc = _documents[index];
              final path = '${doc['path'] ?? ''}';
              return _DocCard(
                key: ValueKey('docCard-$path'),
                doc: doc,
                cachedThumb: _thumbCache[path],
                api: widget.api,
                onThumbLoaded: (bytes) {
                  _thumbCache[path] = bytes;
                },
                selected: _selection.paths.contains(path),
                onSelect: () => setState(() => _selection.click(
                  path,
                  _order,
                  toggle: FileSelection.toggleKeyPressed,
                  range: FileSelection.rangeKeyPressed,
                )),
                onDoubleTap: () => _openDocument(path),
                onContextMenu: (position) => _showContextMenu(position, path),
              );
            },
          ),
        );

        final area = FileOpsArea(
          key: _fileOps,
          api: widget.api,
          method: 'doc_grid',
          selection: _selection,
          order: _order,
          directoryPicker: widget.directoryPicker,
          onSelectionChanged: () => setState(() {}),
          onApplied: _applyFileResults,
          child: grid,
        );
        final body = Column(
          children: [
            if (_streaming) _buildScanBanner(),
            Expanded(child: area),
          ],
        );
        if (!constraints.hasBoundedHeight) {
          return SizedBox(
            height: 600,
            child: body,
          );
        }
        return body;
      },
    );
  }
}

/// 문서 카드 위젯 (화면에 보일 때 썸네일 비동기 요청).
class _DocCard extends StatefulWidget {
  const _DocCard({
    super.key,
    required this.doc,
    required this.api,
    required this.onDoubleTap,
    required this.onSelect,
    required this.onContextMenu,
    this.selected = false,
    this.cachedThumb,
    this.onThumbLoaded,
  });

  final Map<String, Object?> doc;
  final ApiCall api;
  final VoidCallback onDoubleTap;
  final VoidCallback onSelect;
  final ValueChanged<Offset> onContextMenu;
  final bool selected;
  final Uint8List? cachedThumb;
  final ValueChanged<Uint8List>? onThumbLoaded;

  @override
  State<_DocCard> createState() => _DocCardState();
}

class _DocCardState extends State<_DocCard> {
  Uint8List? _thumbBytes;
  bool _loadingThumb = false;
  bool _thumbFailed = false;

  @override
  void initState() {
    super.initState();
    if (widget.cachedThumb != null) {
      _thumbBytes = widget.cachedThumb;
    } else {
      _requestThumbnail();
    }
  }

  @override
  void didUpdateWidget(covariant _DocCard oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.cachedThumb != null && widget.cachedThumb != _thumbBytes) {
      _thumbBytes = widget.cachedThumb;
    }
  }

  Future<void> _requestThumbnail() async {
    final path = '${widget.doc['path'] ?? ''}';
    if (path.isEmpty || _loadingThumb || _thumbBytes != null) return;

    if (widget.doc['cloud'] == true) {
      // 클라우드 전용 파일은 썸네일을 요청하지 않는다
      return;
    }

    setState(() {
      _loadingThumb = true;
      _thumbFailed = false;
    });

    try {
      final res = await widget.api('doc_grid', [
        'thumbnail',
        {'path': path},
      ]);

      if (!mounted) return;

      if (res is Map && res['ok'] == true && res['data'] is String) {
        final b64 = res['data'] as String;
        final bytes = base64Decode(b64);
        setState(() {
          _thumbBytes = bytes;
          _loadingThumb = false;
        });
        widget.onThumbLoaded?.call(bytes);
      } else {
        setState(() {
          _loadingThumb = false;
          _thumbFailed = true;
        });
      }
    } catch (_) {
      if (mounted) {
        setState(() {
          _loadingThumb = false;
          _thumbFailed = true;
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final name = '${widget.doc['name'] ?? ''}';
    final ext = '${widget.doc['ext'] ?? ''}'.toUpperCase();
    final size = widget.doc['size'] is num ? (widget.doc['size'] as num).toInt() : 0;
    final isCloud = widget.doc['cloud'] == true;
    final extColor = _docExtColor(ext);

    return Tooltip(
      message: '$name\n더블클릭하여 열기',
      // 선택은 누르는 즉시 표시한다(InkWell.onTap 은 더블클릭 판정 뒤에야 온다).
      child: Listener(
        onPointerDown: (event) {
          if (event.buttons == kPrimaryButton) widget.onSelect();
        },
        child: InkWell(
          onSecondaryTapUp: (details) => widget.onContextMenu(details.globalPosition),
          // Ctrl·Shift 클릭은 선택만 한다.
          onDoubleTap: () {
            if (!FileSelection.toggleKeyPressed && !FileSelection.rangeKeyPressed) {
              widget.onDoubleTap();
            }
          },
          borderRadius: BorderRadius.circular(8),
          child: Container(
            key: ValueKey('docCardFrame-${widget.doc['path']}'),
            decoration: BoxDecoration(
              color: Theme.of(context).colorScheme.surface,
              borderRadius: BorderRadius.circular(8),
              border: Border.all(
                color: widget.selected
                    ? Theme.of(context).colorScheme.primary
                    : Theme.of(context).dividerColor.withValues(alpha: 0.3),
                width: widget.selected ? 2.5 : 1.0,
              ),
              boxShadow: [
                BoxShadow(
                  color: Colors.black.withValues(alpha: 0.03),
                  blurRadius: 4,
                  offset: const Offset(0, 2),
                ),
              ],
            ),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                // 상단 썸네일 영역
                Expanded(
                  child: Container(
                    decoration: BoxDecoration(
                      color: Theme.of(context).colorScheme.surfaceContainerHighest.withValues(alpha: 0.3),
                      borderRadius: const BorderRadius.vertical(top: Radius.circular(8)),
                    ),
                    clipBehavior: Clip.antiAlias,
                    child: Stack(
                      alignment: Alignment.center,
                      children: [
                        if (_thumbBytes != null)
                          Image.memory(
                            _thumbBytes!,
                            fit: BoxFit.contain,
                            filterQuality: FilterQuality.medium,
                            width: double.infinity,
                            height: double.infinity,
                          )
                        else if (_loadingThumb)
                          const SizedBox(
                            width: 24,
                            height: 24,
                            child: CircularProgressIndicator(strokeWidth: 2),
                          )
                        else if (isCloud)
                          const Icon(Icons.cloud_outlined, size: 36, color: Colors.blueGrey)
                        else if (_thumbFailed)
                          Icon(Icons.description_outlined, size: 40, color: extColor.withValues(alpha: 0.6))
                        else
                          Icon(Icons.insert_drive_file_outlined, size: 40, color: extColor.withValues(alpha: 0.5)),

                        // 좌측 상단 확장자 배지
                        Positioned(
                          top: 6,
                          left: 6,
                          child: Container(
                            padding: const EdgeInsets.symmetric(horizontal: 5, vertical: 2),
                            decoration: BoxDecoration(
                              color: extColor,
                              borderRadius: BorderRadius.circular(4),
                            ),
                            child: Text(
                              ext.isEmpty ? 'DOC' : ext,
                              style: const TextStyle(
                                color: Colors.white,
                                fontSize: 9,
                                fontWeight: FontWeight.bold,
                              ),
                            ),
                          ),
                        ),
                      ],
                    ),
                  ),
                ),

                // 하단 파일 정보 영역
                Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        name,
                        maxLines: 2,
                        overflow: TextOverflow.ellipsis,
                        style: const TextStyle(
                          fontSize: 12,
                          fontWeight: FontWeight.w600,
                          height: 1.25,
                        ),
                      ),
                      const SizedBox(height: 4),
                      Text(
                        _formatBytes(size),
                        style: TextStyle(
                          fontSize: 10,
                          color: Theme.of(context).textTheme.bodySmall?.color?.withValues(alpha: 0.7),
                        ),
                      ),
                    ],
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
