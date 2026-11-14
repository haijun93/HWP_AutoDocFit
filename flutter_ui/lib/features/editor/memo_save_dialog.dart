/// '다른 이름으로 저장' 대화상자(메모장 벤치마킹): 인코딩(UTF-8·UTF-8 BOM·UTF-16 LE·CP949)과 줄바꿈(CRLF·LF)을 고른다.
///
/// CP949를 고르면 저장 전에 `edit_unencodable`로 점검해, 바꿀 수 없는 글자가 있으면 몰래 `?`로 바꾸지 않고
/// 글자 목록(종류별 개수)과 처음 나오는 자리(줄·열, '이동')를 보여 준다. 사용자는 'UTF-8로 저장' 또는 '돌아가기'를 고른다.
library;

import 'package:flutter/material.dart';

import 'editor_colors.dart';
import 'editor_files.dart';

/// 대화상자 결과: 저장할 형식, 또는 변환 불가 글자 자리로 이동.
class MemoSaveChoice {
  const MemoSaveChoice.save(this.encoding, this.lineEnding) : goTo = null;
  const MemoSaveChoice.goTo(UnencodablePlace this.goTo) : encoding = '', lineEnding = '';
  final String encoding;
  final String lineEnding;
  final UnencodablePlace? goTo;
}

class MemoSaveDialog extends StatefulWidget {
  const MemoSaveDialog({super.key, required this.encoding, required this.lineEnding, required this.check, this.fileName});

  final String encoding;
  final String lineEnding;
  final String? fileName;

  /// 고른 인코딩으로 저장할 수 있는지 점검(Rust `edit_unencodable`).
  final Future<UnencodableReport> Function(String encoding) check;

  @override
  State<MemoSaveDialog> createState() => _MemoSaveDialogState();
}

class _MemoSaveDialogState extends State<MemoSaveDialog> {
  late String _encoding = kMemoEncodings.contains(widget.encoding) ? widget.encoding : 'UTF-8';
  late String _lineEnding = widget.lineEnding == 'LF' ? 'LF' : 'CRLF';
  UnencodableReport? _report;
  bool _checking = false;
  String? _error;

  Future<void> _save() async {
    if (_encoding != 'CP949') {
      Navigator.pop(context, MemoSaveChoice.save(_encoding, _lineEnding));
      return;
    }
    setState(() {
      _checking = true;
      _error = null;
    });
    try {
      final report = await widget.check(_encoding);
      if (!mounted) return;
      if (report.okToSave) {
        Navigator.pop(context, MemoSaveChoice.save(_encoding, _lineEnding));
        return;
      }
      setState(() => _report = report);
    } catch (e) {
      if (mounted) setState(() => _error = '저장할 수 있는지 확인하지 못했습니다: $e');
    } finally {
      if (mounted) setState(() => _checking = false);
    }
  }

  Widget _chips(String label, List<String> values, String selected, String keyPrefix, ValueChanged<String> onSelected) => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      Text(label, style: Theme.of(context).textTheme.labelLarge),
      const SizedBox(height: 6),
      Wrap(
        spacing: 6,
        runSpacing: 6,
        children: [
          for (final v in values)
            ChoiceChip(
              key: ValueKey('$keyPrefix-$v'),
              label: Text(keyPrefix == 'memo-save-eol' ? (v == 'LF' ? 'Unix (LF)' : 'Windows (CRLF)') : v),
              selected: v == selected,
              onSelected: _checking ? null : (_) => onSelected(v),
            ),
        ],
      ),
    ],
  );

  Widget _reportView(UnencodableReport r) {
    final scheme = Theme.of(context).colorScheme;
    return Column(
      key: const ValueKey('memo-unencodable'),
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: [
        Text(
          '${r.encoding}로 저장할 수 없는 글자가 ${r.total}개(서로 다른 ${r.distinctCount}종) 있습니다. '
          '이 글자들을 ?로 바꾸지 않습니다. UTF-8로 저장하거나, 돌아가서 글자를 고치세요.',
          key: const ValueKey('memo-unencodable-summary'),
          style: TextStyle(color: scheme.error),
        ),
        const SizedBox(height: 8),
        Wrap(
          spacing: 6,
          runSpacing: 6,
          children: [
            for (final c in r.distinct.take(30))
              Chip(
                visualDensity: VisualDensity.compact,
                label: Text('${c.char}  ${c.code} × ${c.count}', key: ValueKey('memo-unencodable-char-${c.code}')),
              ),
          ],
        ),
        const SizedBox(height: 8),
        Text('처음 나오는 자리', style: Theme.of(context).textTheme.labelLarge),
        ConstrainedBox(
          constraints: const BoxConstraints(maxHeight: 180),
          child: ListView(
            shrinkWrap: true,
            children: [
              for (final (i, p) in r.first.indexed)
                ListTile(
                  dense: true,
                  visualDensity: VisualDensity.compact,
                  title: Text('${p.line}줄 ${p.column}열  ${p.char} (${p.code})'),
                  trailing: TextButton(
                    key: ValueKey('memo-unencodable-goto-$i'),
                    onPressed: () => Navigator.pop(context, MemoSaveChoice.goTo(p)),
                    child: const Text('이동'),
                  ),
                ),
            ],
          ),
        ),
      ],
    );
  }

  @override
  Widget build(BuildContext context) {
    final report = _report;
    return AlertDialog(
      title: Text(widget.fileName == null ? '다른 이름으로 저장' : '다른 이름으로 저장 — ${widget.fileName}'),
      content: SizedBox(
        width: 460,
        child: report != null
            ? _reportView(report)
            : Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  _chips('인코딩', kMemoEncodings, _encoding, 'memo-save-enc', (v) => setState(() => _encoding = v)),
                  const SizedBox(height: 14),
                  _chips('줄바꿈', kMemoLineEndings, _lineEnding, 'memo-save-eol', (v) => setState(() => _lineEnding = v)),
                  const SizedBox(height: 10),
                  Text(
                    _encoding == 'CP949' ? '저장 전에 CP949로 바꿀 수 없는 글자(이모지 등)가 있는지 확인합니다.' : '파일 위치는 다음 창에서 고릅니다.',
                    style: TextStyle(fontSize: 12, color: Theme.of(context).colorScheme.onSurfaceVariant),
                  ),
                  if (_error != null) Text(_error!, key: const ValueKey('memo-save-error'), style: TextStyle(color: EditorColors.of(context).error)),
                ],
              ),
      ),
      actions: report != null
          ? [
              TextButton(key: const ValueKey('memo-unencodable-back'), onPressed: () => setState(() => _report = null), child: const Text('돌아가기')),
              FilledButton(
                key: const ValueKey('memo-save-as-utf8'),
                onPressed: () => Navigator.pop(context, MemoSaveChoice.save('UTF-8', _lineEnding)),
                child: const Text('UTF-8로 저장'),
              ),
            ]
          : [
              TextButton(onPressed: () => Navigator.pop(context), child: const Text('취소')),
              FilledButton(
                key: const ValueKey('memo-save-file-ok'),
                onPressed: _checking ? null : _save,
                child: Text(_checking ? '확인 중…' : '저장…'),
              ),
            ],
    );
  }
}
