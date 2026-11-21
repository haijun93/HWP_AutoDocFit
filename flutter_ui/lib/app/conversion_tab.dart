import 'package:flutter/material.dart';

import '../features/conversion/conversion_workspace.dart';

class ConversionTab extends StatefulWidget {
  const ConversionTab({super.key, required this.host});
  final ConversionHost? Function() host;
  @override
  State<ConversionTab> createState() => _ConversionTabState();
}

class _ConversionTabState extends State<ConversionTab> {
  bool _native = false, _loaded = false, _loading = false, _busy = false;
  String _message = '';
  Future<Map> _call(ConversionHost host, String action, [String? value]) async {
    final payload = <String, Object?>{'action': action};
    if (value != null) payload['value'] = value;
    final raw = await host.apiCall('conversion_window', [payload]);
    if (raw is! Map || raw['ok'] != true) {
      final error = raw is Map ? raw['error'] : null;
      throw StateError(
        error is Map ? '${error['message'] ?? '요청 실패'}' : '요청 실패',
      );
    }
    return raw['data'] is Map
        ? Map<String, dynamic>.from(raw['data'] as Map)
        : {};
  }

  Future<void> _load(ConversionHost host) async {
    if (_loaded || _loading) return;
    _loading = true;
    try {
      final data = await _call(host, 'ui_mode');
      if (!mounted) return;
      _loaded = true;
      setState(() => _native = data['mode'] == 'native');
      if (_native) await _open(host);
    } catch (_) {
      _loaded = true; // 브리지 사용 불가 시 기존 Flutter 화면을 유지한다.
    } finally {
      _loading = false;
    }
  }

  Future<void> _open(ConversionHost host) async {
    setState(() {
      _busy = true;
      _message = '';
    });
    try {
      await _call(host, 'open_native');
      if (mounted) setState(() => _message = 'Win32 변환 창을 열었습니다.');
    } catch (e) {
      if (mounted) setState(() => _message = '$e');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _setMode(ConversionHost host, String mode) async {
    try {
      await _call(host, 'ui_mode', mode);
      if (!mounted) return;
      setState(() => _native = mode == 'native');
      if (mode == 'native') await _open(host);
    } catch (e) {
      if (mounted) setState(() => _message = '$e');
    }
  }

  @override
  Widget build(BuildContext context) {
    final host = widget.host();
    if (host == null) return const Center(child: CircularProgressIndicator());
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted && !_loaded && !_loading) _load(host);
    });
    if (_native)
      return Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 460),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              const Icon(Icons.open_in_new, size: 40),
              const SizedBox(height: 12),
              const Text('몽땅파일변환은 별도 Win32 창에서 열립니다.'),
              if (_message.isNotEmpty) Text(_message),
              const SizedBox(height: 12),
              Wrap(
                spacing: 8,
                children: [
                  FilledButton(
                    onPressed: _busy ? null : () => _open(host),
                    child: const Text('창 다시 열기'),
                  ),
                  OutlinedButton(
                    onPressed: _busy ? null : () => _setMode(host, 'flutter'),
                    child: const Text('앱 안에서 변환'),
                  ),
                ],
              ),
            ],
          ),
        ),
      );
    return Column(
      children: [
        Align(
          alignment: Alignment.centerRight,
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 8),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                if (_message.isNotEmpty) Text(_message),
                TextButton.icon(
                  key: const ValueKey('conversion-open-native'),
                  onPressed: _busy ? null : () => _setMode(host, 'native'),
                  icon: const Icon(Icons.open_in_new, size: 16),
                  label: const Text('새 창(Win32)으로 열기'),
                ),
              ],
            ),
          ),
        ),
        Expanded(child: ConversionWorkspace(host: host)),
      ],
    );
  }
}
