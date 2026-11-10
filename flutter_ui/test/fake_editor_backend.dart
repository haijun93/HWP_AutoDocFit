/// 시험용 가짜 klib 편집 코어(edit_*). Rust editor_core처럼 UTF-8 바이트 위치를 받고,
/// 글자 경계가 아닌 위치는 오류로 거절해 Dart 쪽 UTF-16↔바이트 변환을 검증한다.
/// 서식 연산은 Dart markdown_format을 빌려 쓴다(서식 결과 자체는 Rust cargo test가 검증).
library;

import 'dart:convert';

import 'package:flutter/services.dart';
import 'package:docfit_ui/features/editor/markdown_format.dart' as md;
import 'package:docfit_ui/features/editor/text_offsets.dart';

class _Doc {
  _Doc(this.text, {this.encoding = 'UTF-8', this.lineEnding = 'CRLF', this.name});
  String text;
  String encoding;
  String lineEnding;
  String? name;
  final undo = <String>[];
  final redo = <String>[];
  bool grouping = false;
}

/// 가짜 파일(호스트 파일 대화상자로 고른 파일). [text]는 원문 줄바꿈 그대로.
class FakeTextFile {
  const FakeTextFile(this.name, this.text, {this.encoding = 'UTF-8'});
  final String name;
  final String text;
  final String encoding;
}

/// 가짜 저장 기록.
class FakeSaved {
  const FakeSaved(this.selectionId, this.encoding, this.lineEnding, this.text);
  final String selectionId;
  final String encoding;
  final String lineEnding;

  /// 저장한 줄바꿈으로 바꾼 글.
  final String text;
}

/// CP949로 바꿀 수 없는 글자(가짜 규칙: BMP 밖 글자 = 이모지 등).
bool fakeUnencodableCp949(int rune) => rune > 0xFFFF;

class FakeEditorBackend {
  final docs = <int, _Doc>{};

  /// edit_select_file이 차례로 돌려줄 선택 번호(null = 취소).
  final pickQueue = <String?>[];

  /// 선택 번호 → 열 파일.
  final files = <String, FakeTextFile>{};
  final saved = <FakeSaved>[];
  final log = <Map<String, Object?>>[];
  int _next = 1;

  /// edit_open을 실패시킨다(klib 없음 흉내).
  bool failOpen = false;

  /// 요청 JSON 크기 한도(Python 브리지 64 KiB 흉내). null이면 없음.
  int? maxRequestBytes;

  /// edit_line_range 응답 크기 한도(줄 목록 JSON 기준). null이면 없음.
  int? maxResponseBytes;

  /// klib가 유휴 종료 후 다시 떠서 문서를 모두 잃은 상태를 흉내 낸다.
  void restart() => docs.clear();

  Iterable<String> get commands => log.map((r) => '${r['cmd']}');

  Future<Map<String, Object?>> call(Map<String, Object?> req) async {
    log.add(req);
    try {
      return {'ok': true, ..._handle(req)};
    } on StateError catch (e) {
      return {'ok': false, 'message': e.message};
    }
  }

  static int _u16(String text, int byte) {
    final bytes = utf8.encode(text);
    if (byte < 0 || byte > bytes.length || (byte < bytes.length && (bytes[byte] & 0xC0) == 0x80)) {
      throw StateError('위치 $byte는 글자 경계가 아닙니다.');
    }
    return utf8.decode(bytes.sublist(0, byte)).length;
  }

  static int _b(String text, int unit) => utf8Length(text, 0, unit);

  Map<String, Object?> _state(_Doc d, int cursor) => {
    'cursor': cursor,
    'len': utf8.encode(d.text).length,
    'chars': d.text.runes.length,
    'lines': '\n'.allMatches(d.text).length + 1,
    'dirty': true,
    'can_undo': d.undo.isNotEmpty,
    'can_redo': d.redo.isNotEmpty,
  };

  void _push(_Doc d) {
    d.undo.add(d.text);
    d.redo.clear();
  }

  Map<String, Object?> _format(_Doc d, Map<String, Object?> req, TextEditingValue Function(TextEditingValue v) op) {
    final s = _u16(d.text, req['start'] as int? ?? req['pos'] as int);
    final e = _u16(d.text, req['end'] as int? ?? req['pos'] as int);
    final next = op(TextEditingValue(text: d.text, selection: TextSelection(baseOffset: s, extentOffset: e)));
    d.grouping = false;
    if (next.text != d.text) {
      _push(d);
      d.text = next.text;
    }
    final a = _b(d.text, next.selection.start);
    final b = _b(d.text, next.selection.end);
    return {..._state(d, b), 'sel_start': a, 'sel_end': b};
  }

  Map<String, Object?> _handle(Map<String, Object?> req) {
    if (maxRequestBytes != null && utf8.encode(jsonEncode(req)).length > maxRequestBytes!) {
      throw StateError('가상 폴더 요청이 64 KiB를 초과했습니다.');
    }
    final cmd = req['cmd'] as String;
    if (cmd == 'edit_select_file') {
      if (pickQueue.isEmpty) return {'cancelled': true};
      final id = pickQueue.removeAt(0);
      return id == null ? {'cancelled': true} : {'cancelled': false, 'selection_id': id};
    }
    if (cmd == 'edit_open' && req['selection_id'] != null) {
      final f = files.remove(req['selection_id']);
      if (f == null) throw StateError('파일 선택을 다시 확인해 주세요.');
      final le = f.text.contains('\r\n') ? 'CRLF' : (f.text.contains('\n') ? 'LF' : 'CRLF');
      final id = _next++;
      final d = docs[id] = _Doc(f.text.replaceAll('\r\n', '\n').replaceAll('\r', '\n'), encoding: f.encoding, lineEnding: le, name: f.name);
      return {'doc': id, 'line_ending': le, 'encoding': f.encoding, 'name': f.name, 'mixed_line_endings': false, ..._state(d, 0), 'dirty': false};
    }
    if (cmd == 'edit_open') {
      if (failOpen) throw StateError('klib를 시작할 수 없습니다.');
      final raw = req['text'] as String? ?? '';
      final text = raw.replaceAll('\r\n', '\n').replaceAll('\r', '\n');
      final id = _next++;
      final le = req['line_ending'] as String? ?? (raw.contains('\r\n') ? 'CRLF' : (raw.contains('\n') ? 'LF' : 'CRLF'));
      docs[id] = _Doc(text, lineEnding: le, encoding: req['encoding'] as String? ?? 'UTF-8');
      return {'doc': id, 'line_ending': le, 'encoding': docs[id]!.encoding, ..._state(docs[id]!, 0)};
    }
    final id = req['doc'] as int?;
    final d = docs[id];
    if (d == null) throw StateError('문서 $id를 찾을 수 없습니다.');
    final opts = FindOptions(caseSensitive: req['case_sensitive'] == true, wholeWord: req['whole_word'] == true);
    switch (cmd) {
      case 'edit_close':
        docs.remove(id);
        return {};
      case 'edit_info':
        return {..._state(d, 0), 'line_ending': d.lineEnding, 'encoding': d.encoding};
      case 'edit_line_range':
        final lines = d.text.split('\n');
        final start = (req['start_line'] as int).clamp(0, lines.length);
        final count = (req['count'] as int? ?? 50).clamp(0, 5000);
        final rows = <Map<String, Object?>>[];
        var byte = 0;
        for (var i = 0; i < lines.length && i < start + count; i++) {
          if (i >= start) rows.add({'line': i, 'start': byte, 'text': lines[i]});
          byte += utf8.encode(lines[i]).length + 1;
        }
        if (maxResponseBytes != null && utf8.encode(jsonEncode(rows)).length > maxResponseBytes!) {
          throw StateError('klib 응답이 허용 크기를 초과했습니다.');
        }
        return {'total_lines': lines.length, 'start_line': start, 'chars': d.text.runes.length, 'lines': rows};
      case 'edit_line_col':
        final u = _u16(d.text, req['pos'] as int);
        final before = d.text.substring(0, u);
        final nl = before.lastIndexOf('\n');
        return {'line': '\n'.allMatches(before).length + 1, 'col': before.substring(nl + 1).runes.length + 1};
      case 'edit_unencodable':
        final enc = req['encoding'] as String;
        final bad = <String, int>{};
        final first = <Map<String, Object?>>[];
        var line = 1;
        var col = 1;
        for (final r in d.text.runes) {
          if (r == 0x0A) {
            line++;
            col = 1;
            continue;
          }
          if (enc == 'CP949' && fakeUnencodableCp949(r)) {
            final ch = String.fromCharCode(r);
            bad[ch] = (bad[ch] ?? 0) + 1;
            final code = 'U+${r.toRadixString(16).toUpperCase().padLeft(4, '0')}';
            if (first.length < (req['limit'] as int? ?? 100)) first.add({'char': ch, 'code': code, 'line': line, 'col': col});
          }
          col++;
        }
        final total = bad.values.fold(0, (a, b) => a + b);
        return {
          'encoding': enc,
          'ok_to_save': total == 0,
          'total': total,
          'distinct_count': bad.length,
          'distinct': [
            for (final e in bad.entries)
              {'char': e.key, 'code': 'U+${e.key.runes.first.toRadixString(16).toUpperCase().padLeft(4, '0')}', 'count': e.value},
          ],
          'first': first,
        };
      case 'edit_save':
        final enc = req['encoding'] as String? ?? d.encoding;
        final le = req['line_ending'] as String? ?? d.lineEnding;
        if (enc == 'CP949' && d.text.runes.any(fakeUnencodableCp949)) throw StateError('CP949로 저장할 수 없는 글자가 있습니다.');
        final sid = req['selection_id'] as String;
        saved.add(FakeSaved(sid, enc, le, d.text.replaceAll('\n', le == 'LF' ? '\n' : '\r\n')));
        d
          ..encoding = enc
          ..lineEnding = le
          ..name = '$sid.txt';
        return {..._state(d, 0), 'line_ending': le, 'encoding': enc, 'dirty': false, 'name': '$sid.txt'};
      case 'edit_text':
        return {'text': d.text};
      case 'edit_lines':
        final lines = d.text.split('\n');
        final from = req['from'] as int;
        final to = (req['to'] as int).clamp(0, lines.length);
        return {
          'lines': [for (var i = from; i < to; i++) {'line': i, 'text': lines[i]}],
        };
      case 'edit_commit':
        d.grouping = false;
        return {};
      case 'edit_replace':
        final s = _u16(d.text, req['start'] as int);
        final e = _u16(d.text, req['end'] as int);
        final t = (req['text'] as String).replaceAll('\r\n', '\n');
        if (!d.grouping) _push(d);
        d.grouping = true;
        d.text = d.text.replaceRange(s, e, t);
        return _state(d, _b(d.text, s + t.length));
      case 'edit_undo' || 'edit_redo':
        final (from, to) = cmd == 'edit_undo' ? (d.undo, d.redo) : (d.redo, d.undo);
        d.grouping = false;
        if (from.isEmpty) return {..._state(d, 0), 'changed': false};
        to.add(d.text);
        final old = d.text;
        d.text = from.removeLast();
        var p = 0;
        while (p < old.length && p < d.text.length && old.codeUnitAt(p) == d.text.codeUnitAt(p)) {
          p++;
        }
        return {..._state(d, _b(d.text, p)), 'changed': true};
      case 'edit_find':
        final from = _u16(d.text, req['from'] as int);
        final all = findAllMatches(d.text, req['query'] as String, opts);
        final hit = all.where((m) => m.start >= from).firstOrNull ?? (req['wrap'] == true ? all.firstOrNull : null);
        if (hit == null) return {'found': false};
        return {'found': true, 'start': _b(d.text, hit.start), 'end': _b(d.text, hit.end)};
      case 'edit_find_all':
        final all = findAllMatches(d.text, req['query'] as String, opts, limit: req['limit'] as int? ?? 10000);
        return {
          'count': all.length,
          'matches': [for (final m in all) {'start': _b(d.text, m.start), 'end': _b(d.text, m.end)}],
        };
      case 'edit_replace_next':
        final from = _u16(d.text, req['from'] as int);
        final hit = findAllMatches(d.text, req['query'] as String, opts).where((m) => m.start >= from).firstOrNull;
        if (hit == null) return {'replaced': false};
        _push(d);
        final w = req['with'] as String;
        d.text = d.text.replaceRange(hit.start, hit.end, w);
        d.grouping = false;
        return {..._state(d, _b(d.text, hit.start + w.length)), 'replaced': true, 'start': _b(d.text, hit.start)};
      case 'edit_replace_all':
        final all = findAllMatches(d.text, req['query'] as String, opts, limit: 1 << 30);
        if (all.isNotEmpty) {
          _push(d);
          final w = req['with'] as String;
          for (final m in all.reversed) {
            d.text = d.text.replaceRange(m.start, m.end, w);
          }
        }
        d.grouping = false;
        return {..._state(d, 0), 'count': all.length};
      case 'edit_md_inline':
        final marker = switch (req['kind']) {
          'bold' => '**',
          'italic' => '*',
          _ => '`',
        };
        return _format(d, req, (v) => md.toggleInline(v, marker));
      case 'edit_md_block':
        return _format(d, req, (v) => switch (req['kind']) {
          'heading' => md.setBlockStyle(v, req['level'] as int),
          'paragraph' => md.setBlockStyle(v, 0),
          'bullet' => md.toggleLinePrefix(v, md.MemoListKind.bullet),
          'number' => md.toggleLinePrefix(v, md.MemoListKind.numbered),
          _ => md.toggleLinePrefix(v, md.MemoListKind.quote),
        });
      case 'edit_md_link':
        return _format(d, req, (v) => md.insertLink(v, req['url'] as String));
      case 'edit_md_rule':
        return _format(d, req, md.insertHorizontalRule);
      case 'edit_md_code_block':
        return _format(d, req, md.insertCodeBlock);
      case 'edit_md_clear':
        return _format(d, req, md.clearFormatting);
    }
    throw StateError('알 수 없는 명령: $cmd');
  }
}
