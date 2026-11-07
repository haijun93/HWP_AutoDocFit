import 'package:docfit_ui/features/external_apps/external_app_launcher.dart';
import 'package:docfit_ui/features/workspace/tab_navigation_shell.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

/// 개발자 모드 외부 프로그램(사진첩·아웃라이너·사진관리) 사이드메뉴: 개발자 모드에서만 보이고,
/// 누르면 공용 실행기(`external_app`)에 실행을 요청한다. 실행 파일이 없으면 설치 안내·지정 창.
Future<void> _size(WidgetTester tester, [Size size = const Size(1400, 900)]) async {
  tester.view.physicalSize = size;
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
}

class _FakeApi {
  _FakeApi(this.responses);
  final Map<String, List<Map<String, Object?>>> responses;
  final calls = <List<Object?>>[];

  Future<Object?> call(String method, List<Object?> args) async {
    calls.add([method, ...args]);
    expect(method, 'external_app');
    final queue = responses[args.first as String]!;
    return queue.length > 1 ? queue.removeAt(0) : queue.first;
  }

  List<Object?> get actions => [for (final c in calls) c[1]];
  List<Object?> get payloads => [for (final c in calls) c[2]];
}

Widget _app({required bool developerMode, required _FakeApi api}) => MaterialApp(
      home: TabNavigationShell(
        documentEditor: const Text('문서편집 화면'),
        tableTemplates: const SizedBox(),
        tableSketch: const SizedBox(),
        diagram: const SizedBox(),
        mindMap: const SizedBox(),
        fileConversion: const SizedBox(),
        paint: const SizedBox(),
        fileRename: const SizedBox(),
        timemachine: const SizedBox(),
        developerMode: developerMode,
        onOpenExternalApp: (context, app) => openExternalApp(context, api.call, app),
      ),
    );

const _missing = {
  'ok': false,
  'code': 'APP_MISSING',
  'message': '아웃라이너 프로그램(Logseq)이 설치되어 있지 않습니다.',
};

final _outliner = kExternalApps.firstWhere((a) => a.id == 'outliner');

void main() {
  test('세 프로그램의 id·이름·아이콘이 서로 다르다', () {
    expect([for (final a in kExternalApps) a.id], ['photo_album', 'outliner', 'photo_manager']);
    expect([for (final a in kExternalApps) a.label], ['사진첩', '아웃라이너', '사진관리']);
    expect({for (final a in kExternalApps) a.icon}.length, 3);
  });

  testWidgets('일반 모드에서는 외부 프로그램 메뉴가 없다', (tester) async {
    await _size(tester);
    await tester.pumpWidget(_app(developerMode: false, api: _FakeApi({})));
    for (final app in kExternalApps) {
      expect(find.byKey(app.menuKey), findsNothing);
    }
  });

  testWidgets('개발자 모드: 세 메뉴가 아이콘·이름과 함께 보이고, 누르면 그 프로그램만 실행을 요청한다', (tester) async {
    await _size(tester);
    final api = _FakeApi({
      'launch': [{'ok': true, 'launched': true}],
    });
    await tester.pumpWidget(_app(developerMode: true, api: api));
    for (final app in kExternalApps) {
      final item = find.byKey(app.menuKey);
      expect(item, findsOneWidget);
      expect(find.descendant(of: item, matching: find.byIcon(app.icon)), findsOneWidget);
      expect(find.descendant(of: item, matching: find.text(app.label)), findsOneWidget);
    }
    for (final app in kExternalApps) {
      await tester.tap(find.byKey(app.menuKey));
      await tester.pumpAndSettle();
      expect(find.text('${app.label}을(를) 별도 창으로 열었습니다.'), findsOneWidget);
      ScaffoldMessenger.of(tester.element(find.byKey(app.menuKey))).clearSnackBars();
      await tester.pumpAndSettle();
    }
    expect(api.actions, ['launch', 'launch', 'launch']);
    expect(api.payloads, [
      {'app': 'photo_album'},
      {'app': 'outliner'},
      {'app': 'photo_manager'},
    ]);
    expect(find.text('문서편집 화면'), findsOneWidget);
    expect(find.byKey(const ValueKey('external-app-setup')), findsNothing);
  });

  testWidgets('실행 파일이 없으면 안내 창 → 실행 파일 지정 후 바로 실행', (tester) async {
    await _size(tester);
    final api = _FakeApi({
      'launch': [_missing, {'ok': true, 'launched': true}],
      'choose': [{'ok': true, 'cancelled': false, 'available': true, 'source': 'custom'}],
    });
    await tester.pumpWidget(_app(developerMode: true, api: api));
    await tester.tap(find.byKey(_outliner.menuKey));
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('external-app-setup')), findsOneWidget);
    expect(find.text('아웃라이너 준비'), findsOneWidget);
    expect(find.textContaining('설치되어 있지 않습니다'), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('external-app-choose')));
    await tester.pumpAndSettle();
    expect(api.actions, ['launch', 'choose', 'launch']);
    expect(api.payloads, everyElement({'app': 'outliner'}));
    expect(find.byKey(const ValueKey('external-app-setup')), findsNothing);
  });

  testWidgets('다른 프로그램을 지정하면 이유를 보이고, 포크·공식 배포처 단추는 target만 보낸다', (tester) async {
    await _size(tester);
    final api = _FakeApi({
      'launch': [_missing],
      'choose': [{'ok': false, 'code': 'APP_WRONG_EXE', 'message': '아웃라이너 실행 파일은 Logseq.exe이어야 합니다.'}],
      'open_download': [{'ok': true}],
    });
    await tester.pumpWidget(_app(developerMode: true, api: api));
    await tester.tap(find.byKey(_outliner.menuKey));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('external-app-choose')));
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('external-app-notice')), findsOneWidget);
    expect(find.textContaining('Logseq.exe이어야'), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('external-app-fork')));
    await tester.tap(find.byKey(const ValueKey('external-app-official')));
    await tester.pumpAndSettle();
    expect(api.actions, ['launch', 'choose', 'open_download', 'open_download']);
    expect(api.payloads.sublist(2), [
      {'app': 'outliner', 'target': 'fork'},
      {'app': 'outliner', 'target': 'official'},
    ]);
    await tester.tap(find.text('닫기'));
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('external-app-setup')), findsNothing);
  });

  testWidgets('선택 창을 취소하면 안내 창이 남고 오류를 보이지 않는다', (tester) async {
    await _size(tester);
    final api = _FakeApi({
      'launch': [_missing],
      'choose': [{'ok': true, 'cancelled': true, 'available': false}],
    });
    await tester.pumpWidget(_app(developerMode: true, api: api));
    await tester.tap(find.byKey(_outliner.menuKey));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('external-app-choose')));
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('external-app-setup')), findsOneWidget);
    expect(find.byKey(const ValueKey('external-app-notice')), findsNothing);
  });

  testWidgets('개발자 모드 해제 응답 등 다른 오류는 알림으로만 보인다', (tester) async {
    await _size(tester);
    final api = _FakeApi({
      'launch': [{'ok': false, 'code': 'DEVELOPER_MODE_REQUIRED', 'message': '개발자 모드에서만 사용할 수 있습니다.'}],
    });
    await tester.pumpWidget(_app(developerMode: true, api: api));
    await tester.tap(find.byKey(kExternalApps.last.menuKey));
    await tester.pumpAndSettle();
    expect(find.text('개발자 모드에서만 사용할 수 있습니다.'), findsOneWidget);
    expect(find.byKey(const ValueKey('external-app-setup')), findsNothing);
  });

  for (final sidebarOnly in [false, true]) {
    testWidgets('접힌 메뉴(사이드메뉴 모드=$sidebarOnly): 세 아이콘이 다른 메뉴 아이콘과 같은 세로줄에 있다', (tester) async {
      await _size(tester, const Size(800, 1400));
      await tester.pumpWidget(MaterialApp(
        home: Scaffold(
          body: Row(children: [
            SizedBox(
              width: sidebarOnly ? 120 : kSidebarCollapsedWidth,
              child: WorkspaceTabBar(
                labels: const ['문서편집', '표서식'],
                icons: const [Icons.description, Icons.table_chart],
                selectedIndex: 0,
                onSelected: (_) {},
                scrollController: ScrollController(),
                expanded: false,
                developerMode: true,
                sidebarOnly: sidebarOnly,
                onOpenExternalApp: (_, _) {},
              ),
            ),
          ]),
        ),
      ));
      final reference = tester.getCenter(find.byIcon(Icons.description)).dx;
      for (final app in kExternalApps) {
        expect(find.descendant(of: find.byKey(app.menuKey), matching: find.text(app.label)), findsNothing);
        expect(tester.getCenter(find.descendant(of: find.byKey(app.menuKey), matching: find.byIcon(app.icon))).dx,
            moreOrLessEquals(reference, epsilon: 0.5));
      }
      expect(tester.takeException(), isNull);
    });
  }

  testWidgets('펼친 메뉴: 세 아이콘이 다른 메뉴 아이콘과 같은 세로줄에 있다', (tester) async {
    await _size(tester, const Size(800, 1400));
    await tester.pumpWidget(MaterialApp(
      home: Scaffold(
        body: Row(children: [
          SizedBox(
            width: kSidebarExpandedWidth,
            child: WorkspaceTabBar(
              labels: const ['문서편집'],
              icons: const [Icons.description],
              selectedIndex: 0,
              onSelected: (_) {},
              scrollController: ScrollController(),
              expanded: true,
              developerMode: true,
              onOpenExternalApp: (_, _) {},
            ),
          ),
        ]),
      ),
    ));
    final reference = tester.getCenter(find.byIcon(Icons.description)).dx;
    for (final app in kExternalApps) {
      expect(tester.getCenter(find.descendant(of: find.byKey(app.menuKey), matching: find.byIcon(app.icon))).dx,
          moreOrLessEquals(reference, epsilon: 0.5));
    }
  });
}
