import 'dart:convert';
import 'dart:ffi';
import 'dart:io';

import 'package:ffi/ffi.dart';
import 'package:flutter/foundation.dart' show visibleForTesting;

typedef _CallNative = Pointer<Utf8> Function(Pointer<Utf8>);
typedef _CallDart = Pointer<Utf8> Function(Pointer<Utf8>);
typedef _FreeNative = Void Function(Pointer<Utf8>);
typedef _FreeDart = void Function(Pointer<Utf8>);

/// K에이전트의 기존 저장소를 직접 읽는 선택 경로. 실패 시 호출자가 Python으로 대체한다.
class KClipFfi {
  KClipFfi._(this._call, this._free);
  @visibleForTesting
  KClipFfi.forTesting() : _call = _throwCall, _free = _ignoreFree;

  static Pointer<Utf8> _throwCall(Pointer<Utf8> _) => throw UnsupportedError('test adapter');
  static void _ignoreFree(Pointer<Utf8> _) {}

  final _CallDart _call;
  final _FreeDart _free;

  static KClipFfi? tryOpen({String? libraryPath}) {
    if (!Platform.isWindows) return null;
    try {
      final executable = File(Platform.resolvedExecutable).parent;
      final appRoot = Platform.environment['DOCFIT_APP_ROOT'];
      final candidates = [
        ?libraryPath,
        if (appRoot != null) '$appRoot\\resources\\kclip_ffi.dll',
        '${executable.path}\\kclip_ffi.dll',
        '${executable.path}\\resources\\kclip_ffi.dll',
        '${executable.path}\\_internal\\kclip_ffi.dll',
      ];
      for (final path in candidates) {
        try {
          final library = DynamicLibrary.open(path);
          return KClipFfi._(
            library.lookupFunction<_CallNative, _CallDart>('kclip_call_json'),
            library.lookupFunction<_FreeNative, _FreeDart>('kclip_free'),
          );
        } catch (_) {
          // 다음 패키징 위치를 확인한다.
        }
      }
      return null;
    } catch (_) {
      return null;
    }
  }

  Map<String, Object?> call(Map<String, Object?> request) {
    final input = jsonEncode(request).toNativeUtf8();
    try {
      final output = _call(input);
      if (output == nullptr) throw StateError('FFI 응답이 비었습니다.');
      try {
        final decoded = jsonDecode(output.toDartString());
        if (decoded is! Map<String, Object?>) throw const FormatException('FFI JSON 객체가 아닙니다.');
        return decoded;
      } finally {
        _free(output);
      }
    } finally {
      calloc.free(input);
    }
  }
}
