import 'package:flutter/material.dart';

import '../../core/theme/app_theme.dart';
import 'quick_bar_store.dart';

/// 오른쪽 빠른 실행 막대(참고 화면 2026-11-19의 오른쪽 막대 방식, SIDEBAR2).
/// 창이 이 폭보다 좁으면 본문을 우선해 막대를 접는다(업무 화면은 편집 영역이 넓어야 한다).
const kQuickBarWidth = 56.0;
const kQuickBarMinWindowWidth = 1100.0;

/// 사용자가 막대를 보이게 했는지. 저장은 이 PC의 웹 저장소(quick_bar_store)에 한다.
final ValueNotifier<bool> quickBarVisibleNotifier = ValueNotifier<bool>(readQuickBarVisible());

void setQuickBarVisible(bool value) {
  quickBarVisibleNotifier.value = value;
  writeQuickBarVisible(value);
}

class QuickAction {
  const QuickAction({
    required this.id,
    required this.label,
    required this.icon,
    required this.onPressed,
    this.tooltip,
    this.badge = 0,
  });

  final String id;
  final String label;
  final IconData icon;
  final void Function(BuildContext context) onPressed;

  /// 풍선 도움말(없으면 label). 단축키 안내를 덧붙일 때 쓴다.
  final String? tooltip;

  /// 오른쪽 위 알림 숫자. 0이면 숨김, 99 초과는 '99+'.
  final int badge;
}

class QuickActionBar extends StatelessWidget {
  const QuickActionBar({super.key, required this.actions});
  final List<QuickAction> actions;

  @override
  Widget build(BuildContext context) {
    return Material(
      color: context.colors.surface,
      child: Container(
        width: kQuickBarWidth,
        decoration: BoxDecoration(border: Border(left: BorderSide(color: context.colors.line))),
        child: Column(
          children: [
            SizedBox(
              height: 40,
              child: IconButton(
                key: const ValueKey('quickBarHide'),
                tooltip: '빠른 실행 막대 숨기기',
                visualDensity: VisualDensity.compact,
                onPressed: () => setQuickBarVisible(false),
                icon: const Icon(Icons.chevron_right, size: 20),
              ),
            ),
            Expanded(
              child: ListView(
                padding: const EdgeInsets.symmetric(vertical: 4),
                children: [for (final a in actions) _QuickItem(action: a)],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _QuickItem extends StatelessWidget {
  const _QuickItem({required this.action});
  final QuickAction action;

  @override
  Widget build(BuildContext context) {
    final color = Theme.of(context).colorScheme.primary;
    final badgeText = action.badge > 99 ? '99+' : '${action.badge}';
    return Semantics(
      button: true,
      label: action.badge > 0 ? '${action.label}, 알림 $badgeText' : action.label,
      excludeSemantics: true,
      child: Tooltip(
        message: action.tooltip ?? action.label,
        child: InkWell(
          key: ValueKey('quickAction-${action.id}'),
          borderRadius: BorderRadius.circular(12),
          onTap: () => action.onPressed(context),
          child: Padding(
            padding: const EdgeInsets.symmetric(vertical: 6, horizontal: 2),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Stack(
                  clipBehavior: Clip.none,
                  children: [
                    Container(
                      width: 36,
                      height: 36,
                      decoration: BoxDecoration(
                        color: color.withValues(alpha: .10),
                        shape: BoxShape.circle,
                      ),
                      child: Icon(action.icon, size: 20, color: color),
                    ),
                    if (action.badge > 0)
                      Positioned(
                        right: -6,
                        top: -4,
                        child: Container(
                          key: ValueKey('quickBadge-${action.id}'),
                          padding: const EdgeInsets.symmetric(horizontal: 4),
                          constraints: const BoxConstraints(minWidth: 16, minHeight: 16),
                          decoration: BoxDecoration(color: Colors.red.shade700, borderRadius: BorderRadius.circular(8)),
                          alignment: Alignment.center,
                          child: Text(badgeText, style: const TextStyle(fontSize: 10, color: Colors.white, fontWeight: FontWeight.w700)),
                        ),
                      ),
                  ],
                ),
                const SizedBox(height: 2),
                Text(action.label, maxLines: 1, overflow: TextOverflow.ellipsis, textAlign: TextAlign.center,
                    style: TextStyle(fontSize: 10, color: context.colors.textSecondary)),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// 막대를 숨겼을 때(또는 접혀 있을 때) 오른쪽 가장자리에 남는 다시 보이기 손잡이.
class QuickBarRevealHandle extends StatelessWidget {
  const QuickBarRevealHandle({super.key});

  @override
  Widget build(BuildContext context) => Align(
        alignment: Alignment.topRight,
        child: Padding(
          padding: const EdgeInsets.only(top: 4),
          child: Material(
            color: context.colors.surface,
            shape: RoundedRectangleBorder(
              borderRadius: const BorderRadius.horizontal(left: Radius.circular(8)),
              side: BorderSide(color: context.colors.line),
            ),
            child: InkWell(
              key: const ValueKey('quickBarShow'),
              onTap: () => setQuickBarVisible(true),
              child: Tooltip(
                message: '빠른 실행 막대 보이기',
                child: const SizedBox(width: 20, height: 36, child: Icon(Icons.chevron_left, size: 18)),
              ),
            ),
          ),
        ),
      );
}
