import 'package:flutter/services.dart';

/// 명령 처리 중에 친 글(OUTKEYS2)을 떼어 내고 다른 칸·커서에 다시 넣는 규칙. 위치는 UTF-16.
///
/// [typedSince] 는 [base](선택 포함)에서 [now] 가 되며 새로 친 글을 돌려준다. 돌려주는 값의
/// 선택은 그 글 안의 커서다. 커서 자리가 아닌 곳에 넣은 경우에도 새 글은 잃지 않는다
/// (그때 지운 옛 글은 지워지지 않은 채 남는다). 조합 표시는 옮기지 않는다.
TextEditingValue typedSince(TextEditingValue base, TextEditingValue now) {
  final before = base.text;
  final after = now.text;
  if (after == before) return TextEditingValue.empty;
  final selection = _selectionOf(base);
  var head = selection.start;
  var tail = before.length - selection.end;
  if (after.length < head + tail ||
      !after.startsWith(before.substring(0, head)) ||
      !after.endsWith(before.substring(selection.end))) {
    final limit = before.length < after.length ? before.length : after.length;
    head = 0;
    while (head < limit && before[head] == after[head]) {
      head++;
    }
    if (head > 0 && _isHighSurrogate(before.codeUnitAt(head - 1))) head--;
    tail = 0;
    while (tail < limit - head &&
        before[before.length - 1 - tail] == after[after.length - 1 - tail]) {
      tail++;
    }
    if (tail > 0 && _isLowSurrogate(before.codeUnitAt(before.length - tail))) {
      tail--;
    }
  }
  final text = after.substring(head, after.length - tail);
  final caret = _selectionOf(now);
  return TextEditingValue(
    text: text,
    selection: TextSelection(
      baseOffset: (caret.start - head).clamp(0, text.length),
      extentOffset: (caret.end - head).clamp(0, text.length),
    ),
  );
}

/// [typed] 를 [value] 의 커서(선택) 자리에 넣는다.
TextEditingValue insertTyped(TextEditingValue value, TextEditingValue typed) {
  if (typed.text.isEmpty) return value;
  final at = _selectionOf(value);
  return TextEditingValue(
    text: value.text.replaceRange(at.start, at.end, typed.text),
    selection: TextSelection(
      baseOffset: at.start + typed.selection.start,
      extentOffset: at.start + typed.selection.end,
    ),
  );
}

/// 순서가 바르고 범위 안인 선택(유효하지 않으면 글 끝).
TextSelection _selectionOf(TextEditingValue value) {
  final length = value.text.length;
  final s = value.selection;
  if (!s.isValid) return TextSelection.collapsed(offset: length);
  return TextSelection(
    baseOffset: s.start.clamp(0, length),
    extentOffset: s.end.clamp(0, length),
  );
}

bool _isHighSurrogate(int unit) => unit >= 0xD800 && unit <= 0xDBFF;
bool _isLowSurrogate(int unit) => unit >= 0xDC00 && unit <= 0xDFFF;
