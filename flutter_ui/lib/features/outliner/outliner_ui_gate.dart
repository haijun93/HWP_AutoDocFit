import 'package:flutter/material.dart';

import '../../core/api/abbreviation_api.dart' show ApiCall;
import 'outliner_workspace.dart';

/// 아웃라이너 탭의 화면 방식 문지기(설정 키 `outliner_ui`: 'flutter' 기본 | 'native').
///
/// - 'flutter': 기존 [OutlinerWorkspace] 를 보여 준다(제목 줄 끝에 "새 창(Win32)으로 열기" 아이콘 단추).
/// - 'native': Rust+Win32 창(outliner_win.exe)을 열고, 탭에는 안내만 보여 준다.
/// 설정 읽기·쓰기·창 실행은 모두 기존 `outliner` 브리지 명령(`ui_mode`·`open_native`)이 맡는다.
class OutlinerUiGate extends StatefulWidget {
  const OutlinerUiGate({
    super.key,
    required this.api,
    this.workspaceBuilder,
    this.recentPagesStore,
    this.panelStore,
  });
  final ApiCall api;

  /// 시험용: 파일 대신 쓸 저장소. 없으면 아웃라이너 데이터 폴더의 파일.
  final RecentPagesStore? recentPagesStore;
  final OutlinerPanelStore? panelStore;

  /// 시험용: 기존 화면 대신 보여 줄 위젯.
  final Widget Function(ApiCall api)? workspaceBuilder;

  @override
  State<OutlinerUiGate> createState() => _OutlinerUiGateState();
}

class _OutlinerUiGateState extends State<OutlinerUiGate> {
  String? _mode; // null = 읽는 중
  String _message = '';
  bool _busy = false;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<Map> _call(Map payload) async {
    final raw = await widget.api('outliner', [payload]);
    final envelope = raw is Map ? raw : const {};
    if (envelope['ok'] != true) {
      final error = envelope['error'];
      throw StateError(error is Map ? '${error['message'] ?? '요청 실패'}' : '요청 실패');
    }
    return envelope['data'] is Map ? Map.from(envelope['data'] as Map) : {};
  }

  Future<void> _load() async {
    var mode = 'flutter';
    try {
      final data = await _call({'action': 'ui_mode'});
      if (data['mode'] == 'native') mode = 'native';
    } catch (_) {
      // 설정을 읽지 못하면 기존 화면으로 둔다.
    }
    if (!mounted) return;
    setState(() => _mode = mode);
    if (mode == 'native') await _openNative();
  }

  /// Win32 창을 연다. 실패하면 이유를 안내만 한다(설정은 바꾸지 않는다).
  Future<bool> _openNative() async {
    setState(() {
      _busy = true;
      _message = '';
    });
    try {
      await _call({'action': 'open_native'});
      if (mounted) setState(() => _message = '아웃라이너 창을 열었습니다.');
      return true;
    } catch (e) {
      if (mounted) setState(() => _message = '$e');
      return false;
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _setMode(String value) async {
    try {
      await _call({'action': 'ui_mode', 'value': value});
      if (!mounted) return;
      setState(() => _mode = value);
    } catch (e) {
      if (mounted) setState(() => _message = '$e');
    }
  }

  /// 앱 안 화면에서 "새 창으로 열기": 창이 열릴 때만 설정을 native 로 바꾼다.
  Future<void> _switchToNative() async {
    if (await _openNative()) await _setMode('native');
  }

  @override
  Widget build(BuildContext context) {
    final mode = _mode;
    if (mode == null) return const SizedBox.expand();
    if (mode == 'native') return _nativeNotice(context);
    final openButton = keepEditorFocus(
      IconButton(
        key: const ValueKey('outliner-open-native'),
        tooltip: '새 창(Win32)으로 열기',
        onPressed: _busy ? null : _switchToNative,
        icon: const Icon(Icons.open_in_new),
      ),
    );
    final builder = widget.workspaceBuilder;
    if (builder == null) {
      // 단추를 제목 줄에 넣어 한 줄을 아낀다.
      return OutlinerWorkspace(
        api: widget.api,
        recentPagesStore: widget.recentPagesStore,
        panelStore:
            widget.panelStore ?? FileOutlinerPanelStore(outlinerDataDirectory),
        headerActions: [openButton],
        notice: _message,
      );
    }
    return Column(
      children: [
        Align(alignment: Alignment.centerRight, child: openButton),
        if (_message.isNotEmpty) Text(_message),
        Expanded(child: builder(widget.api)),
      ],
    );
  }

  Widget _nativeNotice(BuildContext context) {
    return Center(
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 460),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const Icon(Icons.open_in_new, size: 40),
            const SizedBox(height: 12),
            const Text(
              '아웃라이너는 별도 창(Win32)에서 열립니다.',
              key: ValueKey('outliner-native-notice'),
              style: TextStyle(fontSize: 16, fontWeight: FontWeight.w600),
            ),
            const SizedBox(height: 6),
            const Text('앱 안 화면으로 돌아가려면 아래 단추를 누르세요.', textAlign: TextAlign.center),
            if (_message.isNotEmpty) ...[
              const SizedBox(height: 10),
              Text(_message, key: const ValueKey('outliner-native-message')),
            ],
            const SizedBox(height: 16),
            Wrap(
              spacing: 8,
              children: [
                FilledButton(
                  key: const ValueKey('outliner-reopen-native'),
                  onPressed: _busy ? null : _openNative,
                  child: const Text('창 다시 열기'),
                ),
                OutlinedButton(
                  key: const ValueKey('outliner-use-flutter'),
                  onPressed: _busy ? null : () => _setMode('flutter'),
                  child: const Text('앱 안에서 보기'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
