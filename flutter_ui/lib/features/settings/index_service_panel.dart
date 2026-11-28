import 'dart:async';

import 'package:flutter/material.dart';

import '../../core/api/abbreviation_api.dart';
import 'settings_colors.dart';

/// 전체 드라이브 파일명 색인 서비스(`HwpAutoDocFit.Index`)를 켜고 끄는 설정 화면.
/// 켜면 관리자 승인(UAC)을 한 번 받아 서비스를 설치·시작하고, 퀵몽땅검색이 전체 드라이브 이름을 찾는다.
/// 켜지 않아도 바탕 화면·문서·다운로드는 사용자 모드 색인으로 검색된다.
class IndexServicePanel extends StatefulWidget {
  const IndexServicePanel({super.key, required this.api});
  final ApiCall api;

  @override
  State<IndexServicePanel> createState() => _IndexServicePanelState();
}

class _IndexServicePanelState extends State<IndexServicePanel> {
  Map status = const {};
  bool loading = true;
  String? error;
  Timer? _poll;

  @override
  void initState() {
    super.initState();
    command('status');
  }

  @override
  void dispose() {
    _poll?.cancel();
    super.dispose();
  }

  bool get _busy => status['busy'] != null;

  Future<void> command(String action) async {
    setState(() {
      loading = true;
      error = null;
    });
    try {
      final response = await widget.api('index_service', [action, null]);
      if (response is! Map) throw StateError('상태를 읽지 못했습니다.');
      if (response['ok'] == false && response['code'] != 'BUSY') {
        throw StateError('${response['message'] ?? '처리하지 못했습니다.'}');
      }
      if (mounted && response.containsKey('installed')) setState(() => status = response);
    } catch (e) {
      if (mounted) setState(() => error = '$e');
    } finally {
      if (mounted) setState(() => loading = false);
      _schedulePoll();
    }
  }

  /// 설치·제거는 관리자 승인과 시간이 걸려 백그라운드에서 돌므로 끝날 때까지 상태를 다시 읽는다.
  void _schedulePoll() {
    _poll?.cancel();
    if (!mounted || !_busy) return;
    _poll = Timer(const Duration(seconds: 2), () => command('status'));
  }

  Future<void> _confirmInstall() async {
    final ok = await showDialog<bool>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        key: const ValueKey('index-service-install-dialog'),
        title: const Text('전체 드라이브 색인을 켤까요?'),
        content: const Text(
          'Windows 서비스를 한 번 설치합니다. 관리자 승인 창이 뜨면 "예"를 눌러 주세요.\n'
          '켜면 컴퓨터를 켤 때마다 모든 드라이브의 파일·폴더 이름을 미리 색인해 퀵몽땅검색에서 바로 찾습니다. '
          '문서 내용이 아니라 이름만 색인하며, 언제든 끌 수 있어요.',
        ),
        actions: [
          TextButton(onPressed: () => Navigator.of(dialogContext).pop(false), child: const Text('취소')),
          FilledButton(
            key: const ValueKey('index-service-install-confirm'),
            onPressed: () => Navigator.of(dialogContext).pop(true),
            child: const Text('켜기'),
          ),
        ],
      ),
    );
    if (ok == true) await command('install');
  }

  Future<void> _confirmUninstall() async {
    final ok = await showDialog<bool>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: const Text('전체 드라이브 색인을 끌까요?'),
        content: const Text('서비스를 제거하고 저장된 이름 색인을 지웁니다. 바탕 화면·문서·다운로드는 계속 검색돼요. '
            '관리자 승인 창이 뜨면 "예"를 눌러 주세요.'),
        actions: [
          TextButton(onPressed: () => Navigator.of(dialogContext).pop(false), child: const Text('취소')),
          FilledButton(onPressed: () => Navigator.of(dialogContext).pop(true), child: const Text('끄기')),
        ],
      ),
    );
    if (ok == true) await command('uninstall');
  }

  String get _summary {
    if (status['busy'] == 'install') return '켜는 중…';
    if (status['busy'] == 'uninstall') return '끄는 중…';
    if (status['running'] == true) return '켜짐 · 전체 드라이브 이름 색인 중';
    if (status['installed'] == true) return '설치됨 · 멈춰 있음';
    return '꺼짐 · 바탕 화면·문서·다운로드만 검색';
  }

  @override
  Widget build(BuildContext context) {
    final colors = SettingsColors.of(context);
    final installed = status['installed'] == true;
    final message = '${status['message'] ?? ''}';
    return ListView(
      children: [
        const ListTile(
          title: Text('전체 드라이브 파일명 색인'),
          subtitle: Text(
            '퀵몽땅검색(Ctrl+Alt+Space)이 모든 드라이브의 파일·폴더 이름을 Everything처럼 바로 찾게 합니다.\n'
            '켜려면 관리자 승인이 한 번 필요해요. 끄면 바탕 화면·문서·다운로드만 검색됩니다.',
          ),
        ),
        if (loading || _busy) const LinearProgressIndicator(),
        if (error != null)
          ListTile(title: Text(error!, style: TextStyle(color: colors.errorText))),
        ListTile(
          key: const ValueKey('index-service-status'),
          title: const Text('상태'),
          subtitle: Text(_summary),
        ),
        if (message.isNotEmpty)
          ListTile(
            title: Text(
              message,
              style: TextStyle(color: status['last_ok'] == false ? colors.errorText : null),
            ),
          ),
        if (status['supported'] == false)
          const ListTile(title: Text('이 컴퓨터(Windows가 아님)에서는 쓸 수 없어요.'))
        else if (!installed)
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 4),
            child: Align(
              alignment: Alignment.centerLeft,
              child: FilledButton.icon(
                key: const ValueKey('index-service-install'),
                onPressed: (loading || _busy) ? null : _confirmInstall,
                icon: const Icon(Icons.shield_outlined),
                label: const Text('관리자 권한으로 켜기'),
              ),
            ),
          )
        else
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 4),
            child: Align(
              alignment: Alignment.centerLeft,
              child: OutlinedButton.icon(
                key: const ValueKey('index-service-uninstall'),
                onPressed: (loading || _busy) ? null : _confirmUninstall,
                icon: const Icon(Icons.power_settings_new),
                label: const Text('끄기(서비스 제거)'),
              ),
            ),
          ),
        TextButton(
          onPressed: (loading || _busy) ? null : () => command('status'),
          child: const Text('상태 새로고침'),
        ),
      ],
    );
  }
}
