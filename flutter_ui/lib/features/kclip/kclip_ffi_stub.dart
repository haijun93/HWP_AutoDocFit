// 웹(dart2js)에는 dart:ffi가 없으므로 몽땅클립보드 FFI를 쓰지 않는다(항상 기존 Python 중계 경로).
import 'package:flutter/foundation.dart';

class KClipFfi {
  factory KClipFfi() => KClipFfi._();
  KClipFfi._();
  @visibleForTesting
  KClipFfi.forTesting();

  static KClipFfi? tryOpen({String? libraryPath}) => null;

  Map<String, Object?> call(Map<String, Object?> request) =>
      throw UnsupportedError('KClipFfi는 웹에서 지원하지 않습니다.');
}
