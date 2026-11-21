import 'package:flutter/material.dart';

import '../../core/api/abbreviation_api.dart' show ApiCall;
import 'file_rename_screen.dart';

/// `rename_ui`: flutter(기본)는 기존 화면, native는 읽기 전용 Win32 뼈대를 연다.
class RenameUiGate extends StatefulWidget {
  const RenameUiGate({super.key, required this.api, this.flutterBuilder});
  final ApiCall api;
  final Widget Function(ApiCall api)? flutterBuilder;
  @override
  State<RenameUiGate> createState() => _RenameUiGateState();
}

class _RenameUiGateState extends State<RenameUiGate> {
  String _mode = 'flutter';
  String _message = '';
  bool _modeLoaded = false;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    // 모든 탭이 영구 마운트되므로 선택된 탭에서만 설정을 읽는다.
    if (TickerMode.valuesOf(context).enabled && !_modeLoaded) {
      _modeLoaded = true;
      _loadMode();
    }
  }

  Future<void> _loadMode() async {
    try {
      final mode = (await _call({'action': 'ui_mode'}))['mode'];
      if (!mounted) return;
      setState(() => _mode = mode == 'native' ? 'native' : 'flutter');
      if (_mode == 'native') await _open();
    } catch (_) {
      // 설정 읽기 실패 시에는 기존 Flutter 화면을 유지한다.
    }
  }

  Future<Map> _call(Map payload) async {
    final raw = await widget.api('rename_window', [payload]);
    final result = raw is Map ? raw : const {};
    if (result['ok'] != true) throw StateError('${(result['error'] as Map?)?['message'] ?? '요청 실패'}');
    return result['data'] is Map ? Map.from(result['data'] as Map) : {};
  }

  Future<void> _open() async {
    try {
      await _call({'action': 'open_native'});
      if (mounted) setState(() => _message = '미리보기 창을 열었습니다.');
    } catch (e) {
      if (mounted) setState(() { _mode = 'flutter'; _message = '$e'; });
    }
  }

  Future<void> _setMode(String mode) async {
    try {
      await _call({'action': 'ui_mode', 'value': mode});
      if (mounted) setState(() { _mode = mode; _message = ''; });
      if (mode == 'native') await _open();
    } catch (e) {
      if (mounted) setState(() => _message = '$e');
    }
  }

  Future<void> _switchToNative() async {
    try {
      await _call({'action': 'open_native'});
      await _call({'action': 'ui_mode', 'value': 'native'});
      if (mounted) setState(() { _mode = 'native'; _message = ''; });
    } catch (e) {
      if (mounted) setState(() => _message = '$e');
    }
  }

  @override
  Widget build(BuildContext context) {
    final mode = _mode;
    if (mode == 'native') {
      return Center(child: Column(mainAxisSize: MainAxisSize.min, children: [
        const Text('파일명 미리보기는 별도 Win32 창에서 열립니다.'),
        if (_message.isNotEmpty) Text(_message),
        Wrap(spacing: 8, children: [
          OutlinedButton(onPressed: _open, child: const Text('창 다시 열기')),
          TextButton(onPressed: () => _setMode('flutter'), child: const Text('기존 화면 사용')),
        ]),
      ]));
    }
    return Column(children: [
      Align(alignment: Alignment.centerRight, child: TextButton.icon(
        key: const ValueKey('rename-open-native'), onPressed: _switchToNative,
        icon: const Icon(Icons.open_in_new, size: 16), label: Text(_message.isEmpty ? 'Win32 미리보기 창' : _message),
      )),
      Expanded(child: widget.flutterBuilder?.call(widget.api) ?? FileRenameScreen(api: widget.api)),
    ]);
  }
}
