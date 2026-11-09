import 'package:flutter/material.dart';

import '../../core/api/abbreviation_api.dart' show ApiCall;
import '../../core/codicons.dart';

/// 개발자 모드 외부 프로그램(사진첩·아웃라이너·사진관리·PDF편집) 진입점.
class ExternalAppSpec {
  const ExternalAppSpec({required this.id, required this.label, required this.icon});

  final String id;
  final String label;
  final IconData icon;
  ValueKey<String> get menuKey => ValueKey('external-app-$id');
}

const kExternalApps = <ExternalAppSpec>[
  ExternalAppSpec(id: 'photo_album', label: '사진첩', icon: Codicons.fileMedia),
  ExternalAppSpec(id: 'outliner', label: '아웃라이너', icon: Codicons.listTree),
  ExternalAppSpec(id: 'photo_manager', label: '사진관리', icon: Codicons.folderOpened),
  ExternalAppSpec(id: 'pdf_editor', label: 'PDF편집', icon: Codicons.filePdf),
];

Future<Map> _callExternalApp(ApiCall api, String action, ExternalAppSpec app,
    [Map<String, Object?> extra = const {}]) async {
  try {
    final raw = await api('external_app', [action, {'app': app.id, ...extra}]);
    return raw is Map ? raw : const {'ok': false};
  } catch (error) {
    return {'ok': false, 'message': '$error'};
  }
}

Future<void> openExternalApp(BuildContext context, ApiCall api, ExternalAppSpec app) async {
  final result = await _callExternalApp(api, 'launch', app);
  if (!context.mounted) return;
  if (result['ok'] == true) {
    ScaffoldMessenger.maybeOf(context)?.showSnackBar(
      SnackBar(content: Text('${app.label}을(를) 별도 창으로 열었습니다.')),
    );
    return;
  }
  if (result['code'] != 'APP_MISSING') {
    ScaffoldMessenger.maybeOf(context)?.showSnackBar(
      SnackBar(content: Text('${result['message'] ?? '${app.label}을(를) 열지 못했습니다.'}')),
    );
    return;
  }
  final metadata = await _callExternalApp(api, 'latest', app);
  if (!context.mounted) return;
  await showDialog<void>(
    context: context,
    builder: (_) => ExternalAppSetupDialog(api: api, app: app, message: '${result['message'] ?? ''}', metadata: metadata),
  );
}

/// 앱 업데이트 패널과 같은 카드/진행률 구성으로 설치 또는 업데이트한다.
class ExternalAppSetupDialog extends StatefulWidget {
  const ExternalAppSetupDialog({super.key, required this.api, required this.app,
    required this.message, this.metadata = const {}});

  final ApiCall api;
  final ExternalAppSpec app;
  final String message;
  final Map metadata;

  @override
  State<ExternalAppSetupDialog> createState() => _ExternalAppSetupDialogState();
}

class _ExternalAppSetupDialogState extends State<ExternalAppSetupDialog> {
  Map? _install;
  bool _busy = false;
  String? _notice;

  Future<Map> _call(String action) => _callExternalApp(widget.api, action, widget.app);

  Future<void> _start() async {
    setState(() { _busy = true; _notice = null; });
    final result = await _call('install');
    if (!mounted) return;
    setState(() {
      _busy = false;
      _install = result['install'] is Map ? Map.from(result['install'] as Map) :
          (result['skipped'] == true ? {'phase': 'done', 'running': false, 'ok': true} : null);
      if (result['ok'] != true) _notice = '${result['message'] ?? '설치를 시작하지 못했습니다.'}';
    });
    if (_install?['running'] == true) {
      await _poll();
    } else if (_install?['ok'] == true) {
      await _launch();
    }
  }

  Future<void> _poll() async {
    while (mounted && _install?['running'] == true) {
      await Future<void>.delayed(const Duration(milliseconds: 500));
      if (!mounted) return;
      final result = await _call('progress');
      if (!mounted) return;
      setState(() => _install = result['install'] is Map ? Map.from(result['install'] as Map) : _install);
    }
    if (mounted && _install?['ok'] == true) await _launch();
  }

  Future<void> _launch() async {
    final result = await _call('launch');
    if (!mounted) return;
    if (result['ok'] == true) {
      Navigator.of(context).pop();
    } else {
      setState(() => _notice = '${result['message'] ?? '${widget.app.label}을(를) 설치했지만 실행하지 못했습니다.'}');
    }
  }

  Future<void> _cancel() async {
    await _call('cancel');
    if (!mounted) return;
    setState(() { _install = {'phase': 'cancelled', 'running': false, 'ok': false}; _notice = '설치를 취소했습니다.'; });
  }

  @override
  Widget build(BuildContext context) {
    final info = widget.metadata;
    final state = _install ?? const {};
    final phase = '${state['phase'] ?? 'idle'}';
    final running = state['running'] == true;
    final failed = phase == 'error' || phase == 'cancelled' || _notice != null;
    final percent = (state['percent'] as num?)?.toInt();
    final version = '${info['version'] ?? '확인 중'}';
    final size = info['size'] is num ? '${(((info['size'] as num) / (1024 * 1024))).toStringAsFixed(1)} MB' : '배포처에서 제공';
    return AlertDialog(
      key: const ValueKey('external-app-setup'),
      icon: Icon(widget.app.icon),
      title: Text('${widget.app.label} ${info['update_available'] == true ? '업데이트' : '설치'}'),
      content: SizedBox(width: 460, child: Column(mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start, children: [
        Text('버전  $version'), const SizedBox(height: 4), Text('크기  $size'),
        const SizedBox(height: 4), Text('출처  ${info['source'] ?? '공식 배포처'}'),
        const SizedBox(height: 4), Text('라이선스  ${info['license'] ?? '오픈소스'}'),
        if (widget.message.isNotEmpty) ...[const SizedBox(height: 8), Text(widget.message, style: const TextStyle(fontSize: 12))],
        if (running) ...[
          const SizedBox(height: 14),
          LinearProgressIndicator(key: const ValueKey('external-app-progress'),
            value: percent == null ? null : (percent / 100).clamp(0.0, 1.0)),
          const SizedBox(height: 6),
          Text('${state['message'] ?? '준비 중…'}${percent == null ? '' : ' ($percent%)'}', key: const ValueKey('external-app-progress-text')),
        ],
        if (_notice != null) ...[const SizedBox(height: 10), Text(_notice!, key: const ValueKey('external-app-notice'),
          style: TextStyle(color: Theme.of(context).colorScheme.error))],
      ])),
      actions: [
        if (running)
          TextButton(key: const ValueKey('external-app-cancel'), onPressed: _cancel, child: const Text('취소')),
        if (!running && !failed && phase != 'done')
          FilledButton(key: const ValueKey('external-app-install'),
            onPressed: _busy ? null : _start,
            child: Text(info['update_available'] == true ? '업데이트' : '설치')),
        if (failed) FilledButton(key: const ValueKey('external-app-retry'), onPressed: _busy ? null : _start, child: const Text('다시 시도')),
        if (!running) TextButton(onPressed: () => Navigator.of(context).pop(), child: const Text('닫기')),
      ],
    );
  }
}
