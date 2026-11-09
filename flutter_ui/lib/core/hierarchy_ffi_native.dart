import 'dart:convert';
import 'dart:ffi' as ffi;
import 'dart:io';
import 'dart:typed_data';
import 'package:ffi/ffi.dart' as pkgffi;

typedef _AnalyzeNative = ffi.Pointer<ffi.Int8> Function(ffi.Pointer<ffi.Uint8>, ffi.Size);
typedef _AnalyzeDart = ffi.Pointer<ffi.Int8> Function(ffi.Pointer<ffi.Uint8>, int);
typedef _FreeNative = ffi.Void Function(ffi.Pointer<ffi.Int8>);
typedef _FreeDart = void Function(ffi.Pointer<ffi.Int8>);

/// Optional local Rust hierarchy engine. A null result means the caller should use its existing backend.
class HierarchyFfi {
  HierarchyFfi._();
  static final HierarchyFfi instance = HierarchyFfi._();
  _AnalyzeDart? _analyze;
  _FreeDart? _free;

  bool get available {
    if (_analyze != null) return true;
    if (!Platform.isWindows) return false;
    for (final path in [
      '${File(Platform.resolvedExecutable).parent.path}${Platform.pathSeparator}hierarchy.dll',
      '${Directory.current.path}${Platform.pathSeparator}resources${Platform.pathSeparator}hierarchy.dll',
    ]) {
      try {
        final library = ffi.DynamicLibrary.open(path);
        _analyze = library.lookupFunction<_AnalyzeNative, _AnalyzeDart>('hierarchy_analyze_json');
        _free = library.lookupFunction<_FreeNative, _FreeDart>('hierarchy_free_string');
        return true;
      } catch (_) {
        // Try the next packaged location, then gracefully use the backend.
      }
    }
    return false;
  }

  Map<String, Object?>? analyze(List<Map<String, Object?>> paragraphs) {
    if (!available) return null;
    final input = Uint8List.fromList(utf8.encode(jsonEncode({'paragraphs': paragraphs})));
    final memory = pkgffi.calloc<ffi.Uint8>(input.length);
    try {
      memory.asTypedList(input.length).setAll(0, input);
      final result = _analyze!(memory, input.length);
      if (result == ffi.nullptr) return null;
      try {
        final decoded = jsonDecode(pkgffi.Utf8Pointer(ffi.Pointer<pkgffi.Utf8>.fromAddress(result.address)).toDartString());
        if (decoded is Map<String, Object?> && decoded['error'] == null) return decoded;
        return null;
      } finally {
        _free!(result);
      }
    } catch (_) {
      return null;
    } finally {
      pkgffi.calloc.free(memory);
    }
  }
}
