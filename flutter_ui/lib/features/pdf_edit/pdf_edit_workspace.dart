import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';

import '../../core/api/abbreviation_api.dart' show ApiCall;

class PdfEditWorkspace extends StatefulWidget {
  const PdfEditWorkspace({super.key, required this.api});
  final ApiCall api;

  @override
  State<PdfEditWorkspace> createState() => _PdfEditWorkspaceState();
}

class _PdfEditWorkspaceState extends State<PdfEditWorkspace> {
  static const _pdfType = XTypeGroup(label: 'PDF', extensions: ['pdf']);
  final _pages = TextEditingController(text: '1');
  String? _source;
  final List<String> _mergeSources = [];
  String? _output;
  String _action = 'delete';
  int _degrees = 90;
  String _message = '';
  bool _busy = false;

  @override
  void dispose() {
    _pages.dispose();
    super.dispose();
  }

  Future<void> _pickSource() async {
    final file = await openFile(acceptedTypeGroups: [_pdfType]);
    if (file == null || !mounted) return;
    setState(() {
      _source = file.path;
      _output = file.path.replaceFirst(RegExp(r'\.pdf$', caseSensitive: false), '_편집본.pdf');
      _message = '';
    });
  }

  Future<void> _pickMergeSources() async {
    final files = await openFiles(acceptedTypeGroups: [_pdfType]);
    if (files.length < 2 || !mounted) return;
    setState(() {
      _mergeSources
        ..clear()
        ..addAll(files.map((file) => file.path));
      _output = files.first.path.replaceFirst(RegExp(r'\.pdf$', caseSensitive: false), '_합본.pdf');
      _message = '';
    });
  }

  Future<void> _pickOutput() async {
    if (_action != 'merge' && _source == null) return;
    final location = await getSaveLocation(suggestedName: _output?.split(RegExp(r'[/\\]')).last ?? '편집본.pdf', acceptedTypeGroups: [_pdfType]);
    if (location != null && mounted) setState(() => _output = location.path);
  }

  Future<void> _run() async {
    if ((_action == 'merge' ? _mergeSources.length < 2 : _source == null) || _output == null) {
      setState(() => _message = '원본 PDF와 결과 경로를 먼저 선택해 주세요.');
      return;
    }
    final pages = _pages.text.split(',').map((v) => int.tryParse(v.trim())).toList();
    if (_action != 'merge' && (pages.isEmpty || pages.any((v) => v == null || v < 1))) {
      setState(() => _message = '페이지 번호를 쉼표로 구분해 입력해 주세요.');
      return;
    }
    setState(() { _busy = true; _message = ''; });
    try {
      final response = await widget.api('pdf_edit', [{
        'action': _action,
        'source': _source,
        'output': _output,
        'pages': pages.cast<int>(),
        if (_action == 'merge') 'sources': _mergeSources,
        if (_action == 'rotate') 'degrees': _degrees,
      }]);
      if (!mounted) return;
      final envelope = response is Map ? response : const {};
      final error = envelope['error'];
      final data = envelope['data'];
      setState(() => _message = envelope['ok'] == true
          ? '완료: ${data is Map ? data['output'] : _output}'
          : (error is Map ? '${error['message'] ?? 'PDF 편집에 실패했습니다.'}' : 'PDF 편집에 실패했습니다.'));
    } catch (error) {
      if (mounted) setState(() => _message = 'PDF 편집 요청에 실패했습니다: $error');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.all(24),
    child: ListView(children: [
      Text('PDF 편집', style: Theme.of(context).textTheme.headlineSmall),
      const SizedBox(height: 20),
      Wrap(spacing: 12, runSpacing: 12, crossAxisAlignment: WrapCrossAlignment.center, children: [
        FilledButton.icon(key: const Key('pdfEditPickSource'), onPressed: _busy ? null : _pickSource, icon: const Icon(Icons.picture_as_pdf), label: const Text('PDF 파일 고르기')),
        if (_action == 'merge') FilledButton.icon(key: const Key('pdfEditPickMergeSources'), onPressed: _busy ? null : _pickMergeSources, icon: const Icon(Icons.library_add), label: const Text('합칠 PDF 파일 고르기')),
        if (_source != null) SizedBox(width: 360, child: Text(_source!, key: const Key('pdfEditSourcePath'), overflow: TextOverflow.ellipsis)),
        if (_mergeSources.isNotEmpty) Text('선택한 파일 ${_mergeSources.length}개'),
      ]),
      const SizedBox(height: 16),
      DropdownButtonFormField<String>(
        key: const Key('pdfEditAction'),
        value: _action,
        decoration: const InputDecoration(labelText: '작업 선택'),
        items: const [
          DropdownMenuItem(value: 'delete', child: Text('페이지 삭제')),
          DropdownMenuItem(value: 'reorder', child: Text('페이지 순서 변경')),
          DropdownMenuItem(value: 'rotate', child: Text('페이지 회전')),
          DropdownMenuItem(value: 'split', child: Text('페이지 분할')),
          DropdownMenuItem(value: 'merge', child: Text('PDF 합치기')),
        ],
        onChanged: _busy ? null : (value) => setState(() => _action = value ?? 'delete'),
      ),
      const SizedBox(height: 12),
      if (_action != 'merge') TextField(key: const Key('pdfEditPages'), controller: _pages, decoration: InputDecoration(labelText: _action == 'split' ? '분할할 페이지' : '페이지 번호', helperText: _action == 'split' ? '쉼표로 입력한 각 페이지를 별도 PDF로 저장합니다.' : '쉼표로 구분해 입력하세요. 순서 변경은 원하는 페이지 순서로 입력합니다.')),
      if (_action == 'rotate') ...[
        const SizedBox(height: 12),
        DropdownButtonFormField<int>(value: _degrees, decoration: const InputDecoration(labelText: '회전 각도'), items: const [90, 180, 270].map((n) => DropdownMenuItem(value: n, child: Text('$n°'))).toList(), onChanged: _busy ? null : (value) => setState(() => _degrees = value ?? 90)),
      ],
      const SizedBox(height: 16),
      Wrap(spacing: 12, runSpacing: 12, crossAxisAlignment: WrapCrossAlignment.center, children: [
        OutlinedButton(key: const Key('pdfEditPickOutput'), onPressed: _busy || (_action != 'merge' && _source == null) ? null : _pickOutput, child: const Text('결과 경로 선택')),
        if (_output != null) SizedBox(width: 420, child: Text(_output!, key: const Key('pdfEditOutputPath'), overflow: TextOverflow.ellipsis)),
      ]),
      const SizedBox(height: 20),
      Align(alignment: Alignment.centerLeft, child: FilledButton(key: const Key('pdfEditRun'), onPressed: _busy ? null : _run, child: Text(_busy ? '처리 중…' : '편집 실행'))),
      if (_message.isNotEmpty) ...[const SizedBox(height: 16), SelectableText(_message, key: const Key('pdfEditMessage'))],
      const SizedBox(height: 12),
      const Text('원본 파일은 유지되며 결과는 새 파일로 저장됩니다. 암호·권한 제한·손상된 PDF는 편집할 수 없습니다.'),
    ]),
  );
}
