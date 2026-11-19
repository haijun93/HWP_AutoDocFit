import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:docfit_ui/features/workspace/dock_mode_controller.dart';
import 'package:docfit_ui/features/workspace/sidebar_menu_settings.dart';

class FakeDock implements DockRuntime {
  bool starts = false, stops = false, ready = true;
  @override
  Future<void> start() async {
    starts = true;
  }

  @override
  Future<bool> waitUntilReady(Duration timeout) async => ready;
  @override
  Future<void> stop() async {
    stops = true;
  }
}

void main() {
  testWidgets('메뉴 표시 방식 설정은 사이드바·Dock 선택을 보여 준다', (tester) async {
    final dock = FakeDock();
    final controller = DockModeController(
      runtime: dock,
      api: (method, args) async =>
          method == 'get_state' ? {'menu_display_mode': 'sidebar'} : null,
      showSidebar: () async {},
      hideSidebar: () async {},
    );
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: MenuDisplayModeSetting(
            api: (method, args) async =>
                method == 'get_state' ? {'menu_display_mode': 'sidebar'} : null,
            controller: controller,
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('메뉴 표시 방식'), findsOneWidget);
    expect(find.text('사이드바'), findsOneWidget);
    await tester.tap(find.byKey(const Key('menuDisplayModePicker')));
    await tester.pumpAndSettle();
    expect(find.text('하단 Dock'), findsOneWidget);
  });

  test('Dock은 ready 응답 뒤 사이드바를 숨기고 설정을 dock으로 저장한다', () async {
    final calls = <String>[];
    final dock = FakeDock();
    var visible = true;
    final controller = DockModeController(
      runtime: dock,
      api: (method, args) async {
        calls.add('${args.single}');
        return null;
      },
      showSidebar: () async {
        visible = true;
      },
      hideSidebar: () async {
        visible = false;
      },
    );
    expect(await controller.select('dock'), isNull);
    expect(calls, ['dock']);
    expect(dock.starts, isTrue);
    expect(visible, isFalse);
  });

  test('백엔드 DockRuntime은 시작·준비·종료 API를 호출한다', () async {
    final calls = <String>[];
    final runtime = ProcessDockRuntime((method, args) async {
      calls.add('$method:${args.join(',')}');
      return method == 'dock_ready';
    });
    await runtime.start();
    expect(await runtime.waitUntilReady(const Duration(seconds: 3)), isTrue);
    await runtime.stop();
    expect(calls, ['dock_start:', 'dock_ready:3000', 'dock_stop:']);
  });

  test('ready 누락은 sidebar 저장·표시로 복귀한다', () async {
    final calls = <String>[];
    final dock = FakeDock()..ready = false;
    var visible = false;
    final controller = DockModeController(
      runtime: dock,
      api: (method, args) async {
        calls.add('${args.single}');
        return null;
      },
      showSidebar: () async {
        visible = true;
      },
      hideSidebar: () async {
        visible = false;
      },
    );
    final error = await controller.select('dock');
    expect(error, contains('사이드바로 돌아왔어요'));
    expect(calls, ['dock', 'sidebar']);
    expect(visible, isTrue);
    expect(dock.stops, isTrue);
  });

  test('앱 시작 시 dock 실행 실패는 저장값을 복구한다', () async {
    final calls = <String>[];
    final dock = FakeDock()..ready = false;
    var visible = false;
    final controller = DockModeController(
      runtime: dock,
      api: (method, args) async {
        calls.add('${args.single}');
        return null;
      },
      showSidebar: () async {
        visible = true;
      },
      hideSidebar: () async {
        visible = false;
      },
    );
    expect(await controller.restore('dock'), contains('사이드바로 복구했어요'));
    expect(calls, ['sidebar']);
    expect(visible, isTrue);
  });
}
