/// 큰 글 보기 화면(3단계). 보이는 줄만 그리고, 편집 중인 한 줄(문단)만 `TextField`로 둔다.
///
/// - 줄은 고정 높이 목록(`ListView.builder` + `itemExtent`)이라 10만 줄이어도 화면에 보이는 수십 줄만 만든다.
///   글은 [LargeTextSession.lineAt]으로 캐시에서 읽고, 없으면 세션이 그 묶음을 받아 온 뒤 다시 그린다.
/// - 자동 줄 바꿈은 하지 않는다(메모장 '자동 줄 바꿈' 끔과 같음). 긴 줄은 잘려 보이고, 편집 칸에서는 가로로 흐른다.
/// - 줄을 누르면 그 줄이 입력칸이 된다. 한글 IME 조합은 그 TextField가 그대로 처리한다(조합 중에는 아래 키를 가로채지 않음).
///   Enter = 줄 나누기, 줄 맨 앞 Backspace = 앞 줄과 합치기, 줄 끝 Delete = 다음 줄과 합치기, ↑/↓ = 위아래 줄,
///   여러 줄 붙여넣기(Ctrl+V)는 세션에 바로 넣는다. 다른 줄로 옮기거나 입력칸이 포커스를 잃으면 그 줄을 확정해 Rust에 보낸다.
library;

import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'large_text_session.dart';
import 'text_offsets.dart';

/// 상태 표시줄용 커서 위치(1부터).
typedef LargeCaret = ({int line, int column});

class LargeTextView extends StatefulWidget {
  const LargeTextView({super.key, required this.session, required this.fontSize, required this.caret, this.onError});

  final LargeTextSession session;
  final double fontSize;
  final ValueNotifier<LargeCaret> caret;

  /// 편집 요청이 실패했을 때 안내.
  final void Function(String message)? onError;

  @override
  State<LargeTextView> createState() => LargeTextViewState();
}

class LargeTextViewState extends State<LargeTextView> {
  final _scroll = ScrollController();
  final _lineText = TextEditingController();
  late final _lineFocus = FocusNode(onKeyEvent: _onKey);

  /// 편집 중인 줄(0부터)과 그 줄의 처음 글(확정 여부 판단).
  int? _editing;
  String _original = '';
  bool _busy = false;
  TextRange _highlight = TextRange.empty;
  int? _highlightLine;

  LargeTextSession get _session => widget.session;
  double get lineHeight => (widget.fontSize * 1.5).ceilToDouble();
  TextStyle get _style => TextStyle(fontSize: widget.fontSize, height: 1.5);

  /// 편집 중인 줄(시험용).
  int? get editingLine => _editing;

  @override
  void initState() {
    super.initState();
    _session.addListener(_onSession);
    _lineText.addListener(_reportCaret);
    _lineFocus.addListener(() {
      if (!_lineFocus.hasFocus) unawaited(commit());
    });
  }

  @override
  void didUpdateWidget(LargeTextView oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.session != widget.session) {
      oldWidget.session.removeListener(_onSession);
      widget.session.addListener(_onSession);
      _editing = null;
    }
  }

  @override
  void dispose() {
    _session.removeListener(_onSession);
    _scroll.dispose();
    _lineText.dispose();
    _lineFocus.dispose();
    super.dispose();
  }

  void _onSession() {
    if (mounted) setState(() {});
  }

  void _reportCaret() {
    final line = _editing;
    if (line == null) return;
    final sel = _lineText.selection;
    final caret = sel.isValid ? sel.extentOffset.clamp(0, _lineText.text.length) : 0;
    widget.caret.value = (line: line + 1, column: _lineText.text.substring(0, caret).runes.length + 1);
  }

  void _fail(Object error) => widget.onError?.call('$error');

  /// 지금 커서의 바이트 위치(찾기 시작점). 편집 중이 아니면 보이는 첫 줄의 시작.
  int get caretByte {
    final line = _editing ?? (_scroll.hasClients ? (_scroll.offset / lineHeight).floor() : 0);
    final l = _session.lineAt(line);
    if (l == null || l.tooLong) return 0;
    if (_editing == null) return l.start;
    final sel = _lineText.selection;
    return l.start + utf16ToUtf8(_original, sel.isValid ? sel.extentOffset.clamp(0, _original.length) : 0);
  }

  /// 편집 중인 줄을 확정한다(바뀌었으면 Rust에 보냄).
  Future<void> commit() async {
    final line = _editing;
    if (line == null || _lineText.value.composing.isValid) return;
    final text = _lineText.text;
    if (text == _original) return;
    _original = text;
    try {
      await _session.replaceLine(line, text);
    } catch (e) {
      _fail(e);
    }
  }

  void _scrollTo(int line) {
    if (!_scroll.hasClients) return;
    final top = line * lineHeight;
    final view = _scroll.position.viewportDimension;
    final now = _scroll.offset;
    if (top < now) {
      _scroll.jumpTo(top);
    } else if (top + lineHeight > now + view) {
      _scroll.jumpTo((top + lineHeight - view).clamp(0, _scroll.position.maxScrollExtent));
    }
  }

  /// [line] 줄을 입력칸으로 만들고 커서(UTF-16)를 [start]~[end]에 둔다.
  Future<bool> editLine(int line, int start, {int? end, bool scroll = true}) async {
    if (line < 0 || line >= _session.totalLines) return false;
    final l = _session.lineAt(line) ?? await _session.ensureLine(line);
    if (!mounted || l == null) return false;
    if (l.tooLong) {
      _fail('이 줄은 너무 길어 큰 파일 보기에서 편집할 수 없습니다.');
      return false;
    }
    setState(() {
      _editing = line;
      _original = l.text;
      _highlight = TextRange.empty;
      _lineText.value = TextEditingValue(
        text: l.text,
        selection: TextSelection(baseOffset: start.clamp(0, l.text.length), extentOffset: (end ?? start).clamp(0, l.text.length)),
      );
    });
    _reportCaret();
    if (scroll) _scrollTo(line);
    _lineFocus.requestFocus();
    return true;
  }

  Future<void> _guard(Future<void> Function() op) async {
    if (_busy) return;
    _busy = true;
    try {
      await op();
    } catch (e) {
      _fail(e);
    } finally {
      _busy = false;
    }
  }

  Future<void> _tapLine(int line, double dx) => _guard(() async {
    await commit();
    final l = _session.lineAt(line);
    if (l == null) return;
    final painter = TextPainter(text: TextSpan(text: l.text, style: _style), textDirection: TextDirection.ltr, maxLines: 1)..layout();
    final pos = painter.getPositionForOffset(Offset(dx, lineHeight / 2)).offset;
    painter.dispose();
    await editLine(line, pos, scroll: false);
  });

  int get _caret {
    final sel = _lineText.selection;
    return sel.isValid ? sel.extentOffset.clamp(0, _lineText.text.length) : _lineText.text.length;
  }

  KeyEventResult _onKey(FocusNode node, KeyEvent event) {
    if (event is KeyUpEvent || _editing == null) return KeyEventResult.ignored;
    if (_lineText.value.composing.isValid) return KeyEventResult.ignored; // IME 조합 중
    final keys = HardwareKeyboard.instance;
    final line = _editing!;
    final sel = _lineText.selection;
    final collapsed = sel.isValid && sel.isCollapsed;
    final key = event.logicalKey;
    if (_busy) {
      // 확정 요청을 기다리는 동안 줄 구조를 바꾸는 키는 버린다.
      final structural = {LogicalKeyboardKey.enter, LogicalKeyboardKey.numpadEnter, LogicalKeyboardKey.arrowUp, LogicalKeyboardKey.arrowDown};
      return structural.contains(key) ? KeyEventResult.handled : KeyEventResult.ignored;
    }
    if (key == LogicalKeyboardKey.enter || key == LogicalKeyboardKey.numpadEnter) {
      final text = _lineText.text;
      final a = sel.isValid ? sel.start : text.length;
      final b = sel.isValid ? sel.end : text.length;
      unawaited(_guard(() async {
        _original = text; // 한 번에 보낸다(나누기 = 한 번의 replace)
        await _session.replaceLine(line, '${text.substring(0, a)}\n${text.substring(b)}');
        await editLine(line + 1, 0);
      }));
      return KeyEventResult.handled;
    }
    if (key == LogicalKeyboardKey.backspace && collapsed && sel.start == 0 && line > 0 && !keys.isControlPressed) {
      unawaited(_guard(() async {
        await commit();
        if (_session.lineAt(line - 1) == null) await _session.ensureLine(line - 1);
        final at = await _session.joinWithPrevious(line);
        await editLine(line - 1, at);
      }));
      return KeyEventResult.handled;
    }
    if (key == LogicalKeyboardKey.delete && collapsed && sel.start == _lineText.text.length && line + 1 < _session.totalLines) {
      unawaited(_guard(() async {
        await commit();
        if (_session.lineAt(line + 1) == null) await _session.ensureLine(line + 1);
        final at = await _session.joinWithPrevious(line + 1);
        await editLine(line, at);
      }));
      return KeyEventResult.handled;
    }
    if ((key == LogicalKeyboardKey.arrowUp || key == LogicalKeyboardKey.arrowDown) && !keys.isShiftPressed) {
      final target = line + (key == LogicalKeyboardKey.arrowUp ? -1 : 1);
      if (target < 0 || target >= _session.totalLines) return KeyEventResult.handled;
      final col = _caret;
      unawaited(_guard(() async {
        await commit();
        await editLine(target, col);
      }));
      return KeyEventResult.handled;
    }
    if (key == LogicalKeyboardKey.keyV && keys.isControlPressed && !keys.isShiftPressed) {
      unawaited(_guard(_pasteMultiline));
      return KeyEventResult.handled;
    }
    return KeyEventResult.ignored;
  }

  /// 입력칸은 한 줄이라 줄바꿈이 든 붙여넣기는 세션에 바로 넣는다(한 번의 실행취소).
  Future<void> _pasteMultiline() async {
    final line = _editing;
    if (line == null) return;
    final clip = (await Clipboard.getData(Clipboard.kTextPlain))?.text ?? '';
    if (clip.isEmpty) return;
    final text = _lineText.text;
    final sel = _lineText.selection;
    final a = sel.isValid ? sel.start : text.length;
    final b = sel.isValid ? sel.end : text.length;
    final pasted = clip.replaceAll('\r\n', '\n').replaceAll('\r', '\n');
    if (!pasted.contains('\n')) {
      _lineText.value = TextEditingValue(
        text: text.replaceRange(a, b, pasted),
        selection: TextSelection.collapsed(offset: a + pasted.length),
      );
      return;
    }
    _original = text;
    await _session.replaceLine(line, text.replaceRange(a, b, pasted));
    final parts = pasted.split('\n');
    await editLine(line + parts.length - 1, parts.last.length + (parts.length == 1 ? a : 0));
  }

  // ---- 메모 편집기가 부르는 명령 ----

  Future<void> undo({bool redo = false}) => _guard(() async {
    await commit();
    final cursor = await _session.history(redo: redo);
    if (cursor == null) return;
    final line = await _session.lineOf(cursor);
    final l = await _session.ensureLine(line);
    setState(() => _editing = null);
    if (l != null && !l.tooLong) await editLine(line, utf8ToUtf16(l.text, cursor - l.start));
  });

  /// 찾기. 찾으면 그 줄을 입력칸으로 만들고 일치 구간을 선택한다.
  Future<bool> find(String query, FindOptions options, {bool backward = false}) async {
    var found = false;
    await _guard(() async {
      await commit();
      final sel = _lineText.selection;
      final l = _editing == null ? null : _session.lineAt(_editing!);
      int from;
      if (l != null && sel.isValid) {
        from = l.start + utf16ToUtf8(l.text, backward ? sel.start : sel.end);
      } else {
        from = caretByte;
      }
      final m = await _session.find(query, options, from, backward: backward);
      if (m == null) return;
      found = await editLine(m.line, m.start, end: m.end);
      if (mounted) {
        setState(() {
          _highlightLine = m.line;
          _highlight = TextRange(start: m.start, end: m.end);
        });
      }
    });
    return found;
  }

  /// 줄·열(1부터)로 이동(변환 불가 글자 '이동').
  Future<void> goTo(int line, int column) => _guard(() async {
    await commit();
    final l = await _session.ensureLine(line - 1);
    if (l == null || l.tooLong) return;
    var unit = 0;
    var col = 1;
    while (unit < l.text.length && col < column) {
      final u = l.text.codeUnitAt(unit);
      unit += (u >= 0xD800 && u <= 0xDBFF && unit + 1 < l.text.length) ? 2 : 1;
      col++;
    }
    await editLine(line - 1, unit, end: (unit + (unit < l.text.length && l.text.codeUnitAt(unit) >= 0xD800 && l.text.codeUnitAt(unit) <= 0xDBFF ? 2 : 1)).clamp(0, l.text.length));
  });

  Future<int> replaceAll(String query, String replacement, FindOptions options) async {
    var n = 0;
    await _guard(() async {
      await commit();
      n = await _session.replaceAll(query, replacement, options);
      if (n > 0 && mounted) setState(() => _editing = null);
    });
    return n;
  }

  Widget _row(BuildContext context, int index) {
    final scheme = Theme.of(context).colorScheme;
    final number = Container(
      width: 64,
      alignment: Alignment.centerRight,
      padding: const EdgeInsets.only(right: 10),
      child: Text('${index + 1}', style: _style.copyWith(color: scheme.onSurfaceVariant.withValues(alpha: 0.6), fontSize: widget.fontSize * 0.8)),
    );
    final Widget body;
    if (index == _editing) {
      body = TextField(
        key: const ValueKey('memo-large-line-input'),
        controller: _lineText,
        focusNode: _lineFocus,
        maxLines: 1,
        style: _style,
        decoration: const InputDecoration.collapsed(hintText: ''),
      );
    } else {
      final line = _session.lineAt(index);
      final text = line?.text ?? '';
      final hl = index == _highlightLine && _highlight.isValid && _highlight.end <= text.length ? _highlight : null;
      body = GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTapDown: (d) => unawaited(_tapLine(index, d.localPosition.dx)),
        child: Text.rich(
          hl == null
              ? TextSpan(text: text)
              : TextSpan(children: [
                  TextSpan(text: text.substring(0, hl.start)),
                  TextSpan(text: text.substring(hl.start, hl.end), style: const TextStyle(backgroundColor: Color(0x66FFC107))),
                  TextSpan(text: text.substring(hl.end)),
                ]),
          key: ValueKey('memo-large-line-$index'),
          maxLines: 1,
          softWrap: false,
          overflow: TextOverflow.clip,
          style: line == null ? _style.copyWith(color: scheme.onSurfaceVariant) : (line.tooLong ? _style.copyWith(color: scheme.error) : _style),
        ),
      );
    }
    return Row(children: [number, Expanded(child: body)]);
  }

  @override
  Widget build(BuildContext context) {
    return Scrollbar(
      controller: _scroll,
      thumbVisibility: true,
      child: ListView.builder(
        key: const ValueKey('memo-large-view'),
        controller: _scroll,
        itemExtent: lineHeight,
        itemCount: _session.totalLines,
        itemBuilder: _row,
      ),
    );
  }
}
