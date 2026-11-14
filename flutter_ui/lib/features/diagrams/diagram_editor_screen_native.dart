import '../../core/close_guards.dart';
import '../flows/edit_save_state.dart';
import '../flows/flow_draft.dart';
import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:math';

import 'package:file_selector/file_selector.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:webview_flutter_windows/webview_flutter_windows.dart';

import '../../core/api/abbreviation_api.dart';
import '../../core/drop_events.dart';
import '../../core/theme/app_theme.dart';
import '../workspace/tab_navigation_shell.dart';

/// 편집기(diagram_editor) 화면 틀 테마 이름: 고대비면 hc, 아니면 밝기에 따라 dark·light.
String _editorThemeOf(BuildContext context) {
  if (AppColors.of(context).highContrast) return 'hc';
  return Theme.of(context).brightness == Brightness.dark ? 'dark' : 'light';
}

class DiagramEditorScreen extends StatefulWidget {
  const DiagramEditorScreen({
    super.key,
    required this.api,
    this.initialMode = 'diagram',
    this.isModal = true,
    this.photoPasteTrigger,
  });
  final ApiCall api;
  final String initialMode;
  final bool isModal;
  final ValueListenable<int>? photoPasteTrigger;

  @override
  State<DiagramEditorScreen> createState() => _DiagramEditorScreenState();
}

class _DiagramEditorScreenState extends State<DiagramEditorScreen> {
  final controller = WebviewController();
  final subscriptions = <StreamSubscription<dynamic>>[];
  HttpServer? server;
  Set<String> assets = {};
  bool initialized = false;
  final saveState = EditSaveState();
  late final draftClient = FlowDraftClient(widget.api, widget.initialMode);
  bool dirty = false;
  bool busy = true;
  bool _pendingPaste = false;
  void Function()? _stopPhotoPasteEvent;
  String? diagramId, revision;
  String name = '새 업무 흐름도';
  String? failure;
  String? _theme;
  Color _background = AppColors.light.canvas;

  CloseLease? _closeLease;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final theme = _editorThemeOf(context);
    _background = AppColors.of(context).canvas;
    if (theme == _theme) return;
    final first = _theme == null;
    _theme = theme;
    if (!first && controller.value.isInitialized) {
      unawaited(_applyTheme(theme));
    }
  }

  /// 앱 테마가 바뀌면 이미 열린 편집기에도 알린다(페이지를 다시 불러오지 않음).
  Future<void> _applyTheme(String theme) async {
    try {
      await controller.setBackgroundColor(_background);
      await controller.executeScript(
        'window.docfitSetTheme&&window.docfitSetTheme(${jsonEncode(theme)})',
      );
    } catch (_) {
      // 편집기가 아직 준비되지 않았으면 다음 테마 변경 때 다시 맞춘다.
    }
  }
  Future<bool> _prepareHostClose() async {
    if (saveState.saving || busy || !canCloseEditor(context)) return false;
    if (!await confirmDiscard() || !mounted) return false;
    closeWorkspaceScreen(context);
    return true;
  }

  @override
  void initState() {
    super.initState();
    if (widget.isModal) {
      _closeLease = CloseGuards.register(_prepareHostClose);
    }
    widget.photoPasteTrigger?.addListener(_onPhotoPasteTrigger);
    _stopPhotoPasteEvent = watchWindowEvent(
      'docfit-open-photo-edit',
      () => unawaited(pasteFromClipboard()),
    );
    initialize();
  }

  void _onPhotoPasteTrigger() {
    unawaited(pasteFromClipboard());
  }

  Future<void> initialize() async {
    try {
      await storage('list');
      final manifest = await rootBundle.loadString(
        'assets/flow_editor/asset-manifest.json',
      );
      final decoded = jsonDecode(manifest) as Map;
      assets = {'index.html'};
      for (final entry in decoded.values) {
        if (entry is Map) {
          for (final key in [
            'file',
            ...((entry['css'] as List?) ?? const []),
          ]) {
            if (key is String && !key.startsWith('/')) assets.add(key);
          }
          for (final key in (entry['assets'] as List?) ?? const []) {
            if (key is String && !key.startsWith('/')) assets.add(key);
          }
        }
      }
      server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
      server!.listen(
        serveAsset,
        onError: (Object error) {
          if (mounted) setState(() => failure = '편집기 리소스를 열지 못했어요.');
        },
      );
      await controller.initialize();
      await controller.setPopupWindowPolicy(WebviewPopupWindowPolicy.deny);
      await controller.setBackgroundColor(_background);
      subscriptions.add(controller.webMessage.listen(handleMessage));
      final theme = Uri.encodeQueryComponent(_theme ?? 'light');
      await controller.loadUrl(
        'http://127.0.0.1:${server!.port}/index.html?theme=$theme',
      );
      if (mounted) {
        setState(() {
          initialized = true;
          busy = false;
        });
      }
    } catch (error) {
      if (mounted) {
        setState(() {
          failure = '$error';
          busy = false;
        });
      }
    }
  }

  Future<void> serveAsset(HttpRequest request) async {
    final raw = Uri.decodeComponent(
      request.uri.path.replaceFirst(RegExp(r'^/'), ''),
    );
    if (raw.isEmpty ||
        raw.contains('..') ||
        raw.contains('\\') ||
        raw.contains('\x00') ||
        !assets.contains(raw)) {
      request.response.statusCode = HttpStatus.notFound;
      await request.response.close();
      return;
    }
    try {
      final data = await rootBundle.load(
        'assets/flow_editor/$raw',
      );
      final ext = raw.split('.').last.toLowerCase();
      request.response.headers
        ..set(HttpHeaders.contentTypeHeader, switch (ext) {
          'html' => 'text/html; charset=utf-8',
          'js' => 'text/javascript; charset=utf-8',
          'css' => 'text/css; charset=utf-8',
          'json' => 'application/json; charset=utf-8',
          'svg' => 'image/svg+xml',
          'woff2' => 'font/woff2',
          'png' => 'image/png',
          _ => 'application/octet-stream',
        })
        ..set('X-Content-Type-Options', 'nosniff')
        ..set(
          'Content-Security-Policy',
          "default-src 'self' data: blob:; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' data:; connect-src 'none'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'",
        );
      request.response.add(
        data.buffer.asUint8List(data.offsetInBytes, data.lengthInBytes),
      );
      await request.response.close();
    } catch (_) {
      request.response.statusCode = HttpStatus.notFound;
      await request.response.close();
    }
  }

  Future<Object?> storage(
    String action, [
    Map<String, Object?> payload = const {},
  ]) {
    return widget.api('flow_editor', [action, payload]);
  }

  void post(Map<String, Object?> message) {
    if (!controller.value.isInitialized) return;
    controller.postWebMessage(jsonEncode(message));
  }

  Future<void> handleMessage(dynamic event) async {
    dynamic message = event;
    if (message is String) {
      try {
        message = jsonDecode(message);
      } catch (_) {
        return;
      }
    }
    if (message is! Map) return;
    final type = message['type'];
    if (saveState.saving && type != 'dirty' && type != 'editor_error') return;
    try {
      switch (type) {
        case 'ready':
          // 불러오는 사이 테마가 바뀌었을 수 있으니 준비되면 한 번 더 맞춘다.
          unawaited(_applyTheme(_theme ?? 'light'));
          post({'type': 'set_mode', 'mode': widget.initialMode});
          if (diagramId != null) {
            final loaded = await storage('load', {'id': diagramId});
            if (loaded is! Map) throw Exception('저장된 도형 문서 응답이 올바르지 않습니다.');
            name = loaded['name'] as String;
            revision = loaded['revision'] as String;
            post({
              'type': 'load',
              'id': diagramId,
              'name': name,
              'snapshot': loaded['snapshot'],
            });
          }
          unawaited(offerRecovery());
          if (_pendingPaste) {
            _pendingPaste = false;
            unawaited(pasteFromClipboard());
          }
          break;
        case 'draft':
          await draftClient.save(message, id: diagramId);
          break;
        case 'dirty':
          if (mounted) {
            setState(() {
              saveState.event(message['generation'], message['value'] == true);
              dirty = saveState.dirty;
            });
          }
          break;
        case 'template_applied':
          if (mounted) {
            setState(() {
              name = '${message['name'] ?? '업무 흐름도'}';
              dirty = true;
            });
          }
          break;
        case 'request_new':
          if (await confirmDiscard()) {
            diagramId = null;
            revision = null;
            name = '새 업무 흐름도';
            dirty = false;
            await draftClient.clear();
            post({'type': 'new'});
            if (mounted) setState(() {});
          }
          break;
        case 'request_template':
          if (await confirmDiscard()) {
            post({'type': 'apply_template', 'template': message['template']});
          }
          break;
        case 'request_save':
          await saveDocument();
          break;
        case 'save_document':
          final id = diagramId ?? _newId();
          final targetGeneration = saveState.begin(message['generation']);
          if (mounted) setState(() {});
          Object? saved;
          try {
            saved = await storage('save', {
              'id': id,
              'name': message['name'],
              'snapshot': message['snapshot'],
              'revision': revision,
            });
            if (saved is! Map ||
                saved["id"] is! String ||
                saved["name"] is! String ||
                saved['revision'] is! String) {
              throw Exception("도형 저장 응답이 올바르지 않습니다.");
            }
          } catch (_) {
            saveState.complete(targetGeneration, success: false);
            rethrow;
          }
          saveState.complete(targetGeneration, success: true);
          diagramId = saved['id'] as String;
          revision = saved['revision'] as String;
          name = saved['name'] as String;
          post({'type': 'mark_saved', 'generation': targetGeneration});
          if (!saveState.dirty) await draftClient.clear();
          if (mounted) setState(() => dirty = saveState.dirty);
          if (mounted) {
            ScaffoldMessenger.of(context).showSnackBar(
              const SnackBar(content: Text('도형 문서를 이 PC에 저장했어요.')),
            );
          }
          break;
        case 'request_list':
          await showSavedDocuments();
          break;
        case 'request_png':
          post({'type': 'export_png'});
          break;
        case 'export_png':
          await savePng(message['base64'] as String? ?? '');
          break;
        case 'editor_error':
          if (mounted) {
            ScaffoldMessenger.of(context).showSnackBar(
              SnackBar(content: Text('${message['message'] ?? '편집기 오류'}')),
            );
          }
          break;
      }
    } catch (error) {
      if (mounted) {
        setState(() {});
        ScaffoldMessenger.of(
          context,
        ).showSnackBar(SnackBar(content: Text('도형 작업을 완료하지 못했어요: $error')));
      }
    }
  }

  Future<void> pasteFromClipboard() async {
    if (!initialized || busy) {
      _pendingPaste = true;
      return;
    }
    try {
      final result = await widget.api('table_sketch', ['clipboard_image']);
      if (result is! Map || result['ok'] != true) {
        if (mounted) {
          final msg = (result is Map ? result['message'] : null) ?? '클립보드에서 사진을 읽지 못했습니다.';
          ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text('$msg')));
        }
        return;
      }
      final kind = result['kind'] == 'png' ? 'png' : 'jpeg';
      final base64Data = result['data'];
      post({
        'type': 'set_background',
        'src': 'data:image/$kind;base64,$base64Data',
        'width': result['width'],
        'height': result['height'],
      });
      if (mounted) {
        setState(() => dirty = true);
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(content: Text('클립보드 사진을 붙임사진으로 넣었습니다.')),
        );
      }
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('클립보드 사진을 넣지 못했습니다: $e')),
        );
      }
    }
  }

  String _newId() => List<int>.generate(
    16,
    (_) => Random.secure().nextInt(256),
  ).map((value) => value.toRadixString(16).padLeft(2, '0')).join();

  Future<void> offerRecovery() async {
    // 저장된 문서를 먼저 열어야 하는 경우(diagramId 있음)에는 복구를 묻지 않는다.
    if (diagramId != null) return;
    final draft = await draftClient.pending();
    if (draft == null || !mounted) return;
    // 묻지 않고 바로 복구한다(사용자 요청 2026-11-07: 기본값으로 "복구"가 시행되게). 알림도 띄우지 않는다(사용자 요청 2026-11-08).
    final (id, rev) = await draftClient.reconnect(draft);
    diagramId = id;
    revision = rev;
    name = '${draft['name']}';
    if (mounted) setState(() => dirty = true);
    post(FlowDraftClient.restoreMessage(draft, id));
  }

  Future<bool> confirmDiscard() async {
    if (!dirty) return true;
    return await showDialog<bool>(
          context: context,
          builder: (context) => AlertDialog(
            title: const Text('저장하지 않은 변경사항'),
            content: const Text('현재 다이어그램의 변경사항을 버릴까요? 저장하려면 먼저 저장을 선택하세요.'),
            actions: [
              TextButton(
                onPressed: () => Navigator.pop(context, false),
                child: const Text('계속 편집'),
              ),
              FilledButton(
                onPressed: () => Navigator.pop(context, true),
                child: const Text('변경사항 버리기'),
              ),
            ],
          ),
        ) ??
        false;
  }

  Future<void> saveDocument({bool asCopy = false}) async {
    final input = TextEditingController(text: name);
    final selected = await showDialog<String>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('도형 문서 저장'),
        content: TextField(
          controller: input,
          autofocus: true,
          maxLength: 120,
          decoration: const InputDecoration(labelText: '문서 이름'),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('취소'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, input.text.trim()),
            child: const Text('저장'),
          ),
        ],
      ),
    );
    input.dispose();
    if (selected == null || selected.isEmpty) return;
    if (asCopy) {
      diagramId = null;
      revision = null;
    }
    name = selected;
    post({'type': 'save', 'id': diagramId, 'name': name});
  }

  Future<void> showSavedDocuments() async {
    final items = await storage('list') as List;
    if (!mounted) return;
    final selected = await showDialog<Map>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('저장된 다이어그램'),
        content: SizedBox(
          width: 520,
          height: 360,
          child: items.isEmpty
              ? const Center(child: Text('저장된 문서가 없습니다.'))
              : ListView(
                  children: [
                    for (final raw in items)
                      ListTile(
                        title: Text('${raw['name']}'),
                        subtitle: Text('${raw['updated_at']}'),
                        trailing: IconButton(
                          tooltip: '삭제',
                          icon: const Icon(Icons.delete_outline),
                          onPressed: () => Navigator.pop(context, {
                            'delete': raw['id'],
                            'name': raw['name'],
                          }),
                        ),
                        onTap: () => Navigator.pop(
                          context,
                          Map<String, dynamic>.from(raw),
                        ),
                      ),
                  ],
                ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('닫기'),
          ),
        ],
      ),
    );
    if (selected == null) return;
    if (selected.containsKey('delete')) {
      if (!mounted) return;
      final approved = await showDialog<bool>(
        context: context,
        builder: (context) => AlertDialog(
          title: const Text('흐름도 삭제'),
          content: Text('${selected['name']}을 삭제할까요?'),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context, false),
              child: const Text('취소'),
            ),
            FilledButton(
              onPressed: () => Navigator.pop(context, true),
              child: const Text('삭제'),
            ),
          ],
        ),
      );
      if (approved == true) {
        await storage('delete', {'id': selected['delete']});
        if (diagramId == selected['delete']) {
          diagramId = null;
          revision = null;
          if (mounted) setState(() => dirty = true);
        }
      }
      return;
    }
    if (!await confirmDiscard()) return;
    final loaded = await storage('load', {'id': selected['id']});
    if (loaded is! Map) throw Exception('저장된 도형 문서 응답이 올바르지 않습니다.');
    diagramId = selected['id'] as String;
    revision = loaded['revision'] as String;
    name = loaded['name'] as String;
    await draftClient.clear();
    post({
      'type': 'load',
      'id': diagramId,
      'name': name,
      'snapshot': loaded['snapshot'],
    });
    if (mounted) setState(() => dirty = false);
  }

  Future<void> savePng(String encoded) async {
    if (encoded.isEmpty || encoded.length > 28 * 1024 * 1024) {
      throw Exception('PNG 크기가 허용 한도를 넘었습니다.');
    }
    final bytes = base64Decode(encoded);
    if (bytes.length < 8 ||
        bytes.length > 20 * 1024 * 1024 ||
        base64Encode(bytes) != encoded ||
        bytes.take(8).join(',') !=
            const [137, 80, 78, 71, 13, 10, 26, 10].join(',')) {
      throw Exception('PNG 데이터가 올바르지 않습니다.');
    }
    final location = await getSaveLocation(
      suggestedName:
          '${name.replaceAll(RegExp(r'[^가-힣A-Za-z0-9 _-]'), '_')}.png',
      acceptedTypeGroups: const [
        XTypeGroup(label: 'PNG 이미지', extensions: ['png']),
      ],
    );
    if (location == null) return;
    final target = File(location.path);
    final temporary = File('${location.path}.tmp');
    final backup = File('${location.path}.bak');
    await temporary.writeAsBytes(bytes, flush: true);
    var backedUp = false;
    try {
      if (await target.exists()) {
        await target.rename(backup.path);
        backedUp = true;
      }
      await temporary.rename(target.path);
      if (backedUp) await backup.delete();
    } catch (_) {
      if (backedUp && !await target.exists() && await backup.exists()) {
        await backup.rename(target.path);
      }
      rethrow;
    } finally {
      if (await temporary.exists()) await temporary.delete();
    }
    if (mounted) {
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text('PNG를 저장했어요: ${target.path}')));
    }
  }

  @override
  Widget build(BuildContext context) => PopScope(
    canPop: !dirty && !saveState.saving,
    onPopInvokedWithResult: (didPop, result) async {
      if (!didPop &&
          !saveState.saving &&
          await confirmDiscard() &&
          context.mounted) {
        closeWorkspaceScreen(context);
      }
    },
    child: Scaffold(
      appBar: AppBar(
        title: Text(name),
        actions: [
          if (widget.initialMode == 'paint')
            IconButton(
              tooltip: '클립보드 사진 붙여넣기',
              icon: const Icon(Icons.content_paste_go),
              onPressed: initialized && !busy ? pasteFromClipboard : null,
            ),
          IconButton(
            tooltip: '사본으로 저장',
            icon: const Icon(Icons.copy),
            onPressed: initialized && !busy && !saveState.saving
                ? () => saveDocument(asCopy: true)
                : null,
          ),
        ],
        // 탭 안(돌아갈 화면 없음)에서는 ← 버튼을 보이지 않는다. 새 화면으로 열었을 때만 표시.
        automaticallyImplyLeading: false,
        leading: !Navigator.of(context).canPop()
            ? null
            : IconButton(
          icon: const Icon(Icons.arrow_back),
          onPressed: () async {
            if (!saveState.saving &&
                await confirmDiscard() &&
                context.mounted) {
              closeWorkspaceScreen(context);
            }
          },
        ),
      ),
      body: failure != null
          ? Center(
              child: Padding(
                padding: const EdgeInsets.all(24),
                child: Text('편집기를 시작하지 못했어요.\n$failure'),
              ),
            )
          : Stack(
              children: [
                if (initialized) Webview(controller),
                if (busy) const Center(child: CircularProgressIndicator()),
              ],
            ),
    ),
  );

  @override
  void dispose() {
    _stopPhotoPasteEvent?.call();
    widget.photoPasteTrigger?.removeListener(_onPhotoPasteTrigger);
    _closeLease?.release();
    for (final subscription in subscriptions) {
      unawaited(subscription.cancel());
    }
    unawaited(controller.dispose());
    if (server != null) unawaited(server!.close(force: true));
    super.dispose();
  }
}
