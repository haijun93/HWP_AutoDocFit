import 'dart:async';

import 'package:docfit_ui/core/theme/app_theme.dart';
import 'package:docfit_ui/main.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

Map<String, dynamic> _state() => <String, dynamic>{
  'files': <Map<String, String>>[],
  'mode': 'spacing',
  'can_start': true,
  'running': false,
  'range': {'enabled': false, 'start': '1', 'end': '1'},
  'results': [],
  'stages': [],
  'profiles': [
    {'id': '', 'name': '기본 서식'},
  ],
  'profile': '',
  'options': {
    'autoclose': true,
    'verify': false,
    'check_updates': true,
    'developer_mode': false,
    'always_on_top': true,
  },
  'theme_mode': 'system',
  'version': 'test',
};

void main() {
  group('ThemeSaveQueue', () {
    test('saves while busy and sends only the latest queued choice', () async {
      final sent = <AppThemeChoice>[];
      final gate = Completer<void>();
      final queue = ThemeSaveQueue((c) async {
        sent.add(c);
        if (sent.length == 1) await gate.future;
      });
      final first = queue.save(AppThemeChoice.dark);
      expect(queue.saving, isTrue);
      // 첫 저장이 끝나기 전 두 번 바꾸면 마지막 값만 이어서 보낸다.
      final second = queue.save(AppThemeChoice.light);
      final third = queue.save(AppThemeChoice.system);
      gate.complete();
      await Future.wait([first, second, third]);
      expect(sent, [AppThemeChoice.dark, AppThemeChoice.system]);
      expect(queue.saving, isFalse);
    });

    test('reports a failed save and keeps working', () async {
      final errors = <Object>[];
      final sent = <AppThemeChoice>[];
      final queue = ThemeSaveQueue((c) async {
        sent.add(c);
        if (c == AppThemeChoice.dark) throw StateError('host busy');
      }, onError: errors.add);
      await queue.save(AppThemeChoice.dark);
      await queue.save(AppThemeChoice.light);
      expect(sent, [AppThemeChoice.dark, AppThemeChoice.light]);
      expect(errors, hasLength(1));
      expect(queue.saving, isFalse);
    });
  });

  testWidgets('theme choice is saved even while another command is running', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(1280, 900);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    addTearDown(() => appThemeChoice.value = AppThemeChoice.system);

    final calls = <List<Object?>>[];
    final state = _state();
    final slow = Completer<void>();
    Future<Object?> api(String name, List<Object?> args) async {
      if (name != 'get_state') calls.add([name, ...args]);
      if (name == 'set_mode') await slow.future;
      if (name == 'set_theme_mode') state['theme_mode'] = args[0];
      return Map<String, dynamic>.from(state);
    }

    await tester.pumpWidget(DocFitApp(api: api));
    await tester.pumpAndSettle();
    final workspace = tester.state(find.byType(Workspace)) as dynamic;
    // 느린 명령이 처리 중(busy)인 동안 테마를 바꾼다.
    final running = workspace.command('set_mode', ['spacing']) as Future<void>;
    await tester.pump();
    expect(workspace.busy, isTrue);
    await (workspace.setThemeChoice(AppThemeChoice.dark) as Future<void>);
    expect(
      calls.where((c) => c.first == 'set_theme_mode').toList(),
      [
        ['set_theme_mode', 'dark'],
      ],
    );
    expect(state['theme_mode'], 'dark');
    slow.complete();
    await running;
    await tester.pumpAndSettle();
    await tester.pumpWidget(const SizedBox());
    appThemeChoice.value = AppThemeChoice.system;
  });

  testWidgets('quick settings theme picker switches to dark and saves', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(1280, 900);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    addTearDown(() => appThemeChoice.value = AppThemeChoice.system);

    final calls = <List<Object?>>[];
    final state = _state();
    Future<Object?> api(String name, List<Object?> args) async {
      if (name != 'get_state') calls.add([name, ...args]);
      if (name == 'set_theme_mode') state['theme_mode'] = args[0];
      return Map<String, dynamic>.from(state);
    }

    await tester.pumpWidget(DocFitApp(api: api));
    await tester.pumpAndSettle();
    expect(appThemeChoice.value, AppThemeChoice.system);

    await tester.tap(find.byTooltip('설정'));
    await tester.pumpAndSettle();
    // 설정 서랍 목록은 필요한 만큼만 그리므로 '프로그램' 묶음까지 내린다.
    await tester.scrollUntilVisible(
      find.text(AppThemeChoice.dark.label),
      200,
      scrollable: find
          .descendant(of: find.byType(Drawer), matching: find.byType(Scrollable))
          .first,
    );
    await tester.pumpAndSettle();
    expect(
      find.byWidgetPredicate(
        (w) => w is Semantics && w.properties.label == '화면 테마',
      ),
      findsOneWidget,
    );
    expect(
      find.text('시스템 설정을 따르면 Windows 밝게/어둡게·고대비에 맞춰 바뀌어요.'),
      findsOneWidget,
    );
    for (final choice in AppThemeChoice.values) {
      expect(find.text(choice.label), findsOneWidget);
    }

    final dark = find.text(AppThemeChoice.dark.label);
    await tester.ensureVisible(dark);
    await tester.tap(dark);
    await tester.pumpAndSettle();

    expect(appThemeChoice.value, AppThemeChoice.dark);
    expect(
      calls.where((c) => c.first == 'set_theme_mode').toList(),
      [
        ['set_theme_mode', 'dark'],
      ],
    );

    final scaffold = find.byType(Scaffold).first;
    final context = tester.element(scaffold);
    expect(Theme.of(context).scaffoldBackgroundColor, AppColors.dark.canvas);
    expect(context.colors.canvas, AppColors.dark.canvas);
    final background = tester.widget<Material>(
      find
          .descendant(of: scaffold, matching: find.byType(Material))
          .first,
    );
    expect(background.color, AppColors.dark.canvas);
    expect(tester.takeException(), isNull);

    await tester.pumpWidget(const SizedBox());
    appThemeChoice.value = AppThemeChoice.system;
  });
}
