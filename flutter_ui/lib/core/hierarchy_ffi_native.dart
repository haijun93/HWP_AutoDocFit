import 'dart:ffi' as ffi;
import 'dart:io';
import 'package:ffi/ffi.dart' as pkgffi;
import 'hierarchy_ffi_codec.dart';

typedef _AnalyzeNative = ffi.Pointer<pkgffi.Utf8> Function(ffi.Pointer<pkgffi.Utf8>);
typedef _AnalyzeDart = ffi.Pointer<pkgffi.Utf8> Function(ffi.Pointer<pkgffi.Utf8>);
typedef _FreeNative = ffi.Void Function(ffi.Pointer<pkgffi.Utf8>);
typedef _FreeDart = void Function(ffi.Pointer<pkgffi.Utf8>);

/// Optional local Rust hierarchy engine. A null result means the caller should use its existing backend.
class HierarchyFfi {
  HierarchyFfi._();
  static final HierarchyFfi instance = HierarchyFfi._();
  _AnalyzeDart? _analyze;
  _FreeDart? _free;
  bool _probed = false;

  /// 두 심볼을 모두 찾은 뒤에만 필드를 채운다. 하나라도 없으면 아무것도 바꾸지 않고 false.
  bool bind(ffi.DynamicLibrary library) {
    final _AnalyzeDart analyze;
    final _FreeDart free;
    try {
      analyze = library.lookupFunction<_AnalyzeNative, _AnalyzeDart>(hierarchyAnalyzeSymbol);
      free = library.lookupFunction<_FreeNative, _FreeDart>(hierarchyFreeSymbol);
    } catch (_) {
      return false;
    }
    _analyze = analyze;
    _free = free;
    return true;
  }

  /// 지정한 DLL을 열어 연결한다. 실패하면 상태를 바꾸지 않는다.
  bool load(String path) {
    try {
      return bind(ffi.DynamicLibrary.open(path));
    } catch (_) {
      return false;
    }
  }

  bool get available {
    if (_analyze != null && _free != null) return true;
    if (_probed || !Platform.isWindows) return false;
    _probed = true;
    for (final path in [
      '${File(Platform.resolvedExecutable).parent.path}${Platform.pathSeparator}hierarchy.dll',
      // 배포본(설치형·포터블)은 실행 파일 옆 resources 폴더에 DLL을 둔다. 작업 폴더와 무관하게 찾는다.
      '${File(Platform.resolvedExecutable).parent.path}${Platform.pathSeparator}resources${Platform.pathSeparator}hierarchy.dll',
      '${Directory.current.path}${Platform.pathSeparator}resources${Platform.pathSeparator}hierarchy.dll',
    ]) {
      // Try each packaged location, then gracefully use the backend.
      if (load(path)) return true;
    }
    return false;
  }

  Map<String, Object?>? analyze(List<Map<String, Object?>> paragraphs) {
    if (!available) return null;
    final analyze = _analyze!;
    final free = _free!;
    final ffi.Pointer<pkgffi.Utf8> input;
    try {
      // toNativeUtf8 은 NUL 종료 버퍼를 만든다(lib.rs 는 CStr 로 읽는다).
      input = encodeHierarchyRequest(paragraphs).toNativeUtf8();
    } catch (_) {
      return null;
    }
    try {
      final result = analyze(input);
      if (result == ffi.nullptr) return null;
      try {
        return decodeHierarchyResponse(result.toDartString(), paragraphCount: paragraphs.length);
      } catch (_) {
        return null;
      } finally {
        free(result);
      }
    } finally {
      pkgffi.malloc.free(input);
    }
  }
}
