import 'dart:ffi' as ffi;
import 'dart:io';

import 'package:flutter/painting.dart';

import 'system_colors_model.dart';

typedef _GetSysColorNative = ffi.Uint32 Function(ffi.Int32);
typedef _GetSysColorDart = int Function(int);

/// Windows GetSysColor로 고대비 색을 읽는다. Windows가 아니거나 실패하면 null.
SystemColors? readSystemColors() {
  if (!Platform.isWindows) return null;
  try {
    final get = ffi.DynamicLibrary.open('user32.dll')
        .lookupFunction<_GetSysColorNative, _GetSysColorDart>('GetSysColor');
    Color c(int index) => SystemColors.fromColorRef(get(index));
    return SystemColors(
      window: c(5), // COLOR_WINDOW
      windowText: c(8), // COLOR_WINDOWTEXT
      highlight: c(13), // COLOR_HIGHLIGHT
      highlightText: c(14), // COLOR_HIGHLIGHTTEXT
      buttonFace: c(15), // COLOR_BTNFACE
      grayText: c(17), // COLOR_GRAYTEXT
      hotlight: c(26), // COLOR_HOTLIGHT
    );
  } catch (_) {
    return null;
  }
}
