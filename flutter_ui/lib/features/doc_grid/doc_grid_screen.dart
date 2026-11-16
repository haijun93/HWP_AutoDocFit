import 'dart:convert';
import 'dart:typed_data';

import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';

import '../../core/api/abbreviation_api.dart' show ApiCall;

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
  String? _currentFolder;
  List<Map<String, Object?>> _documents = [];
  bool _loading = false;
  String? _errorMessage;

  // 썸네일 바이트 메모리 캐시 (경로 -> 바이트)
  final Map<String, Uint8List> _thumbCache = {};

  @override
  void initState() {
    super.initState();
    if (widget.initialFolder != null && widget.initialFolder!.isNotEmpty) {
      _loadFolder(widget.initialFolder!);
    }
  }

  Future<void> _pickFolder() async {
    try {
      final picker = widget.directoryPicker ?? getDirectoryPath;
      final selected = await picker();
      if (selected != null && selected.isNotEmpty) {
        await _loadFolder(selected);
      }
    } catch (e) {
      if (mounted) {
        setState(() {
          _errorMessage = '폴더 선택 중 오류가 발생했습니다: $e';
        });
      }
    }
  }

  Future<void> _loadFolder(String folder) async {
    setState(() {
      _loading = true;
      _errorMessage = null;
      _currentFolder = folder;
      _documents = [];
      _thumbCache.clear();
    });

    try {
      final res = await widget.api('doc_grid', [
        'list',
        {'folder': folder},
      ]);

      if (!mounted) return;

      if (res is Map && res['ok'] == true) {
        final rawDocs = res['documents'];
        final docs = <Map<String, Object?>>[];
        if (rawDocs is List) {
          for (final item in rawDocs) {
            if (item is Map) {
              docs.add(Map<String, Object?>.from(item));
            }
          }
        }
        setState(() {
          _documents = docs;
          _loading = false;
        });
      } else {
        final msg = (res is Map ? res['message'] : null) ?? '문서 목록을 불러오지 못했습니다.';
        setState(() {
          _errorMessage = '$msg';
          _loading = false;
        });
      }
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _errorMessage = '폴더를 불러오는 중 오류가 발생했습니다: $e';
        _loading = false;
      });
    }
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
                  FilledButton.icon(
                    key: const ValueKey('docGridPickFolderButton'),
                    onPressed: _loading ? null : _pickFolder,
                    icon: const Icon(Icons.folder_open, size: 18),
                    label: const Text('폴더 선택'),
                  ),
                  const SizedBox(width: 12),
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
                    IconButton(
                      tooltip: '새로고침',
                      icon: const Icon(Icons.refresh, size: 20),
                      onPressed: _loading ? null : () => _loadFolder(_currentFolder!),
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

  Widget _buildBody() {
    if (_loading) {
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
              OutlinedButton.icon(
                onPressed: _pickFolder,
                icon: const Icon(Icons.folder_open),
                label: const Text('다른 폴더 선택'),
              ),
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
              '폴더를 선택하여 문서를 확인하세요.',
              style: TextStyle(fontSize: 16, fontWeight: FontWeight.w600),
            ),
            const SizedBox(height: 8),
            const Text(
              '지원 문서: HWP, HWPX, XLS, XLSX, PPT, PPTX, PDF',
              style: TextStyle(color: Colors.grey, fontSize: 13),
            ),
            const SizedBox(height: 20),
            FilledButton.icon(
              onPressed: _pickFolder,
              icon: const Icon(Icons.folder_open),
              label: const Text('폴더 선택'),
            ),
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
            OutlinedButton.icon(
              onPressed: _pickFolder,
              icon: const Icon(Icons.folder_open),
              label: const Text('다른 폴더 선택'),
            ),
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
            gridDelegate: const SliverGridDelegateWithMaxCrossAxisExtent(
              maxCrossAxisExtent: 190,
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
                onDoubleTap: () => _openDocument(path),
              );
            },
          ),
        );

        if (!constraints.hasBoundedHeight) {
          return SizedBox(
            height: 600,
            child: grid,
          );
        }
        return grid;
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
    this.cachedThumb,
    this.onThumbLoaded,
  });

  final Map<String, Object?> doc;
  final ApiCall api;
  final VoidCallback onDoubleTap;
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
      child: InkWell(
        onDoubleTap: widget.onDoubleTap,
        borderRadius: BorderRadius.circular(8),
        child: Container(
          decoration: BoxDecoration(
            color: Theme.of(context).colorScheme.surface,
            borderRadius: BorderRadius.circular(8),
            border: Border.all(
              color: Theme.of(context).dividerColor.withValues(alpha: 0.3),
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
    );
  }
}
