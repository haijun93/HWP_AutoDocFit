import 'package:docfit_ui/features/external_apps/external_app_launcher.dart';
import 'package:docfit_ui/features/workspace/tab_navigation_shell.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

/// 개발자 모드 외부 프로그램(사진첩·아웃라이너·사진관리) 사이드메뉴: 개발자 모드에서만 보이고,
/// 누르면 공용 실행기(`external_app`)에 실행을 요청한다. 실행 파일이 없으면 설치 안내·지정 창.
Future<void> _size(
  WidgetTester tester, [
  Size size = const Size(1400, 900),
]) async {
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

Widget _app({required bool developerMode, required _FakeApi api}) =>
    MaterialApp(
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
        onOpenExternalApp: (context, app) =>
            openExternalApp(context, api.call, app),
      ),
    );

void main() {
  test('세 프로그램의 id·이름·아이콘이 서로 다르다', () {
    expect(
      [for (final a in kExternalApps) a.id],
      ['photo_album', 'outliner', 'photo_manager', 'pdf_editor'],
    );
    expect(
      [for (final a in kExternalApps) a.label],
      ['사진첩', '아웃라이너', '사진관리', 'PDF편집'],
    );
    expect({for (final a in kExternalApps) a.icon}.length, 4);
  });

  testWidgets('일반 모드에서는 외부 프로그램 메뉴가 없다', (tester) async {
    await _size(tester);
    await tester.pumpWidget(_app(developerMode: false, api: _FakeApi({})));
    for (final app in kExternalApps) {
      expect(find.byKey(app.menuKey), findsNothing);
    }
  });

  testWidgets('개발자 모드: 세 메뉴가 아이콘·이름과 함께 보이고, 누르면 그 프로그램만 실행을 요청한다', (
    tester,
  ) async {
    await _size(tester);
    final api = _FakeApi({
      'launch': [
        {'ok': true, 'launched': true},
      ],
    });
    await tester.pumpWidget(_app(developerMode: true, api: api));
    for (final app in kExternalApps) {
      final item = find.byKey(app.menuKey);
      expect(item, findsOneWidget);
      expect(
        find.descendant(of: item, matching: find.byIcon(app.icon)),
        findsOneWidget,
      );
      expect(
        find.descendant(of: item, matching: find.text(app.label)),
        findsOneWidget,
      );
    }
    for (final app in kExternalApps) {
      await tester.tap(find.byKey(app.menuKey));
      await tester.pumpAndSettle();
      expect(find.text('${app.label}을(를) 별도 창으로 열었습니다.'), findsOneWidget);
      ScaffoldMessenger.of(
        tester.element(find.byKey(app.menuKey)),
      ).clearSnackBars();
      await tester.pumpAndSettle();
    }
    expect(api.actions, ['launch', 'launch', 'launch', 'launch']);
    expect(api.payloads, [
      {'app': 'photo_album'},
      {'app': 'outliner'},
      {'app': 'photo_manager'},
      {'app': 'pdf_editor'},
    ]);
    expect(find.text('문서편집 화면'), findsOneWidget);
    expect(find.byKey(const ValueKey('external-app-setup')), findsNothing);
  });

  testWidgets('개발자 모드 해제 응답 등 다른 오류는 알림으로만 보인다', (tester) async {
    await _size(tester);
    final api = _FakeApi({
      'launch': [
        {
          'ok': false,
          'code': 'DEVELOPER_MODE_REQUIRED',
          'message': '개발자 모드에서만 사용할 수 있습니다.',
        },
      ],
    });
    await tester.pumpWidget(_app(developerMode: true, api: api));
    await tester.tap(find.byKey(kExternalApps.last.menuKey));
    await tester.pumpAndSettle();
    expect(find.text('개발자 모드에서만 사용할 수 있습니다.'), findsOneWidget);
    expect(find.byKey(const ValueKey('external-app-setup')), findsNothing);
  });

  testWidgets('미설치 앱은 설치 단추 한 번으로 진행 후 자동 실행한다', (tester) async {
    await _size(tester);
    final api = _FakeApi({
      'launch': [
        {'ok': false, 'code': 'APP_MISSING', 'message': '설치되어 있지 않습니다.'},
        {'ok': true, 'launched': true},
      ],
      'latest': [{'ok': true, 'version': '2.0.0', 'size': 2097152,
        'source': 'haijun93/mongdangoutliner', 'license': 'AGPL-3.0'}],
      'install': [{'ok': true, 'install': {'running': true, 'phase': 'downloading',
        'percent': 35, 'message': '내려받는 중…'}}],
      'progress': [{'ok': true, 'install': {'running': false, 'phase': 'done',
        'ok': true, 'percent': 100, 'message': '설치를 마쳤습니다.'}}],
    });
    await tester.pumpWidget(_app(developerMode: true, api: api));
    await tester.tap(find.byKey(kExternalApps[1].menuKey));
    await tester.pumpAndSettle();
    expect(find.text('버전  2.0.0'), findsOneWidget);
    expect(find.text('크기  2.0 MB'), findsOneWidget);
    expect(find.text('출처  haijun93/mongdangoutliner'), findsOneWidget);
    expect(find.text('라이선스  AGPL-3.0'), findsOneWidget);
    expect(find.byKey(const ValueKey('external-app-install')), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('external-app-install')));
    await tester.pump();
    expect(find.byKey(const ValueKey('external-app-progress')), findsOneWidget);
    await tester.pump(const Duration(milliseconds: 600));
    await tester.pumpAndSettle();
    expect(api.actions, ['launch', 'latest', 'install', 'progress', 'launch']);
    expect(find.byKey(const ValueKey('external-app-setup')), findsNothing);
  });

  testWidgets('설치된 앱의 최신 버전이 높으면 업데이트 단추를 표시한다', (tester) async {
    await _size(tester);
    final api = _FakeApi({
      'launch': [{'ok': false, 'code': 'APP_MISSING', 'message': '설치 경로를 확인합니다.'}],
      'latest': [{'ok': true, 'version': '3.2.0', 'installed_version': '3.1.0',
        'update_available': true, 'size': 100, 'source': 'KDE/digikam', 'license': 'GPL-2.0'}],
    });
    await tester.pumpWidget(_app(developerMode: true, api: api));
    await tester.tap(find.byKey(kExternalApps[2].menuKey));
    await tester.pumpAndSettle();
    expect(find.text('사진관리 업데이트'), findsOneWidget);
    expect(find.text('업데이트'), findsOneWidget);
    expect(find.byKey(const ValueKey('external-app-install')), findsOneWidget);
  });

  for (final sidebarOnly in [false, true]) {
    testWidgets('접힌 메뉴(사이드메뉴 모드=$sidebarOnly): 세 아이콘이 다른 메뉴 아이콘과 같은 세로줄에 있다', (
      tester,
    ) async {
      await _size(tester, const Size(800, 1400));
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: Row(
              children: [
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
              ],
            ),
          ),
        ),
      );
      final reference = tester.getCenter(find.byIcon(Icons.description)).dx;
      for (final app in kExternalApps) {
        expect(
          find.descendant(
            of: find.byKey(app.menuKey),
            matching: find.text(app.label),
          ),
          findsNothing,
        );
        expect(
          tester
              .getCenter(
                find.descendant(
                  of: find.byKey(app.menuKey),
                  matching: find.byIcon(app.icon),
                ),
              )
              .dx,
          moreOrLessEquals(reference, epsilon: 0.5),
        );
      }
      expect(tester.takeException(), isNull);
    });
  }

  testWidgets('펼친 메뉴: 세 아이콘이 다른 메뉴 아이콘과 같은 세로줄에 있다', (tester) async {
    await _size(tester, const Size(800, 1400));
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: Row(
            children: [
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
            ],
          ),
        ),
      ),
    );
    final reference = tester.getCenter(find.byIcon(Icons.description)).dx;
    for (final app in kExternalApps) {
      expect(
        tester
            .getCenter(
              find.descendant(
                of: find.byKey(app.menuKey),
                matching: find.byIcon(app.icon),
              ),
            )
            .dx,
        moreOrLessEquals(reference, epsilon: 0.5),
      );
    }
  });
}
