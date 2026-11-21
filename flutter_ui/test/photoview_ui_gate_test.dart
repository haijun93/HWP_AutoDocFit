import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:docfit_ui/features/photo_album/photoview_ui_gate.dart';

/// `photoview_window` 브리지 명령(ui_mode·open_native)만 흉내 내는 가짜 API.
class _FakeApi {
  _FakeApi({this.mode = 'flutter', this.openFails = false});
  String mode;
  bool openFails;
  final calls = <String>[];

  Future<Object?> call(String method, List<Object?> args) async {
    final payload = args.first as Map;
    final action = payload['action'] as String;
    calls.add(action == 'ui_mode' && payload.containsKey('value') ? 'ui_mode=${payload['value']}' : action);
    if (method != 'photoview_window') return {'ok': false};
    if (action == 'ui_mode') {
      if (payload.containsKey('value')) mode = payload['value'] as String;
      return {'ok': true, 'data': {'mode': mode}};
    }
    if (action == 'open_native') {
      if (openFails) {
        return {'ok': false, 'error': {'message': '사진뷰 창 프로그램(photoview_win.exe)을 찾을 수 없습니다.'}};
      }
      return {'ok': true, 'data': {'opened': true}};
    }
    return {'ok': false};
  }
}

Widget _app(_FakeApi api) => MaterialApp(
      home: Scaffold(
        body: PhotoViewUiGate(api: api.call, workspaceBuilder: (_) => const Text('기존 사진뷰 화면')),
      ),
    );

void main() {
  testWidgets('기본(flutter) 설정이면 기존 화면을 보이고 창은 열지 않는다', (tester) async {
    final api = _FakeApi();
    await tester.pumpWidget(_app(api));
    await tester.pumpAndSettle();
    expect(find.text('기존 사진뷰 화면'), findsOneWidget);
    expect(find.byKey(const ValueKey('photoview-open-native')), findsOneWidget);
    expect(api.calls, ['ui_mode']);
  });

  testWidgets('native 설정이면 창을 한 번 열고 탭에는 안내만 보인다', (tester) async {
    final api = _FakeApi(mode: 'native');
    await tester.pumpWidget(_app(api));
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('photoview-native-notice')), findsOneWidget);
    expect(find.text('기존 사진뷰 화면'), findsNothing);
    expect(api.calls, ['ui_mode', 'open_native']);
    await tester.tap(find.byKey(const ValueKey('photoview-reopen-native')));
    await tester.pumpAndSettle();
    expect(api.calls.where((c) => c == 'open_native').length, 2);
  });

  testWidgets('"앱 안에서 보기"는 설정을 flutter 로 되돌리고 기존 화면을 보인다', (tester) async {
    final api = _FakeApi(mode: 'native');
    await tester.pumpWidget(_app(api));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('photoview-use-flutter')));
    await tester.pumpAndSettle();
    expect(api.mode, 'flutter');
    expect(find.text('기존 사진뷰 화면'), findsOneWidget);
  });

  testWidgets('"새 창으로 열기"는 창이 열릴 때만 native 로 바꾼다', (tester) async {
    final api = _FakeApi();
    await tester.pumpWidget(_app(api));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('photoview-open-native')));
    await tester.pumpAndSettle();
    expect(api.calls, ['ui_mode', 'open_native', 'ui_mode=native']);
    expect(api.mode, 'native');
    expect(find.byKey(const ValueKey('photoview-native-notice')), findsOneWidget);
  });

  testWidgets('창 프로그램이 없으면 설정을 바꾸지 않고 이유를 알린다(기존 화면 유지)', (tester) async {
    final api = _FakeApi(openFails: true);
    await tester.pumpWidget(_app(api));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('photoview-open-native')));
    await tester.pumpAndSettle();
    expect(api.mode, 'flutter');
    expect(find.text('기존 사진뷰 화면'), findsOneWidget);
    expect(find.textContaining('photoview_win.exe'), findsOneWidget);
  });

  testWidgets('native 설정인데 창을 못 열면 안내 문구와 되돌리기 단추가 보인다', (tester) async {
    final api = _FakeApi(mode: 'native', openFails: true);
    await tester.pumpWidget(_app(api));
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('photoview-native-message')), findsOneWidget);
    expect(find.byKey(const ValueKey('photoview-use-flutter')), findsOneWidget);
  });

  testWidgets('설정을 읽지 못하면 기존 화면으로 둔다', (tester) async {
    Future<Object?> broken(String method, List<Object?> args) async => throw StateError('x');
    await tester.pumpWidget(MaterialApp(
      home: Scaffold(body: PhotoViewUiGate(api: broken, workspaceBuilder: (_) => const Text('기존 사진뷰 화면'))),
    ));
    await tester.pumpAndSettle();
    expect(find.text('기존 사진뷰 화면'), findsOneWidget);
  });
}
