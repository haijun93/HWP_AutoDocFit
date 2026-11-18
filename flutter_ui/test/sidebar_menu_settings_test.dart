import 'package:flutter_test/flutter_test.dart';
import 'package:docfit_ui/features/workspace/sidebar_menu_settings.dart';
import 'package:docfit_ui/features/workspace/tab_navigation_shell.dart';

void main() {
  test('저장값은 1~11 정수만 남기고 문서편집(0)은 끌 수 없다', () {
    expect(parseHiddenTabs([3, 7, 0, 12, -1, '5', 3]), {3, 7});
    expect(parseHiddenTabs(null), isEmpty);
    expect(parseHiddenTabs('3'), isEmpty);
  });

  test('선택 창 목록은 문서편집을 빼고 나머지 메뉴를 한 번씩 담는다', () {
    final tabs = kSidebarMenuChoices.map((e) => e.tab).toList()..sort();
    expect(tabs, [for (var i = 1; i < kTabCount; i++) i]);
  });
}
