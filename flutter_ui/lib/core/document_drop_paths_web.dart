import 'dart:js_interop';

import 'package:web/web.dart' as web;

typedef DocumentDropPathsCallback = void Function(List<String> paths);

void Function() watchDocumentDropPaths(DocumentDropPathsCallback callback) {
  final listener = ((web.Event event) {
    final detail = (event as web.CustomEvent).detail;
    if (detail == null || !detail.isA<JSArray<JSString>>()) return;
    callback(
      (detail as JSArray<JSString>).toDart.map((path) => path.toDart).toList(),
    );
  }).toJS;
  web.window.addEventListener('docfit-documents-dropped', listener);
  return () =>
      web.window.removeEventListener('docfit-documents-dropped', listener);
}
