// Windows 시스템 색(고대비용): 데스크톱(dart:ffi 있음)은 GetSysColor, 웹 빌드는 null.
export 'system_colors_stub.dart' if (dart.library.ffi) 'system_colors_native.dart';
export 'system_colors_model.dart';
