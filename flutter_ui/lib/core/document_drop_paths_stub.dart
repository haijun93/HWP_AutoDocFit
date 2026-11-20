typedef DocumentDropPathsCallback = void Function(List<String> paths);

final _callbacks = <DocumentDropPathsCallback>[];

void Function() watchDocumentDropPaths(DocumentDropPathsCallback callback) {
  _callbacks.add(callback);
  return () => _callbacks.remove(callback);
}

void dispatchDocumentDropPaths(List<String> paths) {
  for (final callback in List<DocumentDropPathsCallback>.of(_callbacks)) {
    callback(paths);
  }
}
