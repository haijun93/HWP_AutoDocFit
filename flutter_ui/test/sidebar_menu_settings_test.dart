import 'package:flutter_test/flutter_test.dart';
import 'package:docfit_ui/features/workspace/sidebar_menu_settings.dart';
import 'package:docfit_ui/features/workspace/tab_navigation_shell.dart';

void main() {
  test('기존 저장 번호는 유지하고 새 아웃라이너 메뉴(14)와 검색 벤치마크(15)까지 읽는다', () {
    expect(parseHiddenTabs([3, 7, 0, 12, -1, '5', 3]), {3, 7});
    expect(parseHiddenTabs([11, 12, 13, 14, 15]), {11, 14, 15});
    expect(parseHiddenTabs(null), isEmpty);
    expect(parseHiddenTabs('3'), isEmpty);
  });

  test('필수 메뉴와 잘못된 값은 저장값 복원 시 무시한다', () {
    expect(parseHiddenTabs([0, 12, 13, -1, '2', true, 2.0, 2, 2]), {2});
    expect(parseHiddenTabs([0, 12, 13, -1, '2', true, 2.0]), isEmpty);
  });

  test('선택 창 목록은 문서편집을 빼고 나머지 메뉴를 한 번씩 담는다', () {
    final tabs = kSidebarMenuChoices.map((e) => e.tab).toList()..sort();
    expect(tabs, [
      for (var i = 1; i < kTabCount; i++)
        if (i != 12 && i != 13) i,
    ]);
    expect(kTabOutliner, 14);
    expect(kTabQmsBench, 15);
  });
}
