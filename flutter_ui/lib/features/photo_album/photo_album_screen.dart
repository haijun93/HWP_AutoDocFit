import 'dart:convert';
import 'dart:io';

import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../core/api/abbreviation_api.dart' show ApiCall;

/// 파일 크기 문자열 형식화.
String _formatBytes(int bytes) {
  if (bytes < 1024) return '$bytes B';
  if (bytes < 1024 * 1024) return '${(bytes / 1024).toStringAsFixed(1)} KB';
  return '${(bytes / (1024 * 1024)).toStringAsFixed(1)} MB';
}

/// 사진뷰 화면 (MPLB2).
/// 사진 폴더 등록·새로고침, 연·월 묶음 격자 보기(화면에 보일 때만 썸네일 요청),
/// 썸네일 크기 조절, 사진 클릭 시 크게 보기 모달(키보드 좌/우/Esc 조작),
/// 즐겨찾기·태그 추가/삭제, 파일명/날짜/태그 검색,
/// 사진 다중 선택 시 '공문 첨부용 사본 저장'·'사진대지 만들기(2×2·2×3)' 실행 UI를 제공한다.
class PhotoAlbumScreen extends StatefulWidget {
  const PhotoAlbumScreen({
    super.key,
    required this.api,
    this.directoryPicker,
    this.fileSavePicker,
    this.initialFolder,
  });

  final ApiCall api;
  final Future<String?> Function()? directoryPicker;
  final Future<String?> Function({String? suggestedName})? fileSavePicker;
  final String? initialFolder;

  @override
  State<PhotoAlbumScreen> createState() => _PhotoAlbumScreenState();
}

class _PhotoAlbumScreenState extends State<PhotoAlbumScreen> {
  List<Map<String, Object?>> _registeredFolders = [];
  String? _currentFolder;
  List<Map<String, Object?>> _groups = [];
  List<Map<String, Object?>>? _searchResults;
  bool _loading = false;
  String? _errorMessage;

  // 썸네일 크기 (120 ~ 240, 기본 160)
  double _thumbSize = 160.0;

  // 썸네일 바이트 메모리 캐시 (경로 -> 바이트)
  final Map<String, Uint8List> _thumbCache = {};

  // 다중 선택 모드 및 선택된 사진 경로
  bool _selectionMode = false;
  final Set<String> _selectedPaths = {};

  // 검색 및 필터 상태
  final TextEditingController _searchController = TextEditingController();
  bool _favoriteFilter = false;

  @override
  void initState() {
    super.initState();
    _initFolders();
  }

  @override
  void dispose() {
    _searchController.dispose();
    super.dispose();
  }

  Future<void> _initFolders() async {
    setState(() {
      _loading = true;
      _errorMessage = null;
    });

    try {
      if (widget.initialFolder != null && widget.initialFolder!.isNotEmpty) {
        await _registerAndSelectFolder(widget.initialFolder!);
        return;
      }

      final res = await widget.api('photo_album', ['list_folders', {}]);
      if (!mounted) return;

      if (res is Map && res['ok'] == true) {
        final rawFolders = res['folders'];
        final folders = <Map<String, Object?>>[];
        if (rawFolders is List) {
          for (final item in rawFolders) {
            if (item is Map) {
              folders.add(Map<String, Object?>.from(item));
            }
          }
        }
        setState(() {
          _registeredFolders = folders;
        });

        if (folders.isNotEmpty) {
          final first = folders.first['path'] as String?;
          if (first != null && first.isNotEmpty) {
            await _loadPhotos(first);
            return;
          }
        }
      }
      setState(() {
        _loading = false;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _errorMessage = '사진 폴더 목록을 불러오지 못했습니다: $e';
        _loading = false;
      });
    }
  }

  Future<void> _pickFolder() async {
    try {
      final picker = widget.directoryPicker ?? getDirectoryPath;
      final selected = await picker();
      if (selected != null && selected.isNotEmpty) {
        await _registerAndSelectFolder(selected);
      }
    } catch (e) {
      if (mounted) {
        setState(() {
          _errorMessage = '폴더 선택 중 오류가 발생했습니다: $e';
        });
      }
    }
  }

  Future<void> _registerAndSelectFolder(String folder) async {
    setState(() {
      _loading = true;
      _errorMessage = null;
    });

    try {
      // 1. 폴더 등록
      final regRes = await widget.api('photo_album', [
        'register_folder',
        {'folder': folder},
      ]);

      if (regRes is Map && regRes['ok'] != true) {
        final msg = regRes['message'] ?? '폴더 등록에 실패했습니다.';
        if (mounted) {
          setState(() {
            _errorMessage = '$msg';
            _loading = false;
          });
        }
        return;
      }

      // 2. 폴더 스캔
      await widget.api('photo_album', [
        'scan',
        {'folder': folder, 'recursive': false},
      ]);

      // 3. 사진 목록 로드
      await _loadPhotos(folder);

      // 등록된 폴더 목록 새로고침
      final foldersRes = await widget.api('photo_album', ['list_folders', {}]);
      if (foldersRes is Map && foldersRes['ok'] == true && mounted) {
        final rawFolders = foldersRes['folders'];
        if (rawFolders is List) {
          _registeredFolders = [
            for (final item in rawFolders)
              if (item is Map) Map<String, Object?>.from(item),
          ];
        }
      }
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _errorMessage = '폴더 등록 중 오류가 발생했습니다: $e';
        _loading = false;
      });
    }
  }

  Future<void> _loadPhotos(String folder) async {
    setState(() {
      _loading = true;
      _errorMessage = null;
      _currentFolder = folder;
      _searchResults = null;
      _selectedPaths.clear();
      _selectionMode = false;
    });

    try {
      final res = await widget.api('photo_album', [
        'list_by_date',
        {'folder': folder},
      ]);

      if (!mounted) return;

      if (res is Map && res['ok'] == true) {
        final rawGroups = res['groups'];
        final groups = <Map<String, Object?>>[];
        if (rawGroups is List) {
          for (final g in rawGroups) {
            if (g is Map) {
              final groupMap = Map<String, Object?>.from(g);
              final rawPhotos = groupMap['photos'];
              if (rawPhotos is List) {
                groupMap['photos'] = [
                  for (final p in rawPhotos)
                    if (p is Map) Map<String, Object?>.from(p),
                ];
              }
              groups.add(groupMap);
            }
          }
        }
        setState(() {
          _groups = groups;
          _loading = false;
        });
      } else {
        final msg = (res is Map ? res['message'] : null) ?? '사진 목록을 불러오지 못했습니다.';
        setState(() {
          _errorMessage = '$msg';
          _loading = false;
        });
      }
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _errorMessage = '사진 목록을 불러오는 중 오류가 발생했습니다: $e';
        _loading = false;
      });
    }
  }

  Future<void> _refreshCurrentFolder() async {
    if (_currentFolder == null) return;
    setState(() {
      _loading = true;
      _errorMessage = null;
    });

    try {
      await widget.api('photo_album', [
        'scan',
        {'folder': _currentFolder, 'recursive': false},
      ]);
      await _loadPhotos(_currentFolder!);
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _errorMessage = '새로고침 중 오류가 발생했습니다: $e';
        _loading = false;
      });
    }
  }

  Future<void> _performSearch() async {
    final query = _searchController.text.trim();
    if (query.isEmpty && !_favoriteFilter) {
      setState(() {
        _searchResults = null;
      });
      return;
    }

    setState(() {
      _loading = true;
      _errorMessage = null;
    });

    try {
      final res = await widget.api('photo_album', [
        'search',
        {
          'query': query,
          'folder': _currentFolder,
          'favorite_only': _favoriteFilter,
        },
      ]);

      if (!mounted) return;

      if (res is Map && res['ok'] == true) {
        final rawResults = res['results'];
        final results = <Map<String, Object?>>[];
        if (rawResults is List) {
          for (final item in rawResults) {
            if (item is Map) {
              results.add(Map<String, Object?>.from(item));
            }
          }
        }
        setState(() {
          _searchResults = results;
          _loading = false;
        });
      } else {
        final msg = (res is Map ? res['message'] : null) ?? '검색 결과를 불러오지 못했습니다.';
        setState(() {
          _errorMessage = '$msg';
          _loading = false;
        });
      }
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _errorMessage = '검색 중 오류가 발생했습니다: $e';
        _loading = false;
      });
    }
  }

  Future<void> _toggleFavorite(Map<String, Object?> photo) async {
    final path = '${photo['path'] ?? ''}';
    if (path.isEmpty) return;

    try {
      final res = await widget.api('photo_album', [
        'toggle_favorite',
        {'path': path},
      ]);

      if (!mounted) return;

      if (res is Map && res['ok'] == true) {
        final newFav = res['favorite'] == true;
        setState(() {
          photo['favorite'] = newFav;
          // 검색 결과와 연·월 그룹 모두 갱신
          for (final group in _groups) {
            final photos = group['photos'] as List<Map<String, Object?>>?;
            if (photos != null) {
              for (final p in photos) {
                if (p['path'] == path) {
                  p['favorite'] = newFav;
                }
              }
            }
          }
          if (_searchResults != null) {
            for (final p in _searchResults!) {
              if (p['path'] == path) {
                p['favorite'] = newFav;
              }
            }
          }
        });
      }
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('즐겨찾기 변경 실패: $e')),
        );
      }
    }
  }

  Future<void> _addTag(Map<String, Object?> photo, String tag) async {
    final path = '${photo['path'] ?? ''}';
    if (path.isEmpty || tag.trim().isEmpty) return;

    try {
      final res = await widget.api('photo_album', [
        'add_tag',
        {'path': path, 'tag': tag.trim()},
      ]);

      if (!mounted) return;

      if (res is Map && res['ok'] == true) {
        final rawTags = res['tags'];
        final tags = rawTags is List ? List<String>.from(rawTags) : <String>[];
        setState(() {
          photo['tags'] = tags;
          for (final group in _groups) {
            final photos = group['photos'] as List<Map<String, Object?>>?;
            if (photos != null) {
              for (final p in photos) {
                if (p['path'] == path) {
                  p['tags'] = tags;
                }
              }
            }
          }
          if (_searchResults != null) {
            for (final p in _searchResults!) {
              if (p['path'] == path) {
                p['tags'] = tags;
              }
            }
          }
        });
      }
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('태그 추가 실패: $e')),
        );
      }
    }
  }

  Future<void> _removeTag(Map<String, Object?> photo, String tag) async {
    final path = '${photo['path'] ?? ''}';
    if (path.isEmpty || tag.isEmpty) return;

    try {
      final res = await widget.api('photo_album', [
        'remove_tag',
        {'path': path, 'tag': tag},
      ]);

      if (!mounted) return;

      if (res is Map && res['ok'] == true) {
        final rawTags = res['tags'];
        final tags = rawTags is List ? List<String>.from(rawTags) : <String>[];
        setState(() {
          photo['tags'] = tags;
          for (final group in _groups) {
            final photos = group['photos'] as List<Map<String, Object?>>?;
            if (photos != null) {
              for (final p in photos) {
                if (p['path'] == path) {
                  p['tags'] = tags;
                }
              }
            }
          }
          if (_searchResults != null) {
            for (final p in _searchResults!) {
              if (p['path'] == path) {
                p['tags'] = tags;
              }
            }
          }
        });
      }
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('태그 삭제 실패: $e')),
        );
      }
    }
  }

  List<Map<String, Object?>> _getAllCurrentPhotos() {
    if (_searchResults != null) {
      return _searchResults!;
    }
    final all = <Map<String, Object?>>[];
    for (final group in _groups) {
      final photos = group['photos'] as List<Map<String, Object?>>?;
      if (photos != null) {
        all.addAll(photos);
      }
    }
    return all;
  }

  void _openViewerModal(int initialIndex) {
    final photos = _getAllCurrentPhotos();
    if (photos.isEmpty || initialIndex < 0 || initialIndex >= photos.length) return;

    showDialog<void>(
      context: context,
      barrierDismissible: true,
      builder: (context) => _PhotoViewerModal(
        initialIndex: initialIndex,
        photos: photos,
        thumbCache: _thumbCache,
        api: widget.api,
        onToggleFavorite: _toggleFavorite,
        onAddTag: _addTag,
        onRemoveTag: _removeTag,
      ),
    );
  }

  void _selectAll() {
    final all = _getAllCurrentPhotos();
    setState(() {
      _selectionMode = true;
      _selectedPaths.clear();
      for (final p in all) {
        final path = p['path'] as String?;
        if (path != null && path.isNotEmpty) {
          _selectedPaths.add(path);
        }
      }
    });
  }

  void _clearSelection() {
    setState(() {
      _selectedPaths.clear();
    });
  }

  Future<void> _startBatchExport() async {
    if (_selectedPaths.isEmpty) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('사본으로 저장할 사진을 선택해 주세요.')),
      );
      return;
    }

    final picker = widget.directoryPicker ?? getDirectoryPath;
    final outDir = await picker();
    if (outDir == null || outDir.isEmpty) return;

    if (!mounted) return;

    try {
      final res = await widget.api('photo_album', [
        'batch_export',
        {
          'paths': _selectedPaths.toList(),
          'output_dir': outDir,
          'max_width': 1280,
          'quality': 85,
        },
      ]);

      if (!mounted) return;

      if (res is Map && res['ok'] == true) {
        final count = (res['results'] as List?)?.length ?? _selectedPaths.length;
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('공문 첨부용 사본 $count장이 저장되었습니다.')),
        );
        _clearSelection();
      } else {
        final msg = res is Map ? res['message'] : '사본 저장에 실패했습니다.';
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('$msg')),
        );
      }
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('사본 저장 중 오류 발생: $e')),
        );
      }
    }
  }

  Future<void> _startBuildPhotoSheet() async {
    if (_selectedPaths.isEmpty) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('사진대지에 넣을 사진을 선택해 주세요.')),
      );
      return;
    }

    final selectedPhotos = <Map<String, Object?>>[];
    final all = _getAllCurrentPhotos();
    for (final p in all) {
      if (_selectedPaths.contains(p['path'])) {
        selectedPhotos.add(p);
      }
    }

    await showDialog<void>(
      context: context,
      builder: (context) => _PhotoSheetDialog(
        selectedPhotos: selectedPhotos,
        api: widget.api,
        fileSavePicker: widget.fileSavePicker,
        onSuccess: () {
          _clearSelection();
        },
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        final content = Scaffold(
          appBar: PreferredSize(
            preferredSize: const Size.fromHeight(60),
            child: _buildTopBar(context),
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

  Widget _buildTopBar(BuildContext context) {
    final theme = Theme.of(context);
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
      decoration: BoxDecoration(
        color: theme.colorScheme.surface,
        border: Border(
          bottom: BorderSide(
            color: theme.dividerColor.withValues(alpha: 0.2),
          ),
        ),
      ),
      child: Row(
        children: [
          FilledButton.icon(
            key: const ValueKey('photoAlbumPickFolderButton'),
            onPressed: _loading ? null : _pickFolder,
            icon: const Icon(Icons.folder_open, size: 18),
            label: const Text('사진 폴더 선택'),
          ),
          const SizedBox(width: 12),
          if (_currentFolder != null) ...[
            if (_registeredFolders.length > 1) ...[
              DropdownButton<String>(
                value: _registeredFolders.any((f) => f['path'] == _currentFolder)
                    ? _currentFolder
                    : null,
                icon: const Icon(Icons.arrow_drop_down),
                underline: const SizedBox(),
                items: [
                  for (final f in _registeredFolders)
                    DropdownMenuItem(
                      value: '${f['path']}',
                      child: Text(
                        '${f['path']}'.split(Platform.pathSeparator).last,
                        style: const TextStyle(fontSize: 13, fontWeight: FontWeight.w500),
                      ),
                    ),
                ],
                onChanged: _loading ? null : (val) {
                  if (val != null) _loadPhotos(val);
                },
              ),
              const SizedBox(width: 8),
            ] else ...[
              Flexible(
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    const Icon(Icons.photo_library_outlined, size: 18, color: Colors.grey),
                    const SizedBox(width: 6),
                    Flexible(
                      child: Text(
                        _currentFolder!,
                        overflow: TextOverflow.ellipsis,
                        style: const TextStyle(fontWeight: FontWeight.w500),
                      ),
                    ),
                  ],
                ),
              ),
              const SizedBox(width: 8),
            ],
            IconButton(
              key: const ValueKey('photoAlbumRefreshButton'),
              tooltip: '새로고침',
              icon: const Icon(Icons.refresh, size: 20),
              onPressed: _loading ? null : _refreshCurrentFolder,
            ),
          ],
          const Spacer(),
          // 검색창
          SizedBox(
            width: 200,
            height: 36,
            child: TextField(
              key: const ValueKey('photoAlbumSearchField'),
              controller: _searchController,
              decoration: InputDecoration(
                hintText: '파일명·태그 검색',
                hintStyle: const TextStyle(fontSize: 12),
                prefixIcon: const Icon(Icons.search, size: 18),
                suffixIcon: _searchController.text.isNotEmpty
                    ? IconButton(
                        key: const ValueKey('photoAlbumSearchClearButton'),
                        icon: const Icon(Icons.clear, size: 16),
                        onPressed: () {
                          _searchController.clear();
                          _performSearch();
                        },
                      )
                    : null,
                contentPadding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
                border: OutlineInputBorder(
                  borderRadius: BorderRadius.circular(8),
                  borderSide: BorderSide(color: theme.dividerColor),
                ),
              ),
              style: const TextStyle(fontSize: 13),
              onSubmitted: (_) => _performSearch(),
            ),
          ),
          const SizedBox(width: 8),
          IconButton(
            key: const ValueKey('photoAlbumSearchSubmitButton'),
            tooltip: '검색',
            icon: const Icon(Icons.search, size: 20),
            onPressed: _loading ? null : _performSearch,
          ),
          // 즐겨찾기 필터
          IconButton(
            key: const ValueKey('photoAlbumFavoriteFilterButton'),
            tooltip: _favoriteFilter ? '즐겨찾기만 보는 중' : '즐겨찾기만 보기',
            icon: Icon(
              _favoriteFilter ? Icons.star : Icons.star_border,
              color: _favoriteFilter ? Colors.amber : null,
              size: 22,
            ),
            onPressed: () {
              setState(() {
                _favoriteFilter = !_favoriteFilter;
              });
              _performSearch();
            },
          ),
          const SizedBox(width: 8),
          // 썸네일 크기 조절
          Tooltip(
            message: '썸네일 크기 조절',
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                const Icon(Icons.photo_size_select_actual_outlined, size: 16, color: Colors.grey),
                SizedBox(
                  width: 90,
                  child: Slider(
                    key: const ValueKey('photoAlbumThumbSizeSlider'),
                    value: _thumbSize,
                    min: 100,
                    max: 240,
                    divisions: 7,
                    onChanged: (val) {
                      setState(() {
                        _thumbSize = val;
                      });
                    },
                  ),
                ),
              ],
            ),
          ),
          const SizedBox(width: 8),
          // 선택 모드 토글
          IconButton(
            key: const ValueKey('photoAlbumToggleSelectionButton'),
            tooltip: _selectionMode ? '선택 모드 해제' : '다중 선택',
            icon: Icon(
              _selectionMode ? Icons.check_circle : Icons.check_circle_outline,
              color: _selectionMode ? theme.colorScheme.primary : null,
              size: 20,
            ),
            onPressed: () {
              setState(() {
                _selectionMode = !_selectionMode;
                if (!_selectionMode) {
                  _selectedPaths.clear();
                }
              });
            },
          ),
        ],
      ),
    );
  }

  Widget _buildSelectionBar(BuildContext context) {
    final theme = Theme.of(context);
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 6),
      color: theme.colorScheme.primaryContainer.withValues(alpha: 0.6),
      child: Row(
        children: [
          Icon(Icons.checklist, size: 18, color: theme.colorScheme.onPrimaryContainer),
          const SizedBox(width: 8),
          Text(
            '${_selectedPaths.length}장 선택됨',
            style: TextStyle(
              fontWeight: FontWeight.bold,
              fontSize: 13,
              color: theme.colorScheme.onPrimaryContainer,
            ),
          ),
          const SizedBox(width: 12),
          TextButton(
            key: const ValueKey('photoAlbumSelectAllButton'),
            onPressed: _selectAll,
            child: const Text('전체 선택', style: TextStyle(fontSize: 12)),
          ),
          TextButton(
            key: const ValueKey('photoAlbumClearSelectionButton'),
            onPressed: _clearSelection,
            child: const Text('선택 해제', style: TextStyle(fontSize: 12)),
          ),
          const Spacer(),
          OutlinedButton.icon(
            key: const ValueKey('photoAlbumBatchExportButton'),
            onPressed: _selectedPaths.isEmpty ? null : _startBatchExport,
            icon: const Icon(Icons.file_copy_outlined, size: 16),
            label: const Text('공문 첨부용 사본 저장', style: TextStyle(fontSize: 12)),
          ),
          const SizedBox(width: 8),
          FilledButton.icon(
            key: const ValueKey('photoAlbumBuildSheetButton'),
            onPressed: _selectedPaths.isEmpty ? null : _startBuildPhotoSheet,
            icon: const Icon(Icons.table_chart_outlined, size: 16),
            label: const Text('사진대지 만들기(2×2·2×3)', style: TextStyle(fontSize: 12)),
          ),
        ],
      ),
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
            Text('사진 목록을 불러오는 중...'),
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
                label: const Text('사진 폴더 선택'),
              ),
            ],
          ),
        ),
      );
    }

    if (_currentFolder == null && _registeredFolders.isEmpty) {
      return Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(
              Icons.photo_library_outlined,
              key: const ValueKey('photoAlbumEmptyFolderPrompt'),
              size: 64,
              color: Theme.of(context).colorScheme.outline.withValues(alpha: 0.5),
            ),
            const SizedBox(height: 16),
            const Text(
              '사진 폴더를 선택하거나 등록하세요.',
              style: TextStyle(fontSize: 16, fontWeight: FontWeight.w600),
            ),
            const SizedBox(height: 8),
            const Text(
              '사진을 연·월 묶음 격자로 감상하고, 크게 보기·태그·사진대지를 만듭니다.',
              style: TextStyle(color: Colors.grey, fontSize: 13),
            ),
            const SizedBox(height: 20),
            FilledButton.icon(
              onPressed: _pickFolder,
              icon: const Icon(Icons.folder_open),
              label: const Text('사진 폴더 선택'),
            ),
          ],
        ),
      );
    }

    final allPhotos = _getAllCurrentPhotos();
    if (allPhotos.isEmpty) {
      return Column(
        children: [
          if (_selectionMode) _buildSelectionBar(context),
          Expanded(
            child: Center(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Icon(
                    Icons.no_photography_outlined,
                    size: 56,
                    color: Theme.of(context).colorScheme.outline.withValues(alpha: 0.5),
                  ),
                  const SizedBox(height: 16),
                  const Text(
                    '등록된 사진이 없습니다.',
                    key: ValueKey('photoAlbumEmptyPhotosText'),
                    style: TextStyle(fontSize: 15, fontWeight: FontWeight.w600),
                  ),
                  const SizedBox(height: 6),
                  const Text(
                    '폴더에 사진(JPG·PNG·WEBP·BMP·HEIC)이 없거나 검색 결과가 없습니다.',
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
            ),
          ),
        ],
      );
    }

    return Column(
      children: [
        if (_selectionMode) _buildSelectionBar(context),
        Expanded(
          child: _searchResults != null
              ? _buildSearchResultsGrid()
              : _buildGroupedDateGrid(),
        ),
      ],
    );
  }

  Widget _buildSearchResultsGrid() {
    final results = _searchResults ?? [];
    return Scrollbar(
      child: CustomScrollView(
        key: const ValueKey('photoAlbumSearchResultsView'),
        slivers: [
          SliverToBoxAdapter(
            child: Padding(
              padding: const EdgeInsets.fromLTRB(16, 16, 16, 8),
              child: Row(
                children: [
                  const Icon(Icons.search, size: 18, color: Colors.blueAccent),
                  const SizedBox(width: 8),
                  Text(
                    '검색 결과: ${results.length}장',
                    style: const TextStyle(fontWeight: FontWeight.bold, fontSize: 14),
                  ),
                  const Spacer(),
                  TextButton.icon(
                    icon: const Icon(Icons.close, size: 16),
                    label: const Text('검색 닫기', style: TextStyle(fontSize: 12)),
                    onPressed: () {
                      _searchController.clear();
                      setState(() {
                        _searchResults = null;
                      });
                    },
                  ),
                ],
              ),
            ),
          ),
          SliverPadding(
            padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
            sliver: SliverGrid(
              gridDelegate: SliverGridDelegateWithMaxCrossAxisExtent(
                maxCrossAxisExtent: _thumbSize,
                childAspectRatio: 0.8,
                crossAxisSpacing: 10,
                mainAxisSpacing: 10,
              ),
              delegate: SliverChildBuilderDelegate(
                (context, index) {
                  final photo = results[index];
                  final path = '${photo['path'] ?? ''}';
                  final selected = _selectedPaths.contains(path);
                  return _PhotoCard(
                    key: ValueKey('photoCard-$path'),
                    photo: photo,
                    api: widget.api,
                    cachedThumb: _thumbCache[path],
                    selectionMode: _selectionMode,
                    isSelected: selected,
                    onThumbLoaded: (bytes) {
                      _thumbCache[path] = bytes;
                    },
                    onTap: () {
                      if (_selectionMode) {
                        setState(() {
                          if (selected) {
                            _selectedPaths.remove(path);
                          } else {
                            _selectedPaths.add(path);
                          }
                        });
                      } else {
                        _openViewerModal(index);
                      }
                    },
                    onToggleFavorite: () => _toggleFavorite(photo),
                  );
                },
                childCount: results.length,
              ),
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildGroupedDateGrid() {
    var globalIndexCounter = 0;
    return Scrollbar(
      child: CustomScrollView(
        key: const ValueKey('photoAlbumGroupedGridView'),
        slivers: [
          for (final group in _groups) ...[
            SliverToBoxAdapter(
              child: Padding(
                padding: const EdgeInsets.fromLTRB(16, 20, 16, 8),
                child: Row(
                  children: [
                    const Icon(Icons.calendar_month_outlined, size: 18, color: Colors.blueAccent),
                    const SizedBox(width: 8),
                    Text(
                      '${group['label'] ?? '기타'} (${(group['photos'] as List?)?.length ?? 0}장)',
                      key: ValueKey('photoGroupHeader-${group['label']}'),
                      style: const TextStyle(fontWeight: FontWeight.bold, fontSize: 14),
                    ),
                  ],
                ),
              ),
            ),
            SliverPadding(
              padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 4),
              sliver: SliverGrid(
                gridDelegate: SliverGridDelegateWithMaxCrossAxisExtent(
                  maxCrossAxisExtent: _thumbSize,
                  childAspectRatio: 0.8,
                  crossAxisSpacing: 10,
                  mainAxisSpacing: 10,
                ),
                delegate: SliverChildBuilderDelegate(
                  (context, index) {
                    final photos = (group['photos'] as List<Map<String, Object?>>?) ?? [];
                    final photo = photos[index];
                    final path = '${photo['path'] ?? ''}';
                    final selected = _selectedPaths.contains(path);
                    final thisGlobalIndex = globalIndexCounter++;
                    return _PhotoCard(
                      key: ValueKey('photoCard-$path'),
                      photo: photo,
                      api: widget.api,
                      cachedThumb: _thumbCache[path],
                      selectionMode: _selectionMode,
                      isSelected: selected,
                      onThumbLoaded: (bytes) {
                        _thumbCache[path] = bytes;
                      },
                      onTap: () {
                        if (_selectionMode) {
                          setState(() {
                            if (selected) {
                              _selectedPaths.remove(path);
                            } else {
                              _selectedPaths.add(path);
                            }
                          });
                        } else {
                          _openViewerModal(thisGlobalIndex);
                        }
                      },
                      onToggleFavorite: () => _toggleFavorite(photo),
                    );
                  },
                  childCount: ((group['photos'] as List?)?.length ?? 0),
                ),
              ),
            ),
          ],
          const SliverToBoxAdapter(
            child: SizedBox(height: 32),
          ),
        ],
      ),
    );
  }
}

/// 사진 카드 위젯 (화면에 보일 때 비동기 썸네일 요청).
class _PhotoCard extends StatefulWidget {
  const _PhotoCard({
    super.key,
    required this.photo,
    required this.api,
    required this.onTap,
    required this.onToggleFavorite,
    this.cachedThumb,
    this.onThumbLoaded,
    this.selectionMode = false,
    this.isSelected = false,
  });

  final Map<String, Object?> photo;
  final ApiCall api;
  final VoidCallback onTap;
  final VoidCallback onToggleFavorite;
  final Uint8List? cachedThumb;
  final ValueChanged<Uint8List>? onThumbLoaded;
  final bool selectionMode;
  final bool isSelected;

  @override
  State<_PhotoCard> createState() => _PhotoCardState();
}

class _PhotoCardState extends State<_PhotoCard> {
  Uint8List? _thumbBytes;
  bool _loadingThumb = false;

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
  void didUpdateWidget(covariant _PhotoCard oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.cachedThumb != null && widget.cachedThumb != _thumbBytes) {
      _thumbBytes = widget.cachedThumb;
    }
  }

  Future<void> _requestThumbnail() async {
    final path = '${widget.photo['path'] ?? ''}';
    if (path.isEmpty || _loadingThumb || _thumbBytes != null) return;

    if (widget.photo['cloud'] == true) {
      return;
    }

    setState(() {
      _loadingThumb = true;
    });

    try {
      final res = await widget.api('photo_album', [
        'thumbnail',
        {'path': path, 'size': 160, 'as_base64': true},
      ]);

      if (!mounted) return;

      if (res is Map && res['ok'] == true && res['data'] != null) {
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
        });
      }
    } catch (_) {
      if (mounted) {
        setState(() {
          _loadingThumb = false;
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final isFav = widget.photo['favorite'] == true;
    final filename = '${widget.photo['filename'] ?? ''}';
    final path = '${widget.photo['path'] ?? ''}';
    final dateTaken = widget.photo['date_taken'] as String?;

    return InkWell(
      onTap: widget.onTap,
      borderRadius: BorderRadius.circular(8),
      child: Container(
        decoration: BoxDecoration(
          color: theme.colorScheme.surface,
          borderRadius: BorderRadius.circular(8),
          border: Border.all(
            color: widget.isSelected
                ? theme.colorScheme.primary
                : theme.dividerColor.withValues(alpha: 0.3),
            width: widget.isSelected ? 2.5 : 1.0,
          ),
          boxShadow: [
            BoxShadow(
              color: Colors.black.withValues(alpha: 0.04),
              blurRadius: 4,
              offset: const Offset(0, 2),
            ),
          ],
        ),
        clipBehavior: Clip.antiAlias,
        child: Stack(
          fit: StackFit.expand,
          children: [
            Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Expanded(
                  child: Container(
                    color: theme.colorScheme.surfaceContainerHighest.withValues(alpha: 0.3),
                    child: _buildThumbnail(),
                  ),
                ),
                Padding(
                  padding: const EdgeInsets.fromLTRB(8, 4, 8, 4),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Text(
                        filename,
                        overflow: TextOverflow.ellipsis,
                        maxLines: 1,
                        style: const TextStyle(fontSize: 11, fontWeight: FontWeight.w600),
                      ),
                      if (dateTaken != null && dateTaken.isNotEmpty)
                        Text(
                          dateTaken.split(' ').first,
                          style: TextStyle(
                            fontSize: 10,
                            color: theme.colorScheme.onSurface.withValues(alpha: 0.6),
                          ),
                        ),
                    ],
                  ),
                ),
              ],
            ),
            // 좌상단 체크박스 (선택 모드이거나 선택되었을 때)
            if (widget.selectionMode || widget.isSelected)
              Positioned(
                top: 4,
                left: 4,
                child: Container(
                  decoration: BoxDecoration(
                    color: widget.isSelected ? theme.colorScheme.primary : Colors.black45,
                    shape: BoxShape.circle,
                  ),
                  padding: const EdgeInsets.all(2),
                  child: Icon(
                    widget.isSelected ? Icons.check : Icons.circle_outlined,
                    size: 16,
                    color: Colors.white,
                  ),
                ),
              ),
            // 우상단 즐겨찾기 별표
            Positioned(
              top: 2,
              right: 2,
              child: Material(
                color: Colors.transparent,
                child: IconButton(
                  key: ValueKey('photoCardFavorite-$path'),
                  iconSize: 18,
                  padding: EdgeInsets.zero,
                  constraints: const BoxConstraints(minWidth: 26, minHeight: 26),
                  icon: Icon(
                    isFav ? Icons.star : Icons.star_border,
                    color: isFav ? Colors.amber : Colors.white.withValues(alpha: 0.8),
                    shadows: const [
                      Shadow(color: Colors.black45, blurRadius: 4),
                    ],
                  ),
                  onPressed: widget.onToggleFavorite,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildThumbnail() {
    if (_thumbBytes != null) {
      return Image.memory(
        _thumbBytes!,
        fit: BoxFit.cover,
        errorBuilder: (_, _, _) => const Center(
          child: Icon(Icons.broken_image_outlined, size: 32, color: Colors.grey),
        ),
      );
    }

    if (_loadingThumb) {
      return const Center(
        child: SizedBox(
          width: 20,
          height: 20,
          child: CircularProgressIndicator(strokeWidth: 2),
        ),
      );
    }

    return const Center(
      child: Icon(Icons.image_outlined, size: 32, color: Colors.grey),
    );
  }
}

/// 사진 크게 보기 모달 대화상자 (키보드 좌/우 화살표 넘기기, Esc 닫기).
class _PhotoViewerModal extends StatefulWidget {
  const _PhotoViewerModal({
    required this.initialIndex,
    required this.photos,
    required this.thumbCache,
    required this.api,
    required this.onToggleFavorite,
    required this.onAddTag,
    required this.onRemoveTag,
  });

  final int initialIndex;
  final List<Map<String, Object?>> photos;
  final Map<String, Uint8List> thumbCache;
  final ApiCall api;
  final Future<void> Function(Map<String, Object?> photo) onToggleFavorite;
  final Future<void> Function(Map<String, Object?> photo, String tag) onAddTag;
  final Future<void> Function(Map<String, Object?> photo, String tag) onRemoveTag;

  @override
  State<_PhotoViewerModal> createState() => _PhotoViewerModalState();
}

class _PhotoViewerModalState extends State<_PhotoViewerModal> {
  late int _currentIndex;
  final FocusNode _focusNode = FocusNode();
  final TextEditingController _tagInputController = TextEditingController();

  @override
  void initState() {
    super.initState();
    _currentIndex = widget.initialIndex;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _focusNode.requestFocus();
    });
  }

  @override
  void dispose() {
    _focusNode.dispose();
    _tagInputController.dispose();
    super.dispose();
  }

  void _prevPhoto() {
    if (_currentIndex > 0) {
      setState(() {
        _currentIndex--;
      });
    }
  }

  void _nextPhoto() {
    if (_currentIndex < widget.photos.length - 1) {
      setState(() {
        _currentIndex++;
      });
    }
  }

  KeyEventResult _handleKeyEvent(FocusNode node, KeyEvent event) {
    if (event is KeyDownEvent) {
      if (event.logicalKey == LogicalKeyboardKey.arrowLeft) {
        _prevPhoto();
        return KeyEventResult.handled;
      } else if (event.logicalKey == LogicalKeyboardKey.arrowRight) {
        _nextPhoto();
        return KeyEventResult.handled;
      } else if (event.logicalKey == LogicalKeyboardKey.escape) {
        Navigator.of(context).pop();
        return KeyEventResult.handled;
      }
    }
    return KeyEventResult.ignored;
  }

  @override
  Widget build(BuildContext context) {
    final photo = widget.photos[_currentIndex];
    final path = '${photo['path'] ?? ''}';
    final filename = '${photo['filename'] ?? ''}';
    final dateTaken = photo['date_taken'] as String? ?? '-';
    final size = photo['size'] is int ? _formatBytes(photo['size'] as int) : '-';
    final isFav = photo['favorite'] == true;
    final tags = (photo['tags'] as List?)?.map((e) => '$e').toList() ?? <String>[];

    final thumbBytes = widget.thumbCache[path];

    return Focus(
      focusNode: _focusNode,
      autofocus: true,
      onKeyEvent: _handleKeyEvent,
      child: Dialog(
        key: const ValueKey('photoViewerModal'),
        backgroundColor: const Color(0xFF1E222A),
        insetPadding: const EdgeInsets.all(16),
        child: Container(
          width: 900,
          height: 680,
          decoration: BoxDecoration(
            color: const Color(0xFF1E222A),
            borderRadius: BorderRadius.circular(12),
          ),
          child: Column(
            children: [
              // 상단 바
              Container(
                padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 10),
                decoration: const BoxDecoration(
                  border: Border(bottom: BorderSide(color: Colors.white12)),
                ),
                child: Row(
                  children: [
                    Flexible(
                      child: Text(
                        filename,
                        overflow: TextOverflow.ellipsis,
                        style: const TextStyle(
                          color: Colors.white,
                          fontSize: 15,
                          fontWeight: FontWeight.bold,
                        ),
                      ),
                    ),
                    const SizedBox(width: 12),
                    Text(
                      '${_currentIndex + 1} / ${widget.photos.length}',
                      key: const ValueKey('photoViewerIndexText'),
                      style: const TextStyle(color: Colors.white60, fontSize: 13),
                    ),
                    const Spacer(),
                    IconButton(
                      key: const ValueKey('photoViewerFavoriteButton'),
                      tooltip: '즐겨찾기 토글',
                      icon: Icon(
                        isFav ? Icons.star : Icons.star_border,
                        color: isFav ? Colors.amber : Colors.white70,
                      ),
                      onPressed: () async {
                        await widget.onToggleFavorite(photo);
                        setState(() {});
                      },
                    ),
                    IconButton(
                      key: const ValueKey('photoViewerCloseButton'),
                      tooltip: '닫기 (Esc)',
                      icon: const Icon(Icons.close, color: Colors.white70),
                      onPressed: () => Navigator.of(context).pop(),
                    ),
                  ],
                ),
              ),
              // 중앙 사진 뷰
              Expanded(
                child: Stack(
                  children: [
                    Center(
                      child: thumbBytes != null
                          ? Image.memory(
                              thumbBytes,
                              fit: BoxFit.contain,
                              errorBuilder: (_, _, _) => const Center(
                                child: Text('사진을 표시할 수 없습니다.', style: TextStyle(color: Colors.white54)),
                              ),
                            )
                          : const Center(
                              child: Icon(Icons.photo, size: 64, color: Colors.white30),
                            ),
                    ),
                    // 이전 버튼
                    if (_currentIndex > 0)
                      Positioned(
                        left: 12,
                        top: 0,
                        bottom: 0,
                        child: Center(
                          child: CircleAvatar(
                            backgroundColor: Colors.black45,
                            child: IconButton(
                              key: const ValueKey('photoViewerPrevButton'),
                              icon: const Icon(Icons.arrow_back_ios_new, color: Colors.white, size: 20),
                              onPressed: _prevPhoto,
                            ),
                          ),
                        ),
                      ),
                    // 다음 버튼
                    if (_currentIndex < widget.photos.length - 1)
                      Positioned(
                        right: 12,
                        top: 0,
                        bottom: 0,
                        child: Center(
                          child: CircleAvatar(
                            backgroundColor: Colors.black45,
                            child: IconButton(
                              key: const ValueKey('photoViewerNextButton'),
                              icon: const Icon(Icons.arrow_forward_ios, color: Colors.white, size: 20),
                              onPressed: _nextPhoto,
                            ),
                          ),
                        ),
                      ),
                  ],
                ),
              ),
              // 하단 메타데이터 및 태그 영역
              Container(
                padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
                decoration: const BoxDecoration(
                  color: Color(0xFF15181E),
                  border: Border(top: BorderSide(color: Colors.white12)),
                  borderRadius: BorderRadius.vertical(bottom: Radius.circular(12)),
                ),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Row(
                      children: [
                        const Icon(Icons.calendar_today, size: 14, color: Colors.white54),
                        const SizedBox(width: 6),
                        Text(
                          '촬영일: $dateTaken',
                          style: const TextStyle(color: Colors.white70, fontSize: 12),
                        ),
                        const SizedBox(width: 16),
                        const Icon(Icons.data_usage, size: 14, color: Colors.white54),
                        const SizedBox(width: 6),
                        Text(
                          '크기: $size',
                          style: const TextStyle(color: Colors.white70, fontSize: 12),
                        ),
                        const Spacer(),
                        Text(
                          '좌우 화살표: 넘기기 | Esc: 닫기',
                          style: TextStyle(color: Colors.white.withValues(alpha: 0.4), fontSize: 11),
                        ),
                      ],
                    ),
                    const SizedBox(height: 8),
                    // 태그 목록 및 추가 입력
                    Row(
                      children: [
                        const Icon(Icons.label_outline, size: 16, color: Colors.white70),
                        const SizedBox(width: 8),
                        Expanded(
                          child: Wrap(
                            spacing: 6,
                            runSpacing: 4,
                            crossAxisAlignment: WrapCrossAlignment.center,
                            children: [
                              for (final tag in tags)
                                Chip(
                                  key: ValueKey('photoViewerTagChip-$tag'),
                                  label: Text(tag, style: const TextStyle(fontSize: 11)),
                                  visualDensity: VisualDensity.compact,
                                  padding: const EdgeInsets.symmetric(horizontal: 4),
                                  deleteIcon: const Icon(Icons.close, size: 12),
                                  onDeleted: () async {
                                    await widget.onRemoveTag(photo, tag);
                                    setState(() {});
                                  },
                                ),
                              SizedBox(
                                width: 120,
                                height: 28,
                                child: TextField(
                                  key: const ValueKey('photoViewerTagField'),
                                  controller: _tagInputController,
                                  style: const TextStyle(color: Colors.white, fontSize: 11),
                                  decoration: InputDecoration(
                                    hintText: '태그 추가',
                                    hintStyle: TextStyle(
                                      color: Colors.white.withValues(alpha: 0.4),
                                      fontSize: 11,
                                    ),
                                    contentPadding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
                                    border: OutlineInputBorder(
                                      borderRadius: BorderRadius.circular(6),
                                      borderSide: const BorderSide(color: Colors.white24),
                                    ),
                                  ),
                                  onSubmitted: (val) async {
                                    if (val.trim().isNotEmpty) {
                                      await widget.onAddTag(photo, val);
                                      _tagInputController.clear();
                                      setState(() {});
                                    }
                                  },
                                ),
                              ),
                              IconButton(
                                key: const ValueKey('photoViewerAddTagButton'),
                                icon: const Icon(Icons.add, size: 18, color: Colors.white70),
                                constraints: const BoxConstraints(minWidth: 28, minHeight: 28),
                                padding: EdgeInsets.zero,
                                onPressed: () async {
                                  final val = _tagInputController.text.trim();
                                  if (val.isNotEmpty) {
                                    await widget.onAddTag(photo, val);
                                    _tagInputController.clear();
                                    setState(() {});
                                  }
                                },
                              ),
                            ],
                          ),
                        ),
                      ],
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

/// 사진대지 생성 대화상자 (2×2, 2×3 레이아웃, 문서 제목, 파일 경로).
class _PhotoSheetDialog extends StatefulWidget {
  const _PhotoSheetDialog({
    required this.selectedPhotos,
    required this.api,
    this.fileSavePicker,
    required this.onSuccess,
  });

  final List<Map<String, Object?>> selectedPhotos;
  final ApiCall api;
  final Future<String?> Function({String? suggestedName})? fileSavePicker;
  final VoidCallback onSuccess;

  @override
  State<_PhotoSheetDialog> createState() => _PhotoSheetDialogState();
}

class _PhotoSheetDialogState extends State<_PhotoSheetDialog> {
  String _layout = '2x2';
  final TextEditingController _titleController = TextEditingController(text: '사 진 대 지');
  final TextEditingController _outputPathController = TextEditingController();
  bool _creating = false;
  String? _errorMessage;

  @override
  void initState() {
    super.initState();
    final firstFolder = widget.selectedPhotos.isNotEmpty
        ? widget.selectedPhotos.first['folder'] as String?
        : null;
    final defaultPath = firstFolder != null
        ? '$firstFolder${Platform.pathSeparator}사진대지.hwpx'
        : '사진대지.hwpx';
    _outputPathController.text = defaultPath;
  }

  @override
  void dispose() {
    _titleController.dispose();
    _outputPathController.dispose();
    super.dispose();
  }

  Future<void> _pickSavePath() async {
    if (widget.fileSavePicker != null) {
      final res = await widget.fileSavePicker!(suggestedName: '사진대지.hwpx');
      if (res != null && res.isNotEmpty) {
        setState(() {
          _outputPathController.text = res;
        });
      }
    }
  }

  Future<void> _submit() async {
    final title = _titleController.text.trim();
    final outputPath = _outputPathController.text.trim();
    if (outputPath.isEmpty) {
      setState(() {
        _errorMessage = '저장할 파일 경로를 입력하세요.';
      });
      return;
    }

    setState(() {
      _creating = true;
      _errorMessage = null;
    });

    try {
      final items = [
        for (final p in widget.selectedPhotos)
          {
            'path': '${p['path'] ?? ''}',
            'caption': '${p['filename'] ?? ''}',
            'date_taken': '${p['date_taken'] ?? ''}',
          },
      ];

      final res = await widget.api('photo_album', [
        'build_photo_sheet',
        {
          'items': items,
          'output_path': outputPath,
          'layout': _layout,
          'title': title.isEmpty ? '사 진 대 지' : title,
        },
      ]);

      if (!mounted) return;

      if (res is Map && res['ok'] == true) {
        Navigator.of(context).pop();
        widget.onSuccess();
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('사진대지 HWPX가 생성되었습니다: $outputPath')),
        );
      } else {
        final msg = res is Map ? res['message'] : '사진대지 생성 실패';
        setState(() {
          _errorMessage = '$msg';
          _creating = false;
        });
      }
    } catch (e) {
      if (mounted) {
        setState(() {
          _errorMessage = '사진대지 생성 오류: $e';
          _creating = false;
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: const Text('사진대지 만들기'),
      content: SizedBox(
        width: 440,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              '선택한 ${widget.selectedPhotos.length}장의 사진으로 한/글 사진대지(HWPX)를 생성합니다.',
              style: const TextStyle(fontSize: 13, color: Colors.grey),
            ),
            const SizedBox(height: 16),
            const Text('레이아웃 선택', style: TextStyle(fontWeight: FontWeight.bold, fontSize: 13)),
            const SizedBox(height: 6),
            Row(
              children: [
                Expanded(
                  child: RadioListTile<String>(
                    key: const ValueKey('photoSheetLayout2x2'),
                    title: const Text('2×2 (4장/쪽)', style: TextStyle(fontSize: 13)),
                    value: '2x2',
                    groupValue: _layout,
                    onChanged: (val) {
                      if (val != null) setState(() => _layout = val);
                    },
                  ),
                ),
                Expanded(
                  child: RadioListTile<String>(
                    key: const ValueKey('photoSheetLayout2x3'),
                    title: const Text('2×3 (6장/쪽)', style: TextStyle(fontSize: 13)),
                    value: '2x3',
                    groupValue: _layout,
                    onChanged: (val) {
                      if (val != null) setState(() => _layout = val);
                    },
                  ),
                ),
              ],
            ),
            const SizedBox(height: 12),
            TextField(
              key: const ValueKey('photoSheetTitleField'),
              controller: _titleController,
              decoration: const InputDecoration(
                labelText: '문서 제목',
                border: OutlineInputBorder(),
                isDense: true,
              ),
            ),
            const SizedBox(height: 12),
            Row(
              children: [
                Expanded(
                  child: TextField(
                    key: const ValueKey('photoSheetOutputPathField'),
                    controller: _outputPathController,
                    decoration: const InputDecoration(
                      labelText: '저장 파일 경로 (.hwpx)',
                      border: OutlineInputBorder(),
                      isDense: true,
                    ),
                  ),
                ),
                if (widget.fileSavePicker != null) ...[
                  const SizedBox(width: 8),
                  IconButton(
                    icon: const Icon(Icons.folder_open),
                    onPressed: _pickSavePath,
                  ),
                ],
              ],
            ),
            if (_errorMessage != null) ...[
              const SizedBox(height: 12),
              Text(
                _errorMessage!,
                style: const TextStyle(color: Colors.redAccent, fontSize: 12),
              ),
            ],
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: _creating ? null : () => Navigator.of(context).pop(),
          child: const Text('취소'),
        ),
        FilledButton(
          key: const ValueKey('photoAlbumConfirmBuildSheetButton'),
          onPressed: _creating ? null : _submit,
          child: _creating
              ? const SizedBox(
                  width: 16,
                  height: 16,
                  child: CircularProgressIndicator(strokeWidth: 2, color: Colors.white),
                )
              : const Text('생성'),
        ),
      ],
    );
  }
}
