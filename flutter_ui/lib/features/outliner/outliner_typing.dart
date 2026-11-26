import 'package:flutter/services.dart';

/// 아웃라이너 괄호 짝 규칙(OUTLINEKEY1).
///
/// 글자마다 엔진 프로세스를 띄울 수 없어 Rust `outliner_core::type_char`·`pair_backspace` 의 Dart 판을 둔다.
/// 두 구현은 `outliner_core/testdata/typing_rules.json` 을 함께 시험한다. 위치는 UTF-16.
const _pairs = {'[': ']', '(': ')', '{': '}', '`': '`'};

/// 글자 하나를 칠 때의 짝 규칙. null 이면 기본 입력.
({String text, int start, int end})? typeChar(
  String text,
  int start,
  int end,
  String char,
) {
  final previous = start > 0 ? text[start - 1] : null;
  if (start == end &&
      const [']', ')', '}', '`'].contains(char) &&
      start < text.length &&
      text[start] == char &&
      (char != '`' || previous != '`')) {
    return (text: text, start: start + 1, end: start + 1);
  }
  final close = _pairs[char];
  if (close == null) return null;
  if (start == end &&
      char == '(' &&
      !const [null, ' ', '\n', ']', '('].contains(previous)) {
    return null;
  }
  return (
    text:
        '${text.substring(0, start)}$char${text.substring(start, end)}$close${text.substring(end)}',
    start: start + 1,
    end: end + 1,
  );
}

/// 빈 짝 한가운데의 Backspace 는 둘 다 지운다.
({String text, int caret})? pairBackspace(String text, int caret) {
  if (caret <= 0 || caret >= text.length) return null;
  if (_pairs[text[caret - 1]] != text[caret]) return null;
  return (
    text: text.substring(0, caret - 1) + text.substring(caret + 1),
    caret: caret - 1,
  );
}

/// 한 글자 입력·삭제에만 짝 규칙을 적용한다. 한글 조합 중에는 손대지 않는다.
class OutlinerPairFormatter extends TextInputFormatter {
  @override
  TextEditingValue formatEditUpdate(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) {
    if (newValue.composing.isValid && !newValue.composing.isCollapsed) {
      return newValue;
    }
    final selection = oldValue.selection;
    if (!selection.isValid || !newValue.selection.isCollapsed) return newValue;
    final before = oldValue.text;
    final after = newValue.text;
    final caret = newValue.selection.baseOffset;
    final head = before.substring(0, selection.start);
    final tail = before.substring(selection.end);
    if (caret == selection.start + 1 &&
        after.length == head.length + 1 + tail.length &&
        after.startsWith(head) &&
        after.endsWith(tail)) {
      final result = typeChar(
        before,
        selection.start,
        selection.end,
        after[selection.start],
      );
      if (result != null) {
        return TextEditingValue(
          text: result.text,
          selection: TextSelection(
            baseOffset: result.start,
            extentOffset: result.end,
          ),
        );
      }
    }
    if (selection.isCollapsed &&
        selection.start > 0 &&
        caret == selection.start - 1 &&
        after == before.substring(0, selection.start - 1) + tail) {
      final result = pairBackspace(before, selection.start);
      if (result != null) {
        return TextEditingValue(
          text: result.text,
          selection: TextSelection.collapsed(offset: result.caret),
        );
      }
    }
    return newValue;
  }
}
