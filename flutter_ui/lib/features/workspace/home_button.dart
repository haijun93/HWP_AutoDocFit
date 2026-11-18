import 'package:flutter/material.dart';

import '../../core/theme/app_theme.dart';

/// 앱의 첫 화면(문서편집 탭, index 0)으로 즉시 전환하는 공통 홈 버튼 위젯.
///
/// 모든 메뉴(탭 화면)의 상단 왼쪽 모서리에 고정된 위치로 제공되며,
/// 탭마다 상단 구조가 달라 겹치거나 밀리지 않도록 셸(TabNavigationShell) 수준의
/// 공통 슬롯(오버레이)에 배치된다.
///
/// [enabled]: 첫 화면(문서편집 탭, index 0) 자체에서는 이미 홈에 머물러 있으므로
/// 불필요한 재전환 동작을 방지하고 사용자가 현재 위치가 첫 화면임을 직관적으로 인지할 수 있도록
/// 비활성화(disabled, onPressed: null)한다. 위젯 트리에서 완전히 제거하지 않고
/// 비활성 상태로 슬롯 자리를 유지함으로써 탭 전환 시 화면 요소의 위치 흔들림을 방지하고
/// 모든 탭에서 일관된 모서리 슬롯 위치를 제공한다.
///
/// [visible]: 첫 화면 자체에서는 이미 홈에 위치하여 불필요하며, 첫 화면 상단의 도구
/// (서식 관리 버튼 등)와의 시각적 겹침 및 터치 간섭을 방지하기 위해 숨김(hidden) 처리한다.
/// 숨김 처리 시에도 [maintainSize]를 활성화하여 위젯 트리 및 크기 점유(Offset/Size)를
/// 유지하므로 모든 탭에서 동일한 모서리 슬롯 위치가 안정적으로 보존된다.
class HomeButton extends StatelessWidget {
  const HomeButton({
    super.key,
    required this.onPressed,
    this.enabled = true,
    this.visible = true,
    this.tooltip = '첫 화면',
    this.focusNode,
    this.autofocus = false,
  });

  /// 홈(첫 화면)으로 이동하는 콜백.
  final VoidCallback? onPressed;

  /// 버튼 활성화 여부. 첫 화면에서는 false로 전달하여 비활성화된다.
  final bool enabled;

  /// 버튼 표시 여부. false이면 화면에서 숨겨지되 공간 점유(maintainSize)는 유지된다.
  final bool visible;

  /// 마우스 오버 시 표시되는 툴팁 (기본값: '첫 화면').
  final String tooltip;

  /// 키보드 접근성을 위한 FocusNode.
  final FocusNode? focusNode;

  /// 위젯 마운트 시 자동 포커스 여부.
  final bool autofocus;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final appColors = context.colors;

    final effectiveOnPressed = enabled ? onPressed : null;

    final button = Semantics(
      button: true,
      enabled: enabled,
      label: tooltip,
      child: IconButton(
        key: const ValueKey('workspace-home-button'),
        focusNode: focusNode,
        autofocus: autofocus,
        tooltip: enabled ? tooltip : null,
        onPressed: effectiveOnPressed,
        icon: const Icon(Icons.home_outlined, size: 20),
        style: IconButton.styleFrom(
          foregroundColor: appColors.accent,
          disabledForegroundColor: theme.disabledColor.withValues(alpha: 0.38),
          backgroundColor: appColors.surface.withValues(alpha: 0.92),
          disabledBackgroundColor: appColors.surface.withValues(alpha: 0.5),
          side: BorderSide(
            color: appColors.line.withValues(alpha: enabled ? 0.8 : 0.3),
            width: 1,
          ),
          padding: EdgeInsets.zero,
          minimumSize: const Size(36, 36),
          maximumSize: const Size(36, 36),
          shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(8)),
        ),
      ),
    );

    if (!visible) {
      return Visibility(
        visible: false,
        maintainSize: true,
        maintainAnimation: true,
        maintainState: true,
        child: button,
      );
    }

    return button;
  }
}
