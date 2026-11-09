import 'package:flutter/foundation.dart';

/// App UI type scale selected in Settings.
final uiFontScale = ValueNotifier<double>(1.0);

void setUiFontScale(String value) {
  uiFontScale.value = value == 'large' ? 15 / 13 : 1.0;
}
