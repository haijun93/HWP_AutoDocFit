import 'dart:async';
import 'package:flutter/foundation.dart';

import '../../core/api/abbreviation_api.dart' show ApiCall;

final ValueNotifier<bool> sidebarModeVisible = ValueNotifier<bool>(true);

/// Dock 프로세스/명명 파이프 경계를 추상화해 전환 실패를 UI 없이 검증한다.
abstract interface class DockRuntime {
  Future<void> start();
  Future<bool> waitUntilReady(Duration timeout);
  Future<void> stop();
}

class DockModeController {
  DockModeController({
    required this.api,
    required this.runtime,
    required this.showSidebar,
    required this.hideSidebar,
  });
  final ApiCall api;
  final DockRuntime runtime;
  final Future<void> Function() showSidebar;
  final Future<void> Function() hideSidebar;

  Future<String?> select(String mode) async {
    if (mode == 'sidebar') {
      await showSidebar();
      try {
        await runtime.stop();
      } catch (_) {}
      await api('set_menu_display_mode', ['sidebar']);
      return null;
    }
    await api('set_menu_display_mode', ['dock']);
    try {
      await runtime.start();
      if (!await runtime.waitUntilReady(const Duration(seconds: 3))) {
        throw TimeoutException('Dock 준비 응답 시간 초과');
      }
      await hideSidebar();
      return null;
    } catch (error) {
      await api('set_menu_display_mode', ['sidebar']);
      await showSidebar();
      try {
        await runtime.stop();
      } catch (_) {}
      return 'Dock을 시작하지 못해 사이드바로 돌아왔어요. ($error)';
    }
  }

  /// 저장된 Dock 모드의 앱 시작 복구. 실패하면 사이드바를 즉시 표시하고 설정을 고친다.
  Future<String?> restore(String? savedMode) async {
    if (savedMode != 'dock') {
      await showSidebar();
      return null;
    }
    try {
      await runtime.start();
      if (!await runtime.waitUntilReady(const Duration(seconds: 3))) {
        throw TimeoutException('Dock 준비 응답 시간 초과');
      }
      await hideSidebar();
      return null;
    } catch (error) {
      await showSidebar();
      await api('set_menu_display_mode', ['sidebar']);
      try {
        await runtime.stop();
      } catch (_) {}
      return '저장된 Dock을 열지 못해 사이드바로 복구했어요. ($error)';
    }
  }
}

/// Dock 프로세스와 파이프는 Python 백엔드가 관리한다.
class ProcessDockRuntime implements DockRuntime {
  const ProcessDockRuntime(this.api);
  final ApiCall api;

  @override
  Future<void> start() async { await api('dock_start', []); }

  @override
  Future<bool> waitUntilReady(Duration timeout) async =>
      await api('dock_ready', [timeout.inMilliseconds]) == true;

  @override
  Future<void> stop() async { await api('dock_stop', []); }
}
