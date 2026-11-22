import 'dart:convert';
import 'dart:io';
import 'dart:ui' as ui;

import 'package:docfit_ui/main.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

/// OPTUI1 화면 캡처 도구. 환경 변수 OPTUI1_CAPTURE_DIR(저장 폴더)와 OPTUI1_STAGES_JSON(실제
/// '한 번에 적용' 세부 작업 목록)이 있을 때만 PNG를 만들고, 없으면 건너뛴다(일반 시험에는 영향 없음).
void main() {
  final dir = Platform.environment['OPTUI1_CAPTURE_DIR'];
  final stagesFile = Platform.environment['OPTUI1_STAGES_JSON'];
  final tag = Platform.environment['OPTUI1_TAG'] ?? 'shot';
  final skip = dir == null || stagesFile == null;

  testWidgets('capture work options screen', (tester) async {
    final fontBytes = File(r'C:\Windows\Fonts\malgun.ttf').readAsBytesSync();
    final font = FontLoader('NotoSansKR')
      ..addFont(Future.value(ByteData.view(fontBytes.buffer)));
    await tester.runAsync(() => font.load());
    tester.view.physicalSize = const Size(1280, 2400);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    final stages = (jsonDecode(File(stagesFile!).readAsStringSync()) as List)
        .map((e) => Map<String, dynamic>.from(e as Map))
        .toList();
    // 사용자가 한 곳에서 몇 개를 끈 상황(표 제외 켬)을 흉내 낸다.
    for (final s in stages) {
      if (s['key'] == 'table_width' || s['key'] == 'precise_table') s['on'] = false;
    }
    final state = <String, dynamic>{
      'files': [
        {'name': '문서.hwp', 'folder': r'C:\문서'},
      ],
      'mode': 'all',
      'include_spacing': true,
      'can_start': true,
      'conversion_target': 'hwpx',
      'conversion_targets': [],
      'running': false,
      'range': {'enabled': false, 'start': '1', 'end': '1'},
      'results': [],
      'stages': stages,
      'default_saved': false,
      'profiles': [
        {'id': '', 'name': '기본 서식'},
      ],
      'profile': '',
      'card_options': {'all_exclude_tables': false},
      'options': {'autoclose': true},
      'version': 'test',
    };
    Future<Object?> api(String name, List<Object?> args) async =>
        Map<String, dynamic>.from(state);
    await tester.pumpWidget(DocFitApp(api: api));
    await tester.pumpAndSettle();
    await tester.tap(find.text('작업 방식 선택'));
    await tester.pumpAndSettle();
    Future<void> shot(String name) async {
      final boundary = tester.renderObject<RenderRepaintBoundary>(
        find.byType(RepaintBoundary).first,
      );
      await tester.runAsync(() async {
        final image = await boundary.toImage();
        final bytes = await image.toByteData(format: ui.ImageByteFormat.png);
        File('$dir/$name.png').writeAsBytesSync(bytes!.buffer.asUint8List());
      });
    }

    await shot(tag);
    if (tag == 'after') {
      // 고급 설정과 '표'·'쪽 배치' 묶음을 펼친 모습.
      await tester.tap(find.byKey(const Key('advancedOptionsTitle-all')));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const ValueKey('stage-section-title-table')));
      await tester.pumpAndSettle();
      await shot('after_expanded');
    }
    await tester.pumpWidget(const SizedBox());
  }, skip: skip);
}
