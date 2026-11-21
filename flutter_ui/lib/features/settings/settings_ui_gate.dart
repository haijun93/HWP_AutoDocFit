import 'package:flutter/material.dart';

import '../../core/api/abbreviation_api.dart' show ApiCall;

/// 앱의 [설정] 진입이 부르는 문지기 결과.
class NativeSettingsResult {
  const NativeSettingsResult({required this.opened, this.message = ''});

  /// true 면 Win32 설정 창을 열었다 → 앱 안 설정 서랍은 열지 않는다.
  final bool opened;

  /// 창을 열지 못했을 때의 이유(있으면 안내 후 기존 화면으로 연다).
  final String message;
}

Map _envelope(Object? raw) => raw is Map ? raw : const {};

String _errorMessage(Map envelope) {
  final error = envelope['error'];
  return error is Map ? '${error['message'] ?? '요청 실패'}' : '요청 실패';
}

/// 설정 키 `settings_ui` 가 'native' 일 때만 `settings_win.exe` 를 연다(기본 'flutter' 는 아무 일도 하지 않는다).
/// 설정을 읽지 못하는 등 어떤 실패도 기존 화면으로 이어지게 예외를 던지지 않는다.
Future<NativeSettingsResult> openNativeSettingsIfEnabled(ApiCall api) async {
  try {
    final envelope = _envelope(await api('settings_window', [
      {'action': 'open_if_native'},
    ]));
    if (envelope['ok'] != true) {
      return NativeSettingsResult(opened: false, message: _errorMessage(envelope));
    }
    final data = envelope['data'];
    return NativeSettingsResult(opened: data is Map && data['opened'] == true);
  } catch (_) {
    return const NativeSettingsResult(opened: false);
  }
}

/// 설정 서랍의 "설정 창 방식" 줄: 앱 안(기본) ↔ 새 창(Win32) 전환.
/// 새 창으로 바꿀 때는 창이 실제로 열릴 때만 설정을 바꾼다. 읽기·쓰기·창 실행은 `settings_window` 브리지 명령이 맡는다.
class SettingsUiModeTile extends StatefulWidget {
  const SettingsUiModeTile({super.key, required this.api});
  final ApiCall api;

  @override
  State<SettingsUiModeTile> createState() => _SettingsUiModeTileState();
}

class _SettingsUiModeTileState extends State<SettingsUiModeTile> {
  String? _mode; // null = 읽는 중
  String _message = '';
  bool _busy = false;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<Map> _call(Map payload) async {
    final envelope = _envelope(await widget.api('settings_window', [payload]));
    if (envelope['ok'] != true) throw StateError(_errorMessage(envelope));
    return envelope['data'] is Map ? Map.from(envelope['data'] as Map) : {};
  }

  Future<void> _load() async {
    var mode = 'flutter';
    try {
      final data = await _call({'action': 'ui_mode'});
      if (data['mode'] == 'native') mode = 'native';
    } catch (_) {
      // 읽지 못하면 기본(앱 안)으로 둔다.
    }
    if (mounted) setState(() => _mode = mode);
  }

  Future<void> _toNative() async {
    setState(() {
      _busy = true;
      _message = '';
    });
    try {
      await _call({'action': 'open_native'});
      await _call({'action': 'ui_mode', 'value': 'native'});
      if (mounted) {
        setState(() {
          _mode = 'native';
          _message = '새 설정 창을 열었어요. 앞으로 [설정]을 누르면 새 창이 열려요.';
        });
      }
    } catch (e) {
      if (mounted) setState(() => _message = '$e');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _toFlutter() async {
    setState(() {
      _busy = true;
      _message = '';
    });
    try {
      await _call({'action': 'ui_mode', 'value': 'flutter'});
      if (mounted) {
        setState(() {
          _mode = 'flutter';
          _message = '앞으로 [설정]을 누르면 앱 안 설정이 열려요.';
        });
      }
    } catch (e) {
      if (mounted) setState(() => _message = '$e');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final mode = _mode;
    if (mode == null) return const SizedBox.shrink();
    final native = mode == 'native';
    return ListTile(
      key: const ValueKey('settings-ui-mode'),
      contentPadding: EdgeInsets.zero,
      leading: const Icon(Icons.open_in_new),
      title: const Text('설정 창 방식'),
      subtitle: Text(
        _message.isNotEmpty
            ? _message
            : native
                ? '지금은 새 설정 창(Win32)을 써요. 이 서랍은 설정 단추를 오른쪽 클릭하면 열려요.'
                : '지금은 앱 안 설정을 써요. 새 설정 창(Win32)으로 바꿀 수 있어요.',
        key: const ValueKey('settings-ui-mode-text'),
        style: const TextStyle(fontSize: 12),
      ),
      trailing: TextButton(
        key: const ValueKey('settings-ui-mode-toggle'),
        onPressed: _busy ? null : (native ? _toFlutter : _toNative),
        child: Text(native ? '앱 안에서 보기' : '새 창으로 열기'),
      ),
    );
  }
}
