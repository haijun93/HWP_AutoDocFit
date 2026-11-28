import 'dart:io';
import 'dart:ui' as ui;

import 'package:docfit_ui/core/theme/app_theme.dart';
import 'package:docfit_ui/features/outliner/outliner_ui_gate.dart';
import 'package:docfit_ui/features/outliner/outliner_workspace.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

/// OUTPANEL1 화면 캡처 도구(합성 데이터). 환경 변수 OUTPANEL1_CAPTURE_DIR(저장 폴더)가 있을 때만
/// PNG 를 만든다. 아이콘 글꼴은 FLUTTER_ROOT 에서 읽는다.
class _Recent implements RecentPagesStore {
  @override
  Future<List<String>> load() async => ['업무 메모', '회의록'];
  @override
  Future<void> save(List<String> pages) async {}
}

class _Panels implements OutlinerPanelStore {
  _Panels(this.data);
  final Map<String, bool> data;
  @override
  Future<Map<String, bool>> load() async => data;
  @override
  Future<void> save(Map<String, bool> collapsed) async {}
}

Future<Object?> _api(String method, List<Object?> args) async {
  final action = (args.first as Map)['action'];
  const blocks = [
    {'id': 1, 'text': '주간 업무 계획을 정리하고 담당자별 일정을 확인한다', 'indent': 0, 'folded': false},
    {'id': 2, 'text': '자료 취합 (금요일까지)', 'indent': 1, 'folded': false, 'status': 'TODO'},
    {'id': 3, 'text': '검토 의견 반영', 'indent': 1, 'folded': false, 'status': 'DONE'},
    {'id': 4, 'text': '다음 회의 안건: [[회의록]] 참고', 'indent': 0, 'folded': false},
  ];
  final Map data = switch (action) {
    'ui_mode' => {'mode': 'flutter'},
    'list' => {
      'pages': [
        {'name': '업무 메모'},
        {'name': '회의록'},
        {'name': '2026-11-28'},
      ],
    },
    'daily' || 'load' => {
      'page': '2026-11-28',
      'markdown': '- 합성\n',
      'version': 'v1',
      'document': {'blocks': blocks},
    },
    'backlinks' => {'block_ids': [2, 4]},
    _ => {},
  };
  return {'ok': true, 'data': data};
}

void main() {
  final dir = Platform.environment['OUTPANEL1_CAPTURE_DIR'];
  final flutterRoot = Platform.environment['FLUTTER_ROOT'];
  final skip = dir == null || flutterRoot == null;

  testWidgets('capture outliner panels', (tester) async {
    Future<void> font(String family, String path) async {
      final bytes = File(path).readAsBytesSync();
      final loader = FontLoader(family)
        ..addFont(Future.value(ByteData.view(bytes.buffer)));
      await tester.runAsync(loader.load);
    }

    await font('NotoSansKR', r'C:\Windows\Fonts\malgun.ttf');
    await font(
      'MaterialIcons',
      '$flutterRoot/bin/cache/artifacts/material_fonts/materialicons-regular.otf',
    );
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    Directory(dir!).createSync(recursive: true);
    final boundaryKey = GlobalKey();

    for (final (theme, themeName) in [
      (lightAppTheme, 'light'),
      (darkAppTheme, 'dark'),
    ]) {
      for (final width in [700, 1100, 1500]) {
        for (final (state, Map<String, bool> stored) in [
          ('first', <String, bool>{}), // 저장 값 없이 처음 연 모습
          ('open', {'left': false, 'right': false}),
          ('closed', {'left': true, 'right': true}),
          ('headings', {'right': false, 'backlinks': true, 'blockRefs': true}),
        ]) {
          tester.view.physicalSize = Size(width.toDouble(), 640);
          await tester.pumpWidget(const SizedBox());
          await tester.pumpWidget(
            RepaintBoundary(
              key: boundaryKey,
              child: MaterialApp(
                debugShowCheckedModeBanner: false,
                theme: theme,
                home: Scaffold(
                  body: OutlinerUiGate(
                    api: _api,
                    recentPagesStore: _Recent(),
                    panelStore: _Panels(stored),
                  ),
                ),
              ),
            ),
          );
          await tester.pumpAndSettle();
          expect(tester.takeException(), isNull);
          final boundary =
              boundaryKey.currentContext!.findRenderObject()!
                  as RenderRepaintBoundary;
          await tester.runAsync(() async {
            final image = await boundary.toImage();
            final png = await image.toByteData(format: ui.ImageByteFormat.png);
            File(
              '$dir/${themeName}_${width}_$state.png',
            ).writeAsBytesSync(png!.buffer.asUint8List());
          });
        }
      }
    }
    await tester.pumpWidget(const SizedBox());
  }, skip: skip);
}
