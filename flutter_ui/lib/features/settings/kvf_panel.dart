import 'package:flutter/material.dart';
import '../../core/api/abbreviation_api.dart';

class KvfPanel extends StatefulWidget {
  const KvfPanel({super.key, required this.api});
  final ApiCall api;
  @override
  State<KvfPanel> createState() => _KvfPanelState();
}

class _KvfPanelState extends State<KvfPanel> {
  Map status = const {};
  bool busy = true;
  String? error;
  @override
  void initState() {
    super.initState();
    command('status');
  }

  Future<void> command(String action, [Map<String, Object?>? payload]) async {
    setState(() {
      busy = true;
      error = null;
    });
    try {
      final response = await widget.api('kvf', [action, payload]);
      if (response is! Map || response['ok'] != true) {
        throw StateError(
          response is Map ? '${response['message']}' : '상태를 읽지 못했습니다.',
        );
      }
      if (mounted) setState(() => status = response);
    } catch (e) {
      if (mounted) setState(() => error = '$e');
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  @override
  Widget build(BuildContext context) => ListView(
    children: [
      const ListTile(
        title: Text('가상 폴더 창'),
        subtitle: Text(
          'Win+Shift+F로 자료함을 엽니다. 몽땅클립보드와 따로 켜고 끕니다. 기본은 꺼짐입니다.\n'
          '사본이 없는 항목은 앱에서 한 번 열어 주세요.',
        ),
      ),
      if (busy) const LinearProgressIndicator(),
      if (error != null)
        ListTile(
          title: Text(error!, style: const TextStyle(color: Colors.red)),
        ),
      SwitchListTile(
        title: const Text('가상 폴더 창 켜기'),
        subtitle: Text(status['running'] == true ? '켜짐' : '꺼짐'),
        value: status['running'] == true,
        onChanged: busy
            ? null
            : (value) => command(value ? 'enable' : 'disable'),
      ),
      SwitchListTile(
        title: const Text('Windows 로그인 때 자동 시작'),
        value: status['autostart'] == true,
        onChanged: busy || status['running'] != true
            ? null
            : (value) => command('enable', {'autostart': value}),
      ),
      if (status['status'] is Map &&
          (status['status'] as Map)['error'] != null &&
          (status['status'] as Map)['error'] != '')
        ListTile(title: Text('상태 안내: ${(status['status'] as Map)['error']}')),
      TextButton(
        onPressed: busy ? null : () => command('status'),
        child: const Text('상태 새로고침'),
      ),
    ],
  );
}
