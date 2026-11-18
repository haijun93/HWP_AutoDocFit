import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:docfit_ui/features/workspace/tab_open_events.dart';

void main() {
  testWidgets('Dock tab event selects the requested tab and focuses the app', (tester) async {
    final request = ValueNotifier<int?>(null);
    final listeners = <String, void Function()>{};
    var focused = 0;
    final stops = watchTabOpenEvents(
      tabRequest: request,
      hiddenTabs: () => {3},
      bringToFront: () => focused++,
      watch: (name, callback) {
        listeners[name] = callback;
        return () => listeners.remove(name);
      },
    );
    await tester.pumpWidget(MaterialApp(
      home: ValueListenableBuilder<int?>(
        valueListenable: request,
        builder: (_, tab, _) => Text('tab:${tab ?? "none"}'),
      ),
    ));

    expect(tabOpenEventName(3), 'docfit-open-tab-3');
    listeners['docfit-open-tab-4']!();
    await tester.pump();
    expect(find.text('tab:4'), findsOneWidget);
    expect(focused, 1);

    listeners['docfit-open-tab-3']!();
    await tester.pump();
    expect(find.text('tab:4'), findsOneWidget);
    expect(focused, 1);
    for (final stop in stops) {
      stop();
    }
    request.dispose();
  });
}
