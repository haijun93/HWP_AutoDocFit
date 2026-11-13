/// 메모 편집기 화면 조각(재사용 모듈). 제목 + 서식 도구줄 + 찾기·바꾸기 막대 + 본문 + 상태 표시줄 + 자동 저장 표시('Saved') + 삭제.
///
/// 입력은 Flutter `TextField`가 그대로 받고(한글 IME 조합 포함), 서식·실행취소·찾기·평문 변환은 [EditorCore]에 맡긴다.
/// 코어는 klib의 Rust 편집 코어([KlibEditorCore])가 기본이고, 없으면 [DartEditorCore]로 폴백한다.
/// 저장 위치는 모른다: 자동 저장은 [MemoEditor.onAutoSave], 확정 저장은 쓰는 쪽(예: 업무자료함)이 [MemoEditorController]로 꺼내 간다.
///
/// 메모장처럼: Ctrl+F 찾기, Ctrl+H 바꾸기, F3/Shift+F3 다음/이전, Ctrl+휠·Ctrl+= / Ctrl+- / Ctrl+0 확대·축소,
/// 아래 상태 표시줄에 줄·열, 글자 수, 확대 비율, 줄바꿈, 인코딩.
///
/// 3단계(EDCORE3, [MemoEditor.fileBackend]가 있을 때): Ctrl+O 텍스트 파일 열기(인코딩·줄바꿈 자동 감지),
/// Ctrl+Shift+S 다른 이름으로 저장(인코딩·줄바꿈 선택, CP949 변환 불가 글자 확인). 열린 파일이
/// [MemoEditor.largeFileBytes]·[MemoEditor.largeFileLines]를 넘으면 글 전체를 입력칸에 넣지 않고 큰 글 보기([LargeTextView])로 연다.
library;

import 'dart:async';

import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'editor_colors.dart';
import 'editor_core.dart';
import 'editor_files.dart';
import 'klib_editor_core.dart' show EditorBackend, KlibEditorCore, fetchEditorText;
import 'large_text_session.dart';
import 'large_text_view.dart';
import 'markdown_format.dart';
import 'memo_save_dialog.dart';
import 'text_offsets.dart' show lineColumnAt;

/// 제목과 Markdown 본문.
class MemoDocument {
  const MemoDocument({required this.title, required this.markdown});
  final String title;
  final String markdown;

  bool get isBlank => markdown.trim().isEmpty;
}

enum MemoSaveState { saved, pending, saving, failed }

/// 편집기 밖에서 현재 내용을 읽을 때 쓴다.
class MemoEditorController {
  _MemoEditorState? _state;

  MemoDocument get document => _state?._document ?? const MemoDocument(title: '', markdown: '');
  List<PlainParagraph> get plainParagraphs => _state?._core.plainParagraphs() ?? const [];

  /// 기다리는 자동 저장이 있으면 바로 저장한다.
  Future<void> flush() async => _state?._flush();

  /// 큰 파일 보기 중인지(이때 [document]의 본문은 열기 전 메모 그대로다 — 파일은 '다른 이름으로 저장'으로 저장).
  bool get isLargeFile => _state?._large != null;
}

/// 이보다 큰 파일은 큰 글 보기로 연다.
const int kMemoLargeFileBytes = 128 * 1024;
const int kMemoLargeFileLines = 3000;

class MemoEditor extends StatefulWidget {
  const MemoEditor({
    super.key,
    this.initial = const MemoDocument(title: '새 메모', markdown: ''),
    this.onAutoSave,
    this.onDelete,
    this.controller,
    this.createCore,
    this.autoSaveDelay = const Duration(milliseconds: 800),
    this.footer,
    this.fileBackend,
    this.largeFileBytes = kMemoLargeFileBytes,
    this.largeFileLines = kMemoLargeFileLines,
  });

  final MemoDocument initial;

  /// 내용이 바뀌고 [autoSaveDelay] 동안 멈추면 부른다. 없으면 표시만 'Saved'.
  final Future<void> Function(MemoDocument document)? onAutoSave;

  /// 휴지통(삭제)을 눌러 확인하면 부른다. 없으면 휴지통 단추를 숨긴다.
  final VoidCallback? onDelete;
  final MemoEditorController? controller;

  /// 편집 코어를 만든다(예: klib Rust 코어). 없으면 [DartEditorCore].
  final EditorCore Function(String markdown)? createCore;
  final Duration autoSaveDelay;

  /// 본문 아래에 둘 위젯(예: '출처로 저장' 단추).
  final Widget? footer;

  /// 텍스트 파일 열기·다른 이름으로 저장 통로(호스트 파일 대화상자 + klib edit_*). 없으면 두 단추를 숨긴다.
  final EditorBackend? fileBackend;

  /// 열린 파일이 이 크기(UTF-8 바이트)나 줄 수를 넘으면 큰 글 보기로 연다.
  final int largeFileBytes;
  final int largeFileLines;

  @override
  State<MemoEditor> createState() => _MemoEditorState();
}

/// 확대 비율 범위(메모장: 10%~500%). 이 편집기는 50%~500%, 10%씩.
const double kMemoZoomMin = 0.5;
const double kMemoZoomMax = 5.0;
const double kMemoBaseFontSize = 14;

String memoLineEndingLabel(String lineEnding) => switch (lineEnding) {
  'LF' => 'Unix (LF)',
  'CR' => 'Macintosh (CR)',
  _ => 'Windows (CRLF)',
};

/// 찾은 구간을 본문이 포커스가 없어도 보이게 칠하는 입력 컨트롤러.
class _MemoTextController extends TextEditingController {
  _MemoTextController({super.text});

  TextRange _highlight = TextRange.empty;

  set highlight(TextRange range) {
    if (range == _highlight) return;
    _highlight = range;
    notifyListeners();
  }

  @override
  TextSpan buildTextSpan({required BuildContext context, TextStyle? style, required bool withComposing}) {
    final h = _highlight;
    final composing = withComposing && value.composing.isValid && !value.composing.isCollapsed;
    if (composing || !h.isValid || h.isCollapsed || h.end > text.length) {
      return super.buildTextSpan(context: context, style: style, withComposing: withComposing);
    }
    return TextSpan(
      style: style,
      children: [
        TextSpan(text: text.substring(0, h.start)),
        TextSpan(text: text.substring(h.start, h.end), style: TextStyle(backgroundColor: EditorColors.of(context).findMark)),
        TextSpan(text: text.substring(h.end)),
      ],
    );
  }
}

/// Ctrl을 누른 동안 본문이 휠로 스크롤하지 않게 해서, 휠 이벤트가 확대·축소로 가게 한다.
class _CtrlZoomScrollPhysics extends ScrollPhysics {
  const _CtrlZoomScrollPhysics({super.parent});

  @override
  _CtrlZoomScrollPhysics applyTo(ScrollPhysics? ancestor) => _CtrlZoomScrollPhysics(parent: buildParent(ancestor));

  @override
  bool shouldAcceptUserOffset(ScrollMetrics position) =>
      !HardwareKeyboard.instance.isControlPressed && super.shouldAcceptUserOffset(position);
}

class _MemoEditorState extends State<MemoEditor> {
  late EditorCore _core = (widget.createCore ?? (t) => DartEditorCore(text: t))(widget.initial.markdown);

  /// 큰 글 보기 중이면 그 세션(본문 입력칸 대신 [LargeTextView]).
  LargeTextSession? _large;
  final _largeKey = GlobalKey<LargeTextViewState>();
  final _largeCaret = ValueNotifier<LargeCaret>((line: 1, column: 1));

  /// 파일 열기·저장 결과 안내(상태 표시줄 왼쪽).
  String? _fileMessage;
  late final _title = TextEditingController(text: widget.initial.title);
  late final _body = _MemoTextController(text: widget.initial.markdown);
  final _bodyFocus = FocusNode();
  final _findQuery = TextEditingController();
  final _replaceWith = TextEditingController();
  final _findFocus = FocusNode();
  Timer? _saveTimer;
  Timer? _groupTimer;
  MemoSaveState _save = MemoSaveState.saved;
  bool _syncing = false;

  /// 코어 명령(파이프 왕복)이 끝나기 전에는 입력을 받지 않는다(Rust 코어와 글이 어긋나지 않게).
  bool _busy = false;
  double _zoom = 1;
  bool _findOpen = false;
  bool _replaceOpen = false;
  bool _caseSensitive = false;
  bool _wholeWord = false;
  String? _findMessage;

  MemoDocument get _document => MemoDocument(title: _title.text.trim(), markdown: _core.markdown);
  FindOptions get _findOptions => FindOptions(caseSensitive: _caseSensitive, wholeWord: _wholeWord);

  @override
  void initState() {
    super.initState();
    widget.controller?._state = this;
    _body.value = _core.value;
    _body.addListener(_onBodyChanged);
    _title.addListener(_scheduleSave);
    _core.changes.addListener(_onCoreChanged);
    _largeCaret.addListener(_onLargeChanged); // 큰 글 보기 커서 → 상태 표시줄
  }

  @override
  void dispose() {
    if (widget.controller?._state == this) widget.controller!._state = null;
    _saveTimer?.cancel();
    _groupTimer?.cancel();
    _core.changes.removeListener(_onCoreChanged);
    _core.dispose();
    _large?.removeListener(_onLargeChanged);
    _large?.dispose();
    _largeCaret.dispose();
    _title.dispose();
    _body.dispose();
    _bodyFocus.dispose();
    _findQuery.dispose();
    _replaceWith.dispose();
    _findFocus.dispose();
    super.dispose();
  }

  void _setBody(TextEditingValue value) {
    _syncing = true;
    _body.value = value;
    _syncing = false;
  }

  void _onBodyChanged() {
    if (_syncing) return;
    if (_busy) {
      // 코어 명령을 기다리는 동안 들어온 입력은 버린다(명령 결과가 곧 덮어씀).
      if (_body.text != _core.value.text) _setBody(_core.value);
      return;
    }
    _body.highlight = TextRange.empty;
    final before = _core.markdown;
    _core.updateFromInput(_body.value);
    // 코어가 값을 고쳤으면(줄바꿈 LF 통일 등) 입력 칸도 맞춘다.
    if (_core.value.text != _body.text) _setBody(_core.value);
    if (_core.markdown != before) {
      // 잠시(0.7초) 멈추면 실행취소 단계를 끊는다(메모장처럼 단어·문장 단위로 되돌림).
      _groupTimer?.cancel();
      _groupTimer = Timer(const Duration(milliseconds: 700), _core.breakUndoGroup);
      _scheduleSave();
    } else {
      setState(() {}); // 커서 줄의 문단 형식·줄·열 표시 갱신
    }
  }

  /// 코어가 스스로 바뀜(Rust 연결·폴백 전환·상태값). 글이 다르면 맞춘다.
  void _onCoreChanged() {
    if (!mounted || _busy) return;
    if (_core.value.text != _body.text) _setBody(_core.value);
    setState(() {});
  }

  void _syncFromCore() {
    final changed = _core.value.text != _body.text;
    if (changed) _body.highlight = TextRange.empty;
    _setBody(_core.value);
    if (changed) {
      _scheduleSave();
    } else if (mounted) {
      setState(() {});
    }
  }

  /// 코어 명령 하나를 실행하고(기다리는 동안 입력 막음) 결과를 입력 칸에 반영한다.
  Future<T?> _runCore<T>(Future<T> Function() op) async {
    if (_busy) return null;
    setState(() => _busy = true);
    try {
      return await op();
    } catch (error) {
      if (mounted) setState(() => _findMessage = '편집 명령이 실패했습니다: $error');
      return null;
    } finally {
      _busy = false;
      if (mounted) _syncFromCore();
    }
  }

  void _scheduleSave() {
    _saveTimer?.cancel();
    setState(() => _save = MemoSaveState.pending);
    _saveTimer = Timer(widget.autoSaveDelay, () => unawaited(_flush()));
  }

  Future<void> _flush() async {
    _saveTimer?.cancel();
    _saveTimer = null;
    if (_save == MemoSaveState.saved) return;
    final save = widget.onAutoSave;
    if (save == null) {
      if (mounted) setState(() => _save = MemoSaveState.saved);
      return;
    }
    if (mounted) setState(() => _save = MemoSaveState.saving);
    try {
      await save(_document);
      if (mounted && _saveTimer == null) setState(() => _save = MemoSaveState.saved);
    } catch (_) {
      if (mounted) setState(() => _save = MemoSaveState.failed);
    }
  }

  Future<void> _command(EditorCommand command) async {
    if (_large != null) return; // 큰 파일 보기에서는 서식 명령을 쓰지 않는다(평문 편집)
    await _runCore(() => _core.apply(command));
    _bodyFocus.requestFocus();
  }

  Future<void> _undo() async {
    if (_large != null) return _largeKey.currentState?.undo();
    await _runCore(_core.undo);
  }

  Future<void> _redo() async {
    if (_large != null) return _largeKey.currentState?.undo(redo: true);
    await _runCore(_core.redo);
  }

  Future<void> _link() async {
    if (_large != null) return;
    final url = await showDialog<String>(context: context, builder: (_) => const _LinkDialog());
    if (url == null || url.trim().isEmpty) return;
    await _runCore(() => _core.insertLink(url));
    _bodyFocus.requestFocus();
  }

  // ---- 파일 열기·저장(3단계) ----

  MemoFileGateway? get _files => switch (widget.fileBackend) {
    final b? => MemoFileGateway(b),
    null => null,
  };

  EditorFileSupport? get _fileTarget {
    final large = _large;
    if (large != null) return large.canSaveToFile ? large : null;
    final core = _core;
    return core is EditorFileSupport && (core as EditorFileSupport).canSaveToFile ? core as EditorFileSupport : null;
  }

  void _onLargeChanged() {
    if (mounted) setState(() {});
  }

  void _replaceCore(EditorCore core) {
    _core.changes.removeListener(_onCoreChanged);
    _core.dispose();
    _core = core;
    core.changes.addListener(_onCoreChanged);
    _body.highlight = TextRange.empty;
    _setBody(core.value);
  }

  void _setLarge(LargeTextSession? session) {
    _large?.removeListener(_onLargeChanged);
    _large?.dispose();
    _large = session;
    session?.addListener(_onLargeChanged);
    _largeCaret.value = (line: 1, column: 1);
  }

  static int _num(Object? v) => v is num ? v.toInt() : int.tryParse('$v') ?? 0;

  Future<void> _openFile() async {
    final files = _files;
    if (files == null || _busy) return;
    if (!_document.isBlank || _large != null) {
      final ok = await showDialog<bool>(
        context: context,
        builder: (context) => AlertDialog(
          title: const Text('파일 열기'),
          content: const Text('지금 편집 중인 내용 대신 고른 파일을 엽니다. 계속할까요?'),
          actions: [
            TextButton(onPressed: () => Navigator.pop(context, false), child: const Text('취소')),
            FilledButton(key: const ValueKey('memo-open-confirm'), onPressed: () => Navigator.pop(context, true), child: const Text('열기')),
          ],
        ),
      );
      if (ok != true || !mounted) return;
    }
    await _largeKey.currentState?.commit();
    setState(() => _busy = true);
    var opened = false;
    try {
      final selection = await files.pick(save: false);
      if (selection == null) return;
      final info = await files.open(selection);
      final doc = _num(info['doc']);
      final name = info['name'] is String ? info['name'] as String : null;
      final small = _num(info['len']) <= widget.largeFileBytes && _num(info['lines']) <= widget.largeFileLines;
      if (small) {
        final String text;
        try {
          text = await fetchEditorText(files.backend, doc);
        } catch (_) {
          await files.close(doc);
          rethrow;
        }
        _setLarge(null);
        _replaceCore(KlibEditorCore.adopt(
          files.backend,
          doc: doc,
          text: text,
          lineEnding: '${info['line_ending'] ?? 'CRLF'}',
          encoding: '${info['encoding'] ?? 'UTF-8'}',
          fileName: name,
        ));
        opened = true;
      } else {
        _setLarge(LargeTextSession(files.backend, doc: doc, info: info));
      }
      if (name != null && (_title.text.trim().isEmpty || _title.text.trim() == '새 메모')) {
        final dot = name.lastIndexOf('.');
        _title.text = dot > 0 ? name.substring(0, dot) : name;
      }
      final target = _fileTarget;
      _fileMessage = '열었습니다: ${name ?? '파일'} (${memoFileFormatLabel(target?.encoding ?? '${info['encoding']}', target?.lineEnding ?? '${info['line_ending']}')}'
          '${_large != null ? ', 큰 파일 보기' : ''})';
    } catch (error) {
      _fileMessage = '파일을 열지 못했습니다: $error';
    } finally {
      if (mounted) {
        setState(() => _busy = false);
        if (opened) _scheduleSave();
      }
    }
  }

  Future<void> _saveAs() async {
    final files = _files;
    final target = _fileTarget;
    if (files == null || target == null || _busy) return;
    await _largeKey.currentState?.commit();
    if (!mounted) return;
    final choice = await showDialog<MemoSaveChoice>(
      context: context,
      builder: (_) => MemoSaveDialog(encoding: target.encoding, lineEnding: target.lineEnding, fileName: target.fileName, check: target.unencodable),
    );
    if (choice == null || !mounted) return;
    final place = choice.goTo;
    if (place != null) {
      await _goTo(place.line, place.column);
      return;
    }
    setState(() => _busy = true);
    try {
      final selection = await files.pick(save: true);
      if (selection == null) return;
      await target.saveToFile(selection, encoding: choice.encoding, lineEnding: choice.lineEnding);
      _fileMessage = '저장했습니다: ${target.fileName ?? '파일'} (${memoFileFormatLabel(target.encoding, target.lineEnding)})';
    } catch (error) {
      _fileMessage = '저장하지 못했습니다: $error';
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  /// 줄·열(1부터, 글자 단위)로 커서를 옮기고 그 글자를 선택한다.
  Future<void> _goTo(int line, int column) async {
    if (_large != null) {
      await _largeKey.currentState?.goTo(line, column);
      return;
    }
    final text = _body.text;
    var offset = 0;
    for (var l = 1; l < line; l++) {
      final next = text.indexOf('\n', offset);
      if (next < 0) break;
      offset = next + 1;
    }
    int step(int at) {
      final u = text.codeUnitAt(at);
      return (u >= 0xD800 && u <= 0xDBFF && at + 1 < text.length) ? 2 : 1;
    }

    for (var c = 1; c < column && offset < text.length && text.codeUnitAt(offset) != 0x0A; c++) {
      offset += step(offset);
    }
    final end = offset < text.length && text.codeUnitAt(offset) != 0x0A ? offset + step(offset) : offset;
    // 커서(extent)는 그 글자 앞: 상태 표시줄이 알린 줄·열과 같게.
    _body.selection = TextSelection(baseOffset: end, extentOffset: offset);
    _bodyFocus.requestFocus();
  }

  // ---- 확대·축소 ----

  void _setZoom(double zoom) {
    final z = (zoom * 10).roundToDouble() / 10;
    setState(() => _zoom = z.clamp(kMemoZoomMin, kMemoZoomMax));
  }

  void _onPointerSignal(PointerSignalEvent event) {
    if (event is! PointerScrollEvent || !HardwareKeyboard.instance.isControlPressed) return;
    GestureBinding.instance.pointerSignalResolver.register(event, (e) {
      final dy = (e as PointerScrollEvent).scrollDelta.dy;
      if (dy != 0) _setZoom(_zoom + (dy < 0 ? 0.1 : -0.1));
    });
  }

  // ---- 찾기·바꾸기 ----

  void _openFind({bool replace = false}) {
    final sel = _body.selection;
    if (_large == null && sel.isValid && !sel.isCollapsed) {
      final picked = sel.textInside(_body.text);
      if (!picked.contains('\n')) _findQuery.text = picked;
    }
    setState(() {
      _findOpen = true;
      _replaceOpen = replace || _replaceOpen && _findOpen;
      _findMessage = null;
    });
    _findQuery.selection = TextSelection(baseOffset: 0, extentOffset: _findQuery.text.length);
    _findFocus.requestFocus();
  }

  void _closeFind() {
    setState(() {
      _findOpen = false;
      _replaceOpen = false;
      _findMessage = null;
    });
    _body.highlight = TextRange.empty;
    _bodyFocus.requestFocus();
  }

  void _showMatch() {
    final sel = _core.value.selection;
    _body.highlight = sel.isValid && !sel.isCollapsed ? TextRange(start: sel.start, end: sel.end) : TextRange.empty;
  }

  Future<void> _find({bool backward = false}) async {
    final query = _findQuery.text;
    if (query.isEmpty) {
      if (!_findOpen) _openFind();
      return;
    }
    final large = _largeKey.currentState;
    if (_large != null && large != null) {
      final ok = await large.find(query, _findOptions, backward: backward);
      if (!mounted) return;
      if (!ok) {
        setState(() => _findMessage = "'$query'을(를) 찾을 수 없습니다.");
        return;
      }
      var n = 0;
      try {
        n = await _large!.countMatches(query, _findOptions);
      } catch (_) {}
      if (mounted) setState(() => _findMessage = n >= 1000 ? '1000개 이상 일치' : '$n개 일치');
      return;
    }
    final found = await _runCore(() => _core.find(query, _findOptions, backward: backward));
    if (!mounted) return;
    if (found == null) {
      setState(() => _findMessage = "'$query'을(를) 찾을 수 없습니다.");
      _body.highlight = TextRange.empty;
      return;
    }
    _showMatch();
    final count = await _runCore(() => _core.countMatches(query, _findOptions));
    if (mounted) setState(() => _findMessage = '${count ?? 0}개 일치');
    _showMatch();
  }

  Future<void> _replaceNext() async {
    final query = _findQuery.text;
    if (query.isEmpty) return;
    if (_large != null) {
      setState(() => _findMessage = '큰 파일 보기에서는 모두 바꾸기만 됩니다.');
      return;
    }
    final replaced = await _runCore(() => _core.replaceNext(query, _replaceWith.text, _findOptions));
    if (!mounted) return;
    final sel = _core.value.selection;
    final hasMatch = sel.isValid && !sel.isCollapsed;
    setState(() {
      _findMessage = replaced == true
          ? (hasMatch ? '바꿨습니다. 다음 일치를 선택했습니다.' : '바꿨습니다. 더 없습니다.')
          : (hasMatch ? null : "'$query'을(를) 찾을 수 없습니다.");
    });
    _showMatch();
  }

  Future<void> _replaceAll() async {
    final query = _findQuery.text;
    if (query.isEmpty) return;
    final large = _largeKey.currentState;
    final n = _large != null && large != null
        ? await large.replaceAll(query, _replaceWith.text, _findOptions)
        : await _runCore(() => _core.replaceAll(query, _replaceWith.text, _findOptions)) ?? 0;
    if (!mounted) return;
    _body.highlight = TextRange.empty;
    setState(() => _findMessage = n == 0 ? "'$query'을(를) 찾을 수 없습니다." : '$n개를 바꿨습니다.');
  }

  Future<void> _delete() async {
    final ok = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('메모 삭제'),
        content: const Text('이 메모를 지울까요? 되돌릴 수 없습니다.'),
        actions: [
          TextButton(onPressed: () => Navigator.pop(context, false), child: const Text('취소')),
          FilledButton(
            key: const ValueKey('memo-delete-ok'),
            onPressed: () => Navigator.pop(context, true),
            child: const Text('삭제'),
          ),
        ],
      ),
    );
    if (ok == true) {
      _saveTimer?.cancel();
      _saveTimer = null;
      widget.onDelete?.call();
    }
  }

  String get _saveLabel => switch (_save) {
    MemoSaveState.saved => 'Saved',
    MemoSaveState.pending || MemoSaveState.saving => '저장 중…',
    MemoSaveState.failed => '저장 실패',
  };

  Widget _tool(String key, String tooltip, IconData icon, VoidCallback? onPressed) => IconButton(
    key: ValueKey(key),
    tooltip: tooltip,
    visualDensity: VisualDensity.compact,
    iconSize: 20,
    onPressed: onPressed,
    icon: Icon(icon),
  );

  Widget _toolbar() {
    const divider = SizedBox(height: 24, child: VerticalDivider(width: 12));
    final large = _large;
    // 큰 파일 보기에서는 서식 단추를 끈다(평문 편집).
    VoidCallback? fmt(VoidCallback f) => large == null ? f : null;
    final canUndo = large?.canUndo ?? _core.canUndo;
    final canRedo = large?.canRedo ?? _core.canRedo;
    return Wrap(
      key: const ValueKey('memo-toolbar'),
      crossAxisAlignment: WrapCrossAlignment.center,
      children: [
        if (widget.fileBackend != null) ...[
          _tool('memo-open-file', '텍스트 파일 열기 (Ctrl+O)', Icons.folder_open_outlined, () => unawaited(_openFile())),
          _tool(
            'memo-save-file',
            '다른 이름으로 저장 — 인코딩·줄바꿈 선택 (Ctrl+Shift+S)',
            Icons.save_as_outlined,
            _fileTarget == null ? null : () => unawaited(_saveAs()),
          ),
          divider,
        ],
        _tool('memo-undo', '실행취소 (Ctrl+Z)', Icons.undo, canUndo ? () => unawaited(_undo()) : null),
        _tool('memo-redo', '다시실행 (Ctrl+Y)', Icons.redo, canRedo ? () => unawaited(_redo()) : null),
        divider,
        Tooltip(
          message: '문단 형식',
          child: DropdownButton<int>(
            key: const ValueKey('memo-block-style'),
            value: large == null ? _core.blockStyle : 0,
            underline: const SizedBox.shrink(),
            isDense: true,
            items: [
              for (final e in kMemoBlockStyles.entries)
                DropdownMenuItem(key: ValueKey('memo-block-style-${e.key}'), value: e.key, child: Text(e.value)),
            ],
            onChanged: large != null ? null : (level) async {
              if (level == null) return;
              await _runCore(() => _core.setBlockStyle(level));
              _bodyFocus.requestFocus();
            },
          ),
        ),
        divider,
        _tool('memo-bold', '굵게 (Ctrl+B)', Icons.format_bold, fmt(() => unawaited(_command(EditorCommand.bold)))),
        _tool('memo-italic', '기울임 (Ctrl+I)', Icons.format_italic, fmt(() => unawaited(_command(EditorCommand.italic)))),
        _tool('memo-link', '링크 (Ctrl+K)', Icons.link, fmt(() => unawaited(_link()))),
        divider,
        _tool('memo-code', '인라인 코드', Icons.code, fmt(() => unawaited(_command(EditorCommand.inlineCode)))),
        _tool('memo-code-block', '코드 블록', Icons.data_object, fmt(() => unawaited(_command(EditorCommand.codeBlock)))),
        divider,
        _tool('memo-bullet', '글머리 목록', Icons.format_list_bulleted, fmt(() => unawaited(_command(EditorCommand.bulletList)))),
        _tool('memo-numbered', '번호 목록', Icons.format_list_numbered, fmt(() => unawaited(_command(EditorCommand.numberedList)))),
        _tool('memo-quote', '인용', Icons.format_quote, fmt(() => unawaited(_command(EditorCommand.quote)))),
        _tool('memo-rule', '구분선', Icons.horizontal_rule, fmt(() => unawaited(_command(EditorCommand.horizontalRule)))),
        divider,
        _tool('memo-clear', '서식 지우기', Icons.format_clear, fmt(() => unawaited(_command(EditorCommand.clearFormatting)))),
        divider,
        _tool('memo-find', '찾기 (Ctrl+F)', Icons.search, _openFind),
        _tool('memo-replace', '바꾸기 (Ctrl+H)', Icons.find_replace, () => _openFind(replace: true)),
      ],
    );
  }

  Widget _findBar() {
    final muted = Theme.of(context).colorScheme.onSurfaceVariant;
    InputDecoration deco(String hint) =>
        InputDecoration(hintText: hint, isDense: true, border: const OutlineInputBorder(), contentPadding: const EdgeInsets.symmetric(horizontal: 8, vertical: 8));
    return Container(
      key: const ValueKey('memo-find-bar'),
      padding: const EdgeInsets.symmetric(vertical: 6),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              _tool(
                'memo-find-toggle-replace',
                _replaceOpen ? '바꾸기 숨기기' : '바꾸기 보이기',
                _replaceOpen ? Icons.expand_less : Icons.expand_more,
                () => setState(() => _replaceOpen = !_replaceOpen),
              ),
              Expanded(
                child: TextField(
                  key: const ValueKey('memo-find-query'),
                  controller: _findQuery,
                  focusNode: _findFocus,
                  decoration: deco('찾기'),
                  onChanged: (_) => setState(() => _findMessage = null),
                  onSubmitted: (_) async {
                    await _find();
                    _findFocus.requestFocus();
                  },
                ),
              ),
              _tool('memo-find-prev', '이전 찾기 (Shift+F3)', Icons.arrow_upward, () => unawaited(_find(backward: true))),
              _tool('memo-find-next', '다음 찾기 (F3)', Icons.arrow_downward, () => unawaited(_find())),
              Tooltip(
                message: '대/소문자 구분',
                child: FilterChip(
                  key: const ValueKey('memo-find-case'),
                  label: const Text('Aa'),
                  visualDensity: VisualDensity.compact,
                  selected: _caseSensitive,
                  onSelected: (v) => setState(() => _caseSensitive = v),
                ),
              ),
              const SizedBox(width: 4),
              Tooltip(
                message: '온전한 낱말만',
                child: FilterChip(
                  key: const ValueKey('memo-find-word'),
                  label: const Text('낱말'),
                  visualDensity: VisualDensity.compact,
                  selected: _wholeWord,
                  onSelected: (v) => setState(() => _wholeWord = v),
                ),
              ),
              _tool('memo-find-close', '닫기 (Esc)', Icons.close, _closeFind),
            ],
          ),
          if (_replaceOpen)
            Padding(
              padding: const EdgeInsets.only(top: 6, left: 40),
              child: Row(
                children: [
                  Expanded(
                    child: TextField(
                      key: const ValueKey('memo-replace-with'),
                      controller: _replaceWith,
                      decoration: deco('바꿀 내용'),
                      onSubmitted: (_) => unawaited(_replaceNext()),
                    ),
                  ),
                  const SizedBox(width: 6),
                  OutlinedButton(key: const ValueKey('memo-replace-next'), onPressed: () => unawaited(_replaceNext()), child: const Text('바꾸기')),
                  const SizedBox(width: 6),
                  OutlinedButton(key: const ValueKey('memo-replace-all'), onPressed: () => unawaited(_replaceAll()), child: const Text('모두 바꾸기')),
                ],
              ),
            ),
          if (_findMessage != null)
            Padding(
              padding: const EdgeInsets.only(top: 4, left: 40),
              child: Text(_findMessage!, key: const ValueKey('memo-find-message'), style: TextStyle(fontSize: 12, color: muted)),
            ),
        ],
      ),
    );
  }

  Widget _statusBar() {
    final muted = Theme.of(context).colorScheme.onSurfaceVariant;
    final large = _large;
    final value = _body.value;
    final ({int line, int column}) pos;
    final int chars;
    if (large != null) {
      pos = _largeCaret.value;
      chars = large.chars;
    } else {
      final caret = value.selection.isValid ? value.selection.extentOffset : value.text.length;
      pos = lineColumnAt(value.text, caret);
      chars = value.text.runes.length;
    }
    // 상태 표시줄의 인코딩·줄바꿈 = 저장할 때 쓸 값(연 파일에서 감지했거나 마지막으로 저장한 형식).
    final encoding = large?.encoding ?? _core.encoding;
    final lineEnding = large?.lineEnding ?? _core.lineEnding;
    final style = TextStyle(fontSize: 12, color: muted);
    Widget cell(String key, String text, {String? tooltip}) {
      final child = Padding(
        padding: const EdgeInsets.symmetric(horizontal: 10),
        child: Text(text, key: ValueKey(key), style: style),
      );
      return tooltip == null ? child : Tooltip(message: tooltip, child: child);
    }

    const sep = SizedBox(height: 14, child: VerticalDivider(width: 1));
    return Container(
      key: const ValueKey('memo-statusbar'),
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: Row(
        children: [
          Expanded(
            child: Text(
              large?.lostMessage ?? _fileMessage ?? '',
              key: const ValueKey('memo-file-message'),
              style: style.copyWith(color: large?.lostMessage != null ? Theme.of(context).colorScheme.error : null),
              overflow: TextOverflow.ellipsis,
            ),
          ),
          if (large != null) ...[
            cell('memo-status-mode', '큰 파일 보기 · ${large.totalLines}줄', tooltip: '보이는 줄만 불러와 그립니다. 편집 중인 줄만 입력칸이며 서식 단추는 쓰지 않습니다.'),
            sep,
          ],
          cell('memo-status-position', '줄 ${pos.line}, 열 ${pos.column}'),
          sep,
          cell('memo-status-chars', '$chars자'),
          sep,
          cell('memo-status-zoom', '${(_zoom * 100).round()}%', tooltip: 'Ctrl+휠, Ctrl+= / Ctrl+- 로 확대·축소, Ctrl+0 원래 크기'),
          sep,
          cell(
            'memo-status-eol',
            memoLineEndingLabel(lineEnding),
            tooltip: large?.mixedLineEndings == true ? '줄바꿈이 섞여 있습니다. 저장하면 이 형식으로 맞춥니다.' : null,
          ),
          sep,
          cell(
            'memo-status-encoding',
            encoding,
            tooltip: large != null || _core.engineName == 'Rust' ? '편집 엔진: Rust 코어(klib)' : '편집 엔진: 임시(Dart) 코어 — klib 연결 안 됨',
          ),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final muted = Theme.of(context).colorScheme.onSurfaceVariant;
    return CallbackShortcuts(
      // TextField 자체 실행취소 대신 코어 이력을 쓴다(가까운 조상 단축키가 앱 기본 단축키보다 먼저 받는다).
      bindings: {
        const SingleActivator(LogicalKeyboardKey.keyZ, control: true): () => unawaited(_undo()),
        const SingleActivator(LogicalKeyboardKey.keyY, control: true): () => unawaited(_redo()),
        const SingleActivator(LogicalKeyboardKey.keyZ, control: true, shift: true): () => unawaited(_redo()),
        const SingleActivator(LogicalKeyboardKey.keyB, control: true): () => unawaited(_command(EditorCommand.bold)),
        const SingleActivator(LogicalKeyboardKey.keyI, control: true): () => unawaited(_command(EditorCommand.italic)),
        const SingleActivator(LogicalKeyboardKey.keyK, control: true): () => unawaited(_link()),
        const SingleActivator(LogicalKeyboardKey.keyF, control: true): _openFind,
        const SingleActivator(LogicalKeyboardKey.keyH, control: true): () => _openFind(replace: true),
        const SingleActivator(LogicalKeyboardKey.f3): () => unawaited(_find()),
        const SingleActivator(LogicalKeyboardKey.f3, shift: true): () => unawaited(_find(backward: true)),
        const SingleActivator(LogicalKeyboardKey.equal, control: true): () => _setZoom(_zoom + 0.1),
        const SingleActivator(LogicalKeyboardKey.numpadAdd, control: true): () => _setZoom(_zoom + 0.1),
        const SingleActivator(LogicalKeyboardKey.minus, control: true): () => _setZoom(_zoom - 0.1),
        const SingleActivator(LogicalKeyboardKey.numpadSubtract, control: true): () => _setZoom(_zoom - 0.1),
        const SingleActivator(LogicalKeyboardKey.digit0, control: true): () => _setZoom(1),
        const SingleActivator(LogicalKeyboardKey.numpad0, control: true): () => _setZoom(1),
        if (_findOpen) const SingleActivator(LogicalKeyboardKey.escape): _closeFind,
        if (widget.fileBackend != null) ...{
          const SingleActivator(LogicalKeyboardKey.keyO, control: true): () => unawaited(_openFile()),
          const SingleActivator(LogicalKeyboardKey.keyS, control: true, shift: true): () => unawaited(_saveAs()),
        },
      },
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              Expanded(
                child: TextField(
                  key: const ValueKey('memo-title'),
                  controller: _title,
                  style: Theme.of(context).textTheme.titleLarge,
                  decoration: const InputDecoration(hintText: '제목', border: InputBorder.none),
                ),
              ),
              Text(_saveLabel, key: const ValueKey('memo-status'), style: TextStyle(fontSize: 12, color: muted)),
              if (widget.onDelete != null)
                _tool('memo-delete', '메모 삭제', Icons.delete_outline, () => unawaited(_delete())),
            ],
          ),
          _toolbar(),
          if (_findOpen) _findBar(),
          const Divider(height: 1),
          Expanded(
            child: Listener(
              key: const ValueKey('memo-body-zoom'),
              onPointerSignal: _onPointerSignal,
              child: _large != null
                  ? LargeTextView(
                      key: _largeKey,
                      session: _large!,
                      fontSize: kMemoBaseFontSize * _zoom,
                      caret: _largeCaret,
                      onError: (m) {
                        if (mounted) setState(() => _fileMessage = m);
                      },
                    )
                  : TextField(
                key: const ValueKey('memo-body'),
                controller: _body,
                focusNode: _bodyFocus,
                maxLines: null,
                expands: true,
                keyboardType: TextInputType.multiline,
                textAlignVertical: TextAlignVertical.top,
                scrollPhysics: const _CtrlZoomScrollPhysics(),
                style: TextStyle(fontSize: kMemoBaseFontSize * _zoom, height: 1.5),
                decoration: const InputDecoration(
                  hintText: '메모를 쓰세요. 서식은 Markdown으로 저장됩니다.',
                  border: InputBorder.none,
                  contentPadding: EdgeInsets.symmetric(vertical: 12),
                ),
              ),
            ),
          ),
          const Divider(height: 1),
          _statusBar(),
          ?widget.footer,
        ],
      ),
    );
  }
}

class _LinkDialog extends StatefulWidget {
  const _LinkDialog();

  @override
  State<_LinkDialog> createState() => _LinkDialogState();
}

class _LinkDialogState extends State<_LinkDialog> {
  final _controller = TextEditingController(text: 'https://');

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: const Text('링크'),
    content: TextField(
      key: const ValueKey('memo-link-url'),
      controller: _controller,
      autofocus: true,
      decoration: const InputDecoration(labelText: '주소'),
      onSubmitted: (v) => Navigator.pop(context, v),
    ),
    actions: [
      TextButton(onPressed: () => Navigator.pop(context), child: const Text('취소')),
      FilledButton(
        key: const ValueKey('memo-link-ok'),
        onPressed: () => Navigator.pop(context, _controller.text),
        child: const Text('넣기'),
      ),
    ],
  );
}
