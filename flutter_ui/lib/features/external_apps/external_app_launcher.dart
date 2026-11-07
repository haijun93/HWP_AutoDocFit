import 'package:flutter/material.dart';

import '../../core/api/abbreviation_api.dart' show ApiCall;

/// 개발자 모드 외부 프로그램(사진첩·아웃라이너·사진관리) 공용 진입점.
///
/// 세 프로그램은 앱에 이식하지 않은 별도 프로그램이다. 화면은 실행만 요청하고
/// (실행 파일 찾기·검증·실행은 Python `external_app` 공용 실행기), 실행 파일이 없으면
/// 설치 안내·실행 파일 지정 창을 띄운다. 사이드메뉴 항목 키는 `external-app-<id>`.
/// 라이선스·배포 설계: docs/EXTERNAL_APPS.md.
class ExternalAppSpec {
  const ExternalAppSpec({required this.id, required this.label, required this.icon});

  /// Python `external_apps_service.APPS`의 app_id와 같아야 한다.
  final String id;
  final String label;
  final IconData icon;

  ValueKey<String> get menuKey => ValueKey('external-app-$id');
}

/// 사이드메뉴 표시 순서.
const kExternalApps = <ExternalAppSpec>[
  ExternalAppSpec(id: 'photo_album', label: '사진첩', icon: Icons.photo_library_outlined),
  ExternalAppSpec(id: 'outliner', label: '아웃라이너', icon: Icons.account_tree_outlined),
  ExternalAppSpec(id: 'photo_manager', label: '사진관리', icon: Icons.photo_camera_back_outlined),
];

Future<Map> _callExternalApp(ApiCall api, String action, ExternalAppSpec app, [Map<String, Object?> extra = const {}]) async {
  try {
    final raw = await api('external_app', [action, {'app': app.id, ...extra}]);
    return raw is Map ? raw : const {'ok': false};
  } catch (error) {
    return {'ok': false, 'message': '$error'};
  }
}

/// 사이드메뉴에서 외부 프로그램 실행. 실행창(오른쪽 화면)은 바꾸지 않는다.
Future<void> openExternalApp(BuildContext context, ApiCall api, ExternalAppSpec app) async {
  final result = await _callExternalApp(api, 'launch', app);
  if (!context.mounted) return;
  if (result['ok'] == true) {
    ScaffoldMessenger.maybeOf(context)?.showSnackBar(
      SnackBar(content: Text('${app.label}을(를) 별도 창으로 열었습니다.')),
    );
    return;
  }
  if (result['code'] == 'APP_MISSING') {
    await showDialog<void>(
      context: context,
      builder: (_) => ExternalAppSetupDialog(api: api, app: app, message: '${result['message'] ?? ''}'),
    );
    return;
  }
  ScaffoldMessenger.maybeOf(context)?.showSnackBar(
    SnackBar(content: Text('${result['message'] ?? '${app.label}을(를) 열지 못했습니다.'}')),
  );
}

/// 실행 파일이 없을 때: 설치 안내 + 포크 저장소·원본 공식 배포처 + 실행 파일 지정.
class ExternalAppSetupDialog extends StatefulWidget {
  const ExternalAppSetupDialog({super.key, required this.api, required this.app, required this.message});

  final ApiCall api;
  final ExternalAppSpec app;
  final String message;

  @override
  State<ExternalAppSetupDialog> createState() => _ExternalAppSetupDialogState();
}

class _ExternalAppSetupDialogState extends State<ExternalAppSetupDialog> {
  bool _busy = false;
  String? _notice;

  Future<Map> _call(String action, [Map<String, Object?> extra = const {}]) =>
      _callExternalApp(widget.api, action, widget.app, extra);

  Future<void> _choose() async {
    setState(() { _busy = true; _notice = null; });
    final chosen = await _call('choose');
    if (!mounted) return;
    if (chosen['ok'] == true && chosen['cancelled'] != true && chosen['available'] == true) {
      final launched = await _call('launch');
      if (!mounted) return;
      if (launched['ok'] == true) {
        Navigator.of(context).pop();
        return;
      }
      setState(() { _busy = false; _notice = '${launched['message'] ?? '${widget.app.label}을(를) 실행하지 못했습니다.'}'; });
      return;
    }
    setState(() {
      _busy = false;
      _notice = chosen['cancelled'] == true ? null : '${chosen['message'] ?? '실행 파일을 지정하지 못했습니다.'}';
    });
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    key: const ValueKey('external-app-setup'),
    icon: Icon(widget.app.icon),
    title: Text('${widget.app.label} 준비'),
    content: SizedBox(
      width: 460,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SelectableText(widget.message),
          if (_notice != null) ...[
            const SizedBox(height: 12),
            Text(_notice!, key: const ValueKey('external-app-notice'),
                style: TextStyle(color: Theme.of(context).colorScheme.error)),
          ],
        ],
      ),
    ),
    actions: [
      TextButton(
        key: const ValueKey('external-app-fork'),
        onPressed: _busy ? null : () => _call('open_download', {'target': 'fork'}),
        child: const Text('포크 저장소'),
      ),
      TextButton(
        key: const ValueKey('external-app-official'),
        onPressed: _busy ? null : () => _call('open_download', {'target': 'official'}),
        child: const Text('공식 배포처'),
      ),
      TextButton(
        key: const ValueKey('external-app-choose'),
        onPressed: _busy ? null : _choose,
        child: const Text('실행 파일 지정'),
      ),
      TextButton(
        onPressed: () => Navigator.of(context).pop(),
        child: const Text('닫기'),
      ),
    ],
  );
}
