import 'package:file_selector/file_selector.dart';
import 'package:flutter/foundation.dart' show kIsWeb;
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../core/api/abbreviation_api.dart' show ApiCall;
import 'file_selection.dart';

String _baseName(String path) => path.split(RegExp(r'[\\/]')).last;

/// 문서뷰·사진뷰 공용 파일 작업 영역(FILEOPS1).
///
/// 선택 개수와 작업 단추 줄을 목록 위에 두고, 목록에 초점이 있을 때
/// Ctrl+A·Esc·Ctrl+C·Delete·F2 를 처리한다. 작업은 백엔드가 배경에서 하고
/// 이 위젯은 진행 상태만 물어보므로 화면은 멈추지 않는다.
class FileOpsArea extends StatefulWidget {
  const FileOpsArea({
    super.key,
    required this.api,
    required this.method,
    required this.selection,
    required this.order,
    required this.onSelectionChanged,
    required this.onApplied,
    required this.child,
    this.directoryPicker,
  });

  final ApiCall api;

  /// 백엔드 이름: `doc_grid` 또는 `photo_album`.
  final String method;
  final FileSelection selection;
  final List<String> order;
  final VoidCallback onSelectionChanged;

  /// 작업이 끝나면 파일별 결과로 목록·썸네일을 고친다. 이 위젯이 사라진 뒤에도 불릴 수 있다.
  final void Function(String op, List<Map<String, Object?>> results) onApplied;
  final Widget child;
  final Future<String?> Function()? directoryPicker;

  @override
  State<FileOpsArea> createState() => _FileOpsAreaState();
}

class _FileOpsAreaState extends State<FileOpsArea> {
  final FocusNode _focus = FocusNode(debugLabel: 'fileOpsArea');

  /// 확인창·폴더 선택·작업 중. 같은 작업이 겹쳐 시작되지 않게 한다.
  bool _busy = false;
  String? _progress;

  @override
  void dispose() {
    _focus.dispose();
    super.dispose();
  }

  List<String> get _targets => widget.selection.visible(widget.order);

  void _selectAll() {
    widget.selection.selectAll(widget.order);
    widget.onSelectionChanged();
  }

  void _clear() {
    widget.selection.clear();
    widget.onSelectionChanged();
  }

  KeyEventResult _onKey(FocusNode node, KeyEvent event) {
    if (event is! KeyDownEvent) return KeyEventResult.ignored;
    final key = event.logicalKey;
    final control = FileSelection.toggleKeyPressed;
    if (control && key == LogicalKeyboardKey.keyA) {
      _selectAll();
    } else if (control && key == LogicalKeyboardKey.keyC && _targets.isNotEmpty) {
      _run('clipboard');
    } else if (key == LogicalKeyboardKey.escape &&
        widget.selection.paths.isNotEmpty) {
      _clear();
    } else if (key == LogicalKeyboardKey.delete && _targets.isNotEmpty) {
      _run('delete');
    } else if (key == LogicalKeyboardKey.f2 && _targets.length == 1) {
      _run('rename');
    } else {
      return KeyEventResult.ignored;
    }
    return KeyEventResult.handled;
  }

  Future<String?> _pickDestination() async {
    final picker = widget.directoryPicker;
    if (picker != null) return picker();
    // 웹 화면(WebView)에는 폴더 선택창이 없어 백엔드가 띄운다.
    if (!kIsWeb) return getDirectoryPath();
    final res = await widget.api(widget.method, ['pick_destination', {}]);
    if (res is Map && res['ok'] == true) return res['path'] as String?;
    throw StateError(
      '${res is Map ? res['message'] : '폴더 선택창을 열지 못했습니다.'}',
    );
  }

  Future<void> _run(String op) async {
    final targets = _targets;
    if (_busy || targets.isEmpty || (op == 'rename' && targets.length != 1)) {
      return;
    }
    // 작업 중 화면이 바뀌어도 결과를 알릴 수 있게 미리 잡아 둔다.
    final messenger = ScaffoldMessenger.of(context);
    final navigator = Navigator.of(context, rootNavigator: true);
    final api = widget.api;
    final method = widget.method;
    final selection = widget.selection;
    final onApplied = widget.onApplied;
    void say(String message) {
      if (messenger.mounted) {
        messenger.showSnackBar(SnackBar(content: Text(message)));
      }
    }

    void show(String? progress) {
      if (mounted) setState(() => _progress = progress);
    }

    setState(() => _busy = true);
    try {
      final payload = <String, Object?>{'op': op, 'paths': targets};
      if (op == 'delete') {
        if (!await _confirmDelete(targets)) return;
      } else if (op == 'rename') {
        final name = await showDialog<String>(
          context: context,
          builder: (_) => _RenameDialog(fileName: _baseName(targets.single)),
        );
        if (name == null) return;
        payload['new_name'] = name;
      } else if (op == 'copy' || op == 'move') {
        final destination = await _pickDestination();
        if (destination == null || destination.isEmpty) return;
        payload['destination'] = destination;
      }

      show('파일 작업을 시작합니다…');
      final started = await api(method, ['file_op_start', payload]);
      if (started is! Map || started['ok'] != true) {
        say('${started is Map ? started['message'] : '파일 작업을 시작하지 못했습니다.'}');
        return;
      }
      Map<Object?, Object?> state;
      while (true) {
        await Future<void>.delayed(const Duration(milliseconds: 120));
        final progress = await api(method, [
          'file_op_progress',
          {'task_id': started['task_id']},
        ]);
        if (progress is! Map || progress['ok'] != true) {
          say('파일 작업 상태를 확인하지 못했습니다. 새로고침해 결과를 확인해 주세요.');
          return;
        }
        if (progress['state'] == 'complete') {
          state = progress;
          break;
        }
        show('파일 작업 중 ${progress['done'] ?? 0}/${progress['total'] ?? targets.length}');
      }

      show(null); // 작업은 끝났다: 안내창 뒤에서 진행 표시가 계속 돌지 않게 한다.
      final results = [
        for (final item in (state['results'] as List? ?? const []))
          if (item is Map) Map<String, Object?>.from(item),
      ];
      selection.applyResults(op, results);
      onApplied(op, results);
      final failed = results.where((r) => r['ok'] != true).toList();
      final succeeded = results.length - failed.length;
      if (succeeded > 0) {
        say(
          switch (op) {
            'clipboard' => '$succeeded개 파일을 클립보드에 복사했습니다.',
            'copy' => '$succeeded개 파일을 복사했습니다.',
            'move' => '$succeeded개 파일을 옮겼습니다.',
            'delete' => '$succeeded개 파일을 휴지통으로 보냈습니다.',
            _ => '이름을 바꿨습니다.',
          },
        );
      }
      final warned = results.where((r) => r['warning'] != null).toList();
      if ((failed.isNotEmpty || warned.isNotEmpty) && navigator.mounted) {
        await _showProblems(navigator.context, succeeded, failed, warned);
      }
    } catch (e) {
      say('파일 작업 중 오류가 발생했습니다: $e');
    } finally {
      if (mounted) {
        setState(() {
          _busy = false;
          _progress = null;
        });
        _focus.requestFocus();
      }
    }
  }

  Future<bool> _confirmDelete(List<String> targets) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        key: const ValueKey('fileOpDeleteDialog'),
        title: const Text('휴지통으로 보내기'),
        content: SizedBox(
          width: 420,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text('파일 ${targets.length}개를 휴지통으로 보낼까요?'),
              const SizedBox(height: 8),
              ConstrainedBox(
                constraints: const BoxConstraints(maxHeight: 240),
                child: SizedBox(
                  height: 22.0 * targets.length,
                  child: ListView.builder(
                    itemExtent: 22,
                    itemCount: targets.length,
                    itemBuilder: (_, index) => Tooltip(
                      message: targets[index],
                      child: Text(
                        _baseName(targets[index]),
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                      ),
                    ),
                  ),
                ),
              ),
            ],
          ),
        ),
        actions: [
          // 초점은 취소에 둔다: Delete 뒤 Enter 를 눌러도 지워지지 않는다.
          TextButton(
            key: const ValueKey('fileOpDeleteCancel'),
            autofocus: true,
            onPressed: () => Navigator.pop(dialogContext, false),
            child: const Text('취소'),
          ),
          FilledButton(
            key: const ValueKey('fileOpDeleteConfirm'),
            onPressed: () => Navigator.pop(dialogContext, true),
            child: const Text('휴지통으로 보내기'),
          ),
        ],
      ),
    );
    return confirmed == true;
  }

  Future<void> _showProblems(
    BuildContext context,
    int succeeded,
    List<Map<String, Object?>> failed,
    List<Map<String, Object?>> warned,
  ) {
    return showDialog<void>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        key: const ValueKey('fileOpProblemDialog'),
        title: Text('성공 $succeeded개 · 실패 ${failed.length}개'),
        content: SizedBox(
          width: 480,
          child: ListView(
            shrinkWrap: true,
            children: [
              for (final r in [...failed, ...warned])
                ListTile(
                  dense: true,
                  title: Text('${r['name']}'),
                  subtitle: Text('${r['ok'] == true ? r['warning'] : r['message']}'),
                ),
            ],
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(dialogContext),
            child: const Text('닫기'),
          ),
        ],
      ),
    );
  }

  Widget _button(String key, IconData icon, String label, VoidCallback? onPressed) {
    return TextButton.icon(
      key: ValueKey(key),
      onPressed: onPressed,
      icon: Icon(icon, size: 16),
      label: Text(label, style: const TextStyle(fontSize: 12)),
      style: TextButton.styleFrom(visualDensity: VisualDensity.compact),
    );
  }

  Widget _buildBar(BuildContext context) {
    final count = _targets.length;
    final any = !_busy && count > 0;
    // 줄은 늘 같은 높이로 둔다: 클릭으로 목록이 밀리면 더블클릭이 빗나간다.
    return SizedBox(
      height: 40,
      child: SingleChildScrollView(
        scrollDirection: Axis.horizontal,
        padding: const EdgeInsets.symmetric(horizontal: 12),
        child: Row(
          children: [
            Text(
              '$count개 선택',
              key: const ValueKey('fileSelectionCount'),
              style: const TextStyle(fontSize: 12, fontWeight: FontWeight.bold),
            ),
            IconButton(
              key: const ValueKey('fileOpSelectAll'),
              tooltip: '모두 선택 (Ctrl+A)',
              iconSize: 18,
              icon: const Icon(Icons.select_all),
              onPressed: _selectAll,
            ),
            IconButton(
              key: const ValueKey('fileOpClear'),
              tooltip: '선택 풀기 (Esc)',
              iconSize: 18,
              icon: const Icon(Icons.deselect),
              onPressed: widget.selection.paths.isEmpty ? null : _clear,
            ),
            _button('fileOpClipboard', Icons.copy, '복사',
                any ? () => _run('clipboard') : null),
            _button('fileOpCopy', Icons.file_copy_outlined, '폴더로 복사',
                any ? () => _run('copy') : null),
            _button('fileOpMove', Icons.drive_file_move_outline, '폴더 이동',
                any ? () => _run('move') : null),
            _button('fileOpRename', Icons.edit_outlined, '이름 변경',
                !_busy && count == 1 ? () => _run('rename') : null),
            _button('fileOpDelete', Icons.delete_outline, '삭제',
                any ? () => _run('delete') : null),
            if (_progress != null) ...[
              const SizedBox(width: 12),
              const SizedBox(
                width: 14,
                height: 14,
                child: CircularProgressIndicator(strokeWidth: 2),
              ),
              const SizedBox(width: 6),
              Text(_progress!, style: const TextStyle(fontSize: 12)),
            ],
          ],
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return Column(
      children: [
        _buildBar(context),
        Expanded(
          child: Focus(
            focusNode: _focus,
            autofocus: true,
            onKeyEvent: _onKey,
            child: Listener(
              onPointerDown: (_) => _focus.requestFocus(),
              child: widget.child,
            ),
          ),
        ),
      ],
    );
  }
}

/// 확장자는 고정하고 이름만 받는다. 이름 검사는 백엔드가 한다.
class _RenameDialog extends StatefulWidget {
  const _RenameDialog({required this.fileName});

  final String fileName;

  @override
  State<_RenameDialog> createState() => _RenameDialogState();
}

class _RenameDialogState extends State<_RenameDialog> {
  late final int _dot = widget.fileName.lastIndexOf('.');
  late final String _stem = _dot > 0
      ? widget.fileName.substring(0, _dot)
      : widget.fileName;
  late final String _extension = _dot > 0 ? widget.fileName.substring(_dot) : '';
  late final TextEditingController _controller = TextEditingController(
    text: _stem,
  )..selection = TextSelection(baseOffset: 0, extentOffset: _stem.length);

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  void _submit() => Navigator.pop(context, _controller.text);

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      key: const ValueKey('fileOpRenameDialog'),
      title: const Text('이름 변경'),
      content: SizedBox(
        width: 420,
        child: TextField(
          key: const ValueKey('fileOpRenameField'),
          controller: _controller,
          autofocus: true,
          decoration: InputDecoration(suffixText: _extension),
          onSubmitted: (_) => _submit(),
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('취소'),
        ),
        FilledButton(
          key: const ValueKey('fileOpRenameConfirm'),
          onPressed: _submit,
          child: const Text('바꾸기'),
        ),
      ],
    );
  }
}
