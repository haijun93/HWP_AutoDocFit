/// 메모 편집 화면(전체 화면 경로): [MemoEditor] + 저장 단추. 업무자료함 '+메모 추가'와 자료함 '+메모'·글 자료 고치기가 같이 쓴다.
///
/// 저장할 곳은 모른다: [MemoEditorPage.onSave]가 저장하고 돌려준 값으로 화면을 닫는다(`Navigator.pop`).
/// 저장이 실패하면 화면을 닫지 않고 아래 안내 줄에 오류를 보여 준다.
library;

import 'package:flutter/material.dart';

import 'editor_colors.dart';
import 'klib_editor_core.dart';
import 'memo_editor.dart';

String _errorText(Object error) => error.toString().replaceFirst(RegExp(r'^(Bad state|StateError): '), '');

class MemoEditorPage<T> extends StatefulWidget {
  const MemoEditorPage({
    super.key,
    required this.initial,
    required this.onSave,
    required this.saveLabel,
    required this.hint,
    this.controller,
    this.onAutoSave,
    this.onDelete,
    this.editorBackend,
    this.pageTitle = '메모',
  });

  final MemoDocument initial;

  /// 저장 단추. 돌려준 값으로 화면을 닫는다. 예외를 던지면 화면에 남아 오류를 보여 준다.
  final Future<T> Function(MemoDocument document) onSave;
  final String saveLabel;

  /// 저장 단추 옆 안내 문구(오류가 나면 오류로 바뀐다).
  final String hint;
  final MemoEditorController? controller;

  /// 자동 저장(초안 보관 등). 없으면 표시만 'Saved'.
  final Future<void> Function(MemoDocument document)? onAutoSave;

  /// 휴지통(삭제). 없으면 휴지통 단추를 숨긴다.
  final VoidCallback? onDelete;

  /// Rust 편집 코어(klib edit_*) 통로. 없으면 편집기가 Dart 임시 코어를 쓴다.
  final EditorBackend? editorBackend;
  final String pageTitle;

  @override
  State<MemoEditorPage<T>> createState() => _MemoEditorPageState<T>();
}

class _MemoEditorPageState<T> extends State<MemoEditorPage<T>> {
  late final MemoEditorController _controller = widget.controller ?? MemoEditorController();
  bool _saving = false;
  String? _error;

  Future<void> _save() async {
    if (_controller.isLargeFile) {
      // 큰 파일은 글 전체를 화면으로 가져오지 않으므로 메모(업무자료함·자료함)로 넣지 않는다.
      setState(() => _error = "큰 파일 보기 중입니다. 도구줄의 '다른 이름으로 저장'으로 파일에 저장하세요.");
      return;
    }
    await _controller.flush();
    final doc = _controller.document;
    if (doc.isBlank) {
      setState(() => _error = '메모에 내용이 없습니다.');
      return;
    }
    setState(() {
      _saving = true;
      _error = null;
    });
    try {
      final result = await widget.onSave(doc);
      if (mounted) Navigator.of(context).pop(result);
    } catch (error) {
      if (mounted) setState(() => _error = _errorText(error));
    } finally {
      if (mounted) setState(() => _saving = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: Text(widget.pageTitle), toolbarHeight: 44),
      body: Padding(
        padding: const EdgeInsets.fromLTRB(16, 8, 16, 12),
        child: MemoEditor(
          initial: widget.initial,
          controller: _controller,
          onAutoSave: widget.onAutoSave,
          onDelete: widget.onDelete,
          createCore: switch (widget.editorBackend) {
            final backend? => (text) => KlibEditorCore(backend, text: text),
            null => null,
          },
          // 텍스트 파일 열기·다른 이름으로 저장(인코딩·줄바꿈 선택)도 같은 klib 통로를 쓴다.
          fileBackend: widget.editorBackend,
          footer: Padding(
            padding: const EdgeInsets.only(top: 8),
            child: Row(
              children: [
                Expanded(
                  child: Text(
                    _error ?? widget.hint,
                    key: const ValueKey('memo-hint'),
                    style: TextStyle(fontSize: 12, color: _error == null ? EditorColors.of(context).secondaryText : EditorColors.of(context).error),
                  ),
                ),
                FilledButton.icon(
                  key: const ValueKey('memo-save'),
                  onPressed: _saving ? null : _save,
                  icon: const Icon(Icons.save_outlined, size: 18),
                  label: Text(widget.saveLabel),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
