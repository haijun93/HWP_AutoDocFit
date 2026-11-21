import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:docfit_ui/features/settings/settings_ui_gate.dart';

/// `settings_window` 브리지 명령(ui_mode·open_native·open_if_native)만 흉내 내는 가짜 API.
class _FakeApi {
  _FakeApi({this.mode = 'flutter', this.openFails = false, this.throwOnCall = false});
  String mode;
  bool openFails;
  bool throwOnCall;
  final calls = <String>[];

  Future<Object?> call(String method, List<Object?> args) async {
    if (throwOnCall) throw StateError('브리지 없음');
    final payload = args.first as Map;
    final action = payload['action'] as String;
    calls.add(action == 'ui_mode' && payload.containsKey('value') ? 'ui_mode=${payload['value']}' : action);
    if (method != 'settings_window') return {'ok': false};
    if (action == 'ui_mode') {
      if (payload.containsKey('value')) mode = payload['value'] as String;
      return {'ok': true, 'data': {'mode': mode}};
    }
    if (action == 'open_native' || action == 'open_if_native') {
      if (action == 'open_if_native' && mode != 'native') {
        return {'ok': true, 'data': {'mode': mode, 'opened': false}};
      }
      if (openFails) {
        return {'ok': false, 'error': {'message': '설정 창 프로그램(settings_win.exe)을 찾을 수 없습니다.'}};
      }
      return {'ok': true, 'data': {'mode': mode, 'opened': true}};
    }
    return {'ok': false};
  }
}

Widget _tile(_FakeApi api) => MaterialApp(home: Scaffold(body: SettingsUiModeTile(api: api.call)));

void main() {
  test('기본(flutter)이면 창을 열지 않고 기존 화면으로 이어진다', () async {
    final api = _FakeApi();
    final result = await openNativeSettingsIfEnabled(api.call);
    expect(result.opened, isFalse);
    expect(result.message, isEmpty);
    expect(api.calls, ['open_if_native']);
  });

  test('native 이면 창을 열고 opened=true 를 돌려준다', () async {
    final api = _FakeApi(mode: 'native');
    final result = await openNativeSettingsIfEnabled(api.call);
    expect(result.opened, isTrue);
  });

  test('native 인데 창 프로그램이 없으면 이유를 돌려주고 기존 화면으로 이어진다', () async {
    final api = _FakeApi(mode: 'native', openFails: true);
    final result = await openNativeSettingsIfEnabled(api.call);
    expect(result.opened, isFalse);
    expect(result.message, contains('settings_win.exe'));
  });

  test('브리지가 예외를 던져도 문지기는 예외 없이 기존 화면으로 이어진다', () async {
    final result = await openNativeSettingsIfEnabled(_FakeApi(throwOnCall: true).call);
    expect(result.opened, isFalse);
  });

  testWidgets('서랍 줄: 기본에서는 "새 창으로 열기"가 창을 열고 설정을 native 로 바꾼다', (tester) async {
    final api = _FakeApi();
    await tester.pumpWidget(_tile(api));
    await tester.pumpAndSettle();
    expect(find.text('새 창으로 열기'), findsOneWidget);
    expect(api.mode, 'flutter');
    await tester.tap(find.byKey(const ValueKey('settings-ui-mode-toggle')));
    await tester.pumpAndSettle();
    expect(api.calls, ['ui_mode', 'open_native', 'ui_mode=native']);
    expect(api.mode, 'native');
    expect(find.text('앱 안에서 보기'), findsOneWidget);
  });

  testWidgets('서랍 줄: 창을 열지 못하면 설정을 바꾸지 않고 이유를 보인다', (tester) async {
    final api = _FakeApi(openFails: true);
    await tester.pumpWidget(_tile(api));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('settings-ui-mode-toggle')));
    await tester.pumpAndSettle();
    expect(api.mode, 'flutter');
    expect(find.textContaining('settings_win.exe'), findsOneWidget);
    expect(find.text('새 창으로 열기'), findsOneWidget);
  });

  testWidgets('서랍 줄: native 에서는 "앱 안에서 보기"가 설정을 flutter 로 되돌린다', (tester) async {
    final api = _FakeApi(mode: 'native');
    await tester.pumpWidget(_tile(api));
    await tester.pumpAndSettle();
    expect(find.text('앱 안에서 보기'), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('settings-ui-mode-toggle')));
    await tester.pumpAndSettle();
    expect(api.mode, 'flutter');
    expect(api.calls.contains('open_native'), isFalse);
    expect(find.text('새 창으로 열기'), findsOneWidget);
  });
}
