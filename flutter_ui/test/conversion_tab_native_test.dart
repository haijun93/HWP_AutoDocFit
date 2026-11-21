import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:docfit_ui/app/conversion_tab.dart';
import 'package:docfit_ui/features/conversion/conversion_workspace.dart';

import 'conversion_workspace_test.dart' show FakeHost;

/// 기존 가짜 호스트에 `conversion_window` 브리지 명령(ui_mode·open_native)만 더한다.
class _Host extends FakeHost {
  _Host({this.mode = 'flutter', this.openFails = false});
  String mode;
  bool openFails;
  final windowCalls = <String>[];

  @override
  Future<Object?> apiCall(String method, List<Object?> args) async {
    if (method != 'conversion_window') return super.apiCall(method, args);
    final payload = args.first as Map;
    final action = payload['action'] as String;
    windowCalls.add(
      action == 'ui_mode' && payload.containsKey('value')
          ? 'ui_mode=${payload['value']}'
          : action,
    );
    if (action == 'ui_mode') {
      if (payload.containsKey('value')) mode = payload['value'] as String;
      return {
        'ok': true,
        'data': {'mode': mode},
      };
    }
    if (action == 'open_native') {
      if (openFails) {
        return {
          'ok': false,
          'error': {'message': '몽땅파일변환 창(convert_win.exe)을 찾을 수 없습니다.'},
        };
      }
      return {
        'ok': true,
        'data': {'opened': true},
      };
    }
    return {'ok': false};
  }
}

Widget _app(ConversionHost host) => MaterialApp(
  home: Scaffold(body: ConversionTab(host: () => host)),
);

void main() {
  void bigScreen(WidgetTester tester) {
    tester.view.physicalSize = const Size(1280, 800);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
  }

  testWidgets('기본(flutter)이면 기존 변환 화면을 보이고 창은 열지 않는다', (tester) async {
    bigScreen(tester);
    final host = _Host();
    await tester.pumpWidget(_app(host));
    await tester.pumpAndSettle();
    expect(find.byType(ConversionWorkspace), findsOneWidget);
    expect(
      find.byKey(const ValueKey('conversion-open-native')),
      findsOneWidget,
    );
    expect(host.windowCalls, ['ui_mode']);
  });

  testWidgets('native 설정이면 창을 한 번 열고 안내만 보인다', (tester) async {
    bigScreen(tester);
    final host = _Host(mode: 'native');
    await tester.pumpWidget(_app(host));
    await tester.pumpAndSettle();
    expect(find.byType(ConversionWorkspace), findsNothing);
    expect(find.text('몽땅파일변환은 별도 Win32 창에서 열립니다.'), findsOneWidget);
    expect(host.windowCalls, ['ui_mode', 'open_native']);
    await tester.tap(find.text('창 다시 열기'));
    await tester.pumpAndSettle();
    expect(host.windowCalls.where((c) => c == 'open_native').length, 2);
  });

  testWidgets('"앱 안에서 변환"은 설정을 flutter 로 되돌리고 기존 화면을 보인다', (tester) async {
    bigScreen(tester);
    final host = _Host(mode: 'native');
    await tester.pumpWidget(_app(host));
    await tester.pumpAndSettle();
    await tester.tap(find.text('앱 안에서 변환'));
    await tester.pumpAndSettle();
    expect(host.mode, 'flutter');
    expect(find.byType(ConversionWorkspace), findsOneWidget);
  });

  testWidgets('"새 창(Win32)으로 열기"는 native 로 저장하고 창을 연다', (tester) async {
    bigScreen(tester);
    final host = _Host();
    await tester.pumpWidget(_app(host));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('conversion-open-native')));
    await tester.pumpAndSettle();
    expect(host.windowCalls, ['ui_mode', 'ui_mode=native', 'open_native']);
    expect(host.mode, 'native');
  });

  testWidgets('창을 열지 못하면 오류 문구를 탭에 보인다', (tester) async {
    bigScreen(tester);
    final host = _Host(mode: 'native', openFails: true);
    await tester.pumpWidget(_app(host));
    await tester.pumpAndSettle();
    expect(find.textContaining('convert_win.exe'), findsOneWidget);
  });
}
