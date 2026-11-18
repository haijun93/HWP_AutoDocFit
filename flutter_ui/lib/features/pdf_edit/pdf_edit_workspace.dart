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
  final _pages = TextEditingController(text: '1');
  final List<String> _mergeSources = [];
  String? _source;
  String? _output;
  String _action = 'delete';
  int _degrees = 90;
  String _message = '';
  bool _busy = false;
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

  String _outputFor(String path, [String suffix = '_편집본']) =>
      path.replaceFirst(RegExp(r'\.pdf$', caseSensitive: false), '$suffix.pdf');

  Future<void> _pickSource() async {
    final file = await openFile(acceptedTypeGroups: [_pdfType]);
    if (file != null) _addFile(file.path);
  }

  void _addFile(String path) {
    if (!path.toLowerCase().endsWith('.pdf')) return;
    setState(() {
      _source = path;
      if (_action == 'merge' && !_mergeSources.contains(path)) _mergeSources.add(path);
      _output = _outputFor(path, _action == 'merge' ? '_합본' : '_편집본');
      _message = '';
      _dragActive = false;
    });
  }

  Future<void> _pickMergeSources() async {
    final files = await openFiles(acceptedTypeGroups: [_pdfType]);
    if (files.length < 2 || !mounted) return;
    setState(() {
      _mergeSources..clear()..addAll(files.map((file) => file.path));
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

  bool get _canRun => !_busy && _output != null &&
      (_action == 'merge' ? _mergeSources.length >= 2 : _source != null);

  Future<void> _run() async {
    if (!_canRun) return;
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
  Widget build(BuildContext context) {
    final c = context.colors;
    return LayoutBuilder(builder: (context, constraints) {
      final compact = constraints.maxWidth < 900;
      return Padding(
        padding: EdgeInsets.all(compact ? 16 : 24),
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Row(children: [
            Expanded(child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Text('PDF 편집', style: Theme.of(context).textTheme.headlineSmall?.copyWith(color: c.ink, fontWeight: FontWeight.w700)),
              const SizedBox(height: 4),
              Text('페이지를 정리하고 새 PDF로 저장합니다.', style: TextStyle(color: c.muted)),
            ])),
            OutlinedButton.icon(key: const Key('pdfEditPickSource'), onPressed: _busy ? null : _pickSource, icon: const Icon(Icons.add), label: const Text('PDF 추가')),
          ]),
          const SizedBox(height: 20),
          Wrap(spacing: 8, runSpacing: 8, children: [for (final tool in _tools)
            _ToolChip(label: tool.$2, icon: tool.$3, selected: _action == tool.$1,
              onTap: _busy ? null : () => setState(() {
                _action = tool.$1;
                if (tool.$1 == 'merge') {
                  _output = _source == null ? null : _outputFor(_source!, '_합본');
                } else if (_source != null) {
                  _output = _outputFor(_source!);
                }
              }))]),
          const SizedBox(height: 16),
          Expanded(child: Row(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
            Expanded(child: _source == null ? _dropZone(c) : _editor(c)),
            if (!compact) ...[const SizedBox(width: 16), SizedBox(width: 284, child: _sidePanel(c))],
          ])),
          if (compact && _source != null) ...[const SizedBox(height: 12), _sidePanel(c)],
        ]),
      );
    });
  }

  Widget _dropZone(AppColors c) => DragTarget<String>(
    onWillAcceptWithDetails: (_) { setState(() => _dragActive = true); return true; },
    onLeave: (_) => setState(() => _dragActive = false),
    onAcceptWithDetails: (details) => _addFile(details.data),
    builder: (context, candidates, rejected) => Center(child: InkWell(
      key: const Key('pdfEditDropZone'), onTap: _pickSource, borderRadius: BorderRadius.circular(8),
      child: AnimatedContainer(duration: const Duration(milliseconds: 140), width: double.infinity,
        constraints: const BoxConstraints(maxWidth: 720, maxHeight: 340),
        decoration: BoxDecoration(color: _dragActive ? c.accentSoft : c.surface,
          borderRadius: BorderRadius.circular(8), border: Border.all(color: _dragActive ? c.accent : c.lineStrong, width: _dragActive ? 2 : 1)),
        child: Column(mainAxisAlignment: MainAxisAlignment.center, children: [
          Icon(Icons.upload_file_outlined, size: 38, color: c.accent),
          const SizedBox(height: 14),
          Text('PDF를 여기로 끌어다 놓거나 클릭해서 선택', textAlign: TextAlign.center,
            style: TextStyle(color: c.textStrong, fontSize: 17, fontWeight: FontWeight.w600)),
          const SizedBox(height: 6),
          Text('PDF 파일 한 개를 추가할 수 있습니다.', style: TextStyle(color: c.muted)),
          const SizedBox(height: 18),
          OutlinedButton.icon(onPressed: _pickSource, icon: const Icon(Icons.folder_open_outlined), label: const Text('파일 선택')),
        ]),
      ),
    )),
  );

  Widget _editor(AppColors c) => Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
    _fileCard(c),
    const SizedBox(height: 14),
    Expanded(child: DecoratedBox(decoration: BoxDecoration(color: c.surface, border: Border.all(color: c.line), borderRadius: BorderRadius.circular(8)),
      child: _action == 'merge' ? _mergeEditor(c) : _pageGrid(c))),
  ]);

  Widget _fileCard(AppColors c) => Container(key: const Key('pdfEditFileCard'), padding: const EdgeInsets.all(14),
    decoration: BoxDecoration(color: c.surface, border: Border.all(color: c.line), borderRadius: BorderRadius.circular(8)),
    child: Row(children: [Icon(Icons.picture_as_pdf_outlined, color: c.accent), const SizedBox(width: 12),
      Expanded(child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        Text(_source!.split(RegExp(r'[/\\]')).last, overflow: TextOverflow.ellipsis, style: TextStyle(color: c.textStrong, fontWeight: FontWeight.w600)),
        Text('쪽 수는 파일을 실행할 때 확인됩니다.', style: TextStyle(color: c.muted, fontSize: 12)),
      ])),
      IconButton(tooltip: '파일 제거', onPressed: _busy ? null : () => setState(() { _source = null; _output = null; _mergeSources.clear(); }), icon: const Icon(Icons.close), color: c.muted),
    ]));

  Widget _pageGrid(AppColors c) => Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
    Padding(padding: const EdgeInsets.fromLTRB(16, 14, 16, 8), child: Row(children: [
      Text('쪽 미리보기', style: TextStyle(color: c.textStrong, fontWeight: FontWeight.w600)), const SizedBox(width: 8),
      Text('페이지 번호를 입력해 작업할 쪽을 지정하세요.', style: TextStyle(color: c.muted, fontSize: 12)),
    ])),
    Expanded(child: GridView.builder(key: const Key('pdfEditThumbnailGrid'), padding: const EdgeInsets.all(16),
      gridDelegate: const SliverGridDelegateWithMaxCrossAxisExtent(maxCrossAxisExtent: 150, mainAxisExtent: 178, crossAxisSpacing: 12, mainAxisSpacing: 12),
      itemCount: 12, itemBuilder: (context, i) => _PageCard(number: i + 1, c: c))),
    Padding(padding: const EdgeInsets.fromLTRB(16, 0, 16, 14), child: Row(children: [
      Expanded(child: TextField(key: const Key('pdfEditPages'), controller: _pages, decoration: InputDecoration(isDense: true,
        labelText: _action == 'split' ? '분할할 페이지' : '페이지 번호', helperText: _action == 'split' ? '쉼표로 입력한 쪽을 별도 PDF로 저장' : '쉼표로 구분 · 순서 변경은 원하는 순서'))),
      if (_action == 'rotate') ...[const SizedBox(width: 12), DropdownButton<int>(value: _degrees, items: const [90,180,270].map((n) => DropdownMenuItem(value: n, child: Text('$n°'))).toList(), onChanged: _busy ? null : (v) => setState(() => _degrees = v ?? 90))],
    ])),
  ]);

  Widget _mergeEditor(AppColors c) => Padding(padding: const EdgeInsets.all(16), child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
    Text('합칠 PDF', style: TextStyle(color: c.textStrong, fontWeight: FontWeight.w600)),
    const SizedBox(height: 8),
    Text('두 개 이상의 PDF를 선택하고 합칠 순서를 확인하세요.', style: TextStyle(color: c.muted)),
    const SizedBox(height: 12),
    OutlinedButton.icon(key: const Key('pdfEditPickMergeSources'), onPressed: _busy ? null : _pickMergeSources, icon: const Icon(Icons.library_add_outlined), label: const Text('PDF 여러 개 선택')),
    const SizedBox(height: 8),
    Expanded(child: ListView(children: [for (var i=0; i<_mergeSources.length; i++) ListTile(dense: true, leading: Text('${i+1}'), title: Text(_mergeSources[i].split(RegExp(r'[/\\]')).last, overflow: TextOverflow.ellipsis))])),
  ]));

  Widget _sidePanel(AppColors c) => Container(padding: const EdgeInsets.all(16), decoration: BoxDecoration(color: c.surface, border: Border.all(color: c.line), borderRadius: BorderRadius.circular(8)),
    child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
      Text('작업 설정', style: TextStyle(color: c.textStrong, fontWeight: FontWeight.w600)), const SizedBox(height: 12),
      Text(_action == 'merge' ? '입력 파일' : '대상 페이지', style: TextStyle(color: c.muted, fontSize: 12)),
      Text(_action == 'merge' ? '${_mergeSources.length}개 선택' : _pages.text.isEmpty ? '선택 없음' : _pages.text, style: TextStyle(color: c.text)),
      const SizedBox(height: 14),
      OutlinedButton(key: const Key('pdfEditPickOutput'), onPressed: _busy || (_action != 'merge' && _source == null) ? null : _pickOutput, child: const Text('저장 위치 변경')),
      if (_output != null) Padding(padding: const EdgeInsets.only(top: 8), child: Text(_output!, key: const Key('pdfEditOutputPath'), maxLines: 2, overflow: TextOverflow.ellipsis, style: TextStyle(color: c.muted, fontSize: 12))),
      const SizedBox(height: 12),
      if (_busy) const LinearProgressIndicator(minHeight: 2),
      FilledButton(key: const Key('pdfEditRun'), onPressed: _canRun ? _run : null, child: Text(_busy ? '처리 중…' : '편집 실행')),
      if (_message.isNotEmpty) ...[const SizedBox(height: 10), SelectableText(_message, key: const Key('pdfEditMessage'), style: TextStyle(color: _message.startsWith('완료') ? c.success : c.danger))],
      const SizedBox(height: 8),
      Text('원본은 유지되고 결과는 새 파일로 저장됩니다.', style: TextStyle(color: c.muted, fontSize: 11)),
    ]));
}

class _ToolChip extends StatelessWidget {
  const _ToolChip({required this.label, required this.icon, required this.selected, required this.onTap});
  final String label;
  final IconData icon;
  final bool selected;
  final VoidCallback? onTap;
  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return Material(color: selected ? c.accentSoft : c.surface, borderRadius: BorderRadius.circular(8),
      child: InkWell(key: Key('pdfEditTool_${label.replaceAll(' ', '')}'), onTap: onTap, borderRadius: BorderRadius.circular(8),
        child: Container(padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 9), decoration: BoxDecoration(borderRadius: BorderRadius.circular(8), border: Border.all(color: selected ? c.accent : c.line)),
          child: Row(mainAxisSize: MainAxisSize.min, children: [Icon(icon, size: 17, color: selected ? c.accent : c.textSecondary), const SizedBox(width: 7), Text(label, style: TextStyle(color: selected ? c.accent : c.textSecondary, fontWeight: selected ? FontWeight.w600 : FontWeight.w500))]))));
  }
}

class _PageCard extends StatelessWidget {
  const _PageCard({required this.number, required this.c});
  final int number;
  final AppColors c;
  @override
  Widget build(BuildContext context) => Container(decoration: BoxDecoration(color: c.surfaceMuted, border: Border.all(color: c.line), borderRadius: BorderRadius.circular(8)),
    child: Stack(children: [Center(child: Column(mainAxisSize: MainAxisSize.min, children: [Icon(Icons.description_outlined, size: 54, color: c.lineStrong), const SizedBox(height: 8), Text('쪽 $number', style: TextStyle(color: c.muted, fontSize: 12))])),
      Positioned(left: 8, top: 8, child: Container(padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 3), decoration: BoxDecoration(color: c.surface, borderRadius: BorderRadius.circular(8), border: Border.all(color: c.line)), child: Text('$number', style: TextStyle(color: c.textSecondary, fontSize: 11)))),
    ]));
}
