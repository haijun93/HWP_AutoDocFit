// 위계 엔진 FFI: 데스크톱(dart:ffi 있음)은 네이티브 DLL, 웹 빌드는 빈 대체 구현.
export 'hierarchy_ffi_stub.dart' if (dart.library.ffi) 'hierarchy_ffi_native.dart';
