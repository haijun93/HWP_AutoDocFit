import 'package:flutter/foundation.dart';

import '../../core/drop_events.dart';
import 'tab_navigation_shell.dart';

/// Keep this exact event contract aligned with dock::tab_event_name.
String tabOpenEventName(int tab) {
  if (tab < 0 || tab >= kTabCount) throw RangeError.range(tab, 0, kTabCount - 1);
  return 'docfit-open-tab-$tab';
}

String tabCloseEventName(int tab) {
  if (tab < 0 || tab >= kTabCount) throw RangeError.range(tab, 0, kTabCount - 1);
  return 'docfit-close-tab-$tab';
}

/// Registers Dock requests and returns a disposer for all native/web listeners.
List<void Function()> watchTabOpenEvents({
  required ValueNotifier<int?> tabRequest,
  required Set<int> Function() hiddenTabs,
  required void Function() bringToFront,
  void Function() Function(String, void Function()) watch = watchWindowEvent,
}) => [
  for (var tab = 0; tab < kTabCount; tab++)
    watch(tabOpenEventName(tab), () {
      if (tab != 0 && (tab == 12 || tab == 13 || hiddenTabs().contains(tab))) return;
      tabRequest.value = null;
      tabRequest.value = tab;
      bringToFront();
    }),
];

/// A close request only collapses the tab currently selected in the host.
List<void Function()> watchTabCloseEvents({
  required ValueNotifier<int?> tabRequest,
  void Function() Function(String, void Function()) watch = watchWindowEvent,
}) => [
  for (var tab = 0; tab < kTabCount; tab++)
    watch(tabCloseEventName(tab), () {
      if (tabRequest.value == tab) tabRequest.value = null;
    }),
];
