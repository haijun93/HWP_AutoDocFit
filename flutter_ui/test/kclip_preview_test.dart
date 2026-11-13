// 몽땅클립보드 그림 미리보기: 상태 기계(빠른 목록 preview.rs와 같은 규칙)·위치·앱 목록 마우스 오버.
import 'dart:ui' show PointerDeviceKind;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:docfit_ui/features/kclip/kclip_dialog.dart';
import 'package:docfit_ui/features/kclip/kclip_preview.dart';

const _png1x1 =
    'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/pV8AAAAASUVORK5CYII=';

void main() {
  const a = KClipPreviewTarget(1, 10);
  const b = KClipPreviewTarget(2, 20);

  test('캐시 없는 그림은 요청 → 준비되면 보이고, 같은 항목 위 움직임은 옮기기', () {
    final m = KClipPreviewMachine();
    expect(m.point(a, cached: false), KClipPreviewAction.request);
    expect(m.phase, KClipPreviewPhase.waiting);
    expect(m.point(a, cached: false), KClipPreviewAction.nothing, reason: '같은 그림을 다시 요청하지 않음');
    expect(m.ready(a, ok: true), KClipPreviewAction.show);
    expect(m.point(a, cached: true), KClipPreviewAction.move);
    expect(m.point(null, cached: false), KClipPreviewAction.hide);
    expect(m.phase, KClipPreviewPhase.hidden);
    expect(m.hide(), KClipPreviewAction.nothing);
  });

  test('늦게 온 결과·실패는 다른 그림을 보이지 않는다', () {
    final m = KClipPreviewMachine();
    m.point(a, cached: false);
    expect(m.point(b, cached: false), KClipPreviewAction.request);
    expect(m.ready(a, ok: true), KClipPreviewAction.nothing);
    expect(m.ready(b, ok: false), KClipPreviewAction.hide);
    expect(m.ready(b, ok: true), KClipPreviewAction.nothing, reason: '숨긴 뒤 도착');
    expect(m.point(a, cached: true), KClipPreviewAction.show);
    expect(m.point(b, cached: true), KClipPreviewAction.show);
  });

  test('그림·그림 파일 항목만 미리보기 대상', () {
    expect(kclipPreviewTarget({'id': 3, 'kind': 'image', 'size': 9}), const KClipPreviewTarget(3, 9));
    expect(kclipPreviewTarget({'id': 3, 'kind': 'text', 'preview': 'a.png'}), isNull);
    expect(kclipPreviewTarget({'id': 4, 'kind': 'files', 'preview': '캡처 · a.png', 'cats': ['전체', '그림', '파일', '화면캡처']}),
        const KClipPreviewTarget(4, -1));
    expect(kclipPreviewTarget({'id': 5, 'kind': 'files', 'preview': '파일 1개: 보고.hwpx'}), isNull);
    expect(kclipPreviewTarget({'id': 6, 'kind': 'files', 'preview': '파일 1개: 사진.jpg'}), isNotNull);
  });

  test('위치: 오른쪽 우선, 넘치면 왼쪽, 화면 안', () {
    const screen = Size(1000, 800);
    const box = Size(200, 100);
    expect(kclipPreviewPlace(right: 100, left: 100, top: 50, box: box, screen: screen), const Offset(112, 50));
    expect(kclipPreviewPlace(right: 900, left: 600, top: 50, box: box, screen: screen), const Offset(388, 50));
    expect(kclipPreviewPlace(right: 100, left: 100, top: 780, box: box, screen: screen), const Offset(112, 700));
  });

  testWidgets('앱 목록에서 그림 항목에 마우스를 올리면 미리보기가 뜨고 벗어나면 사라진다', (tester) async {
    final calls = <String>[];
    Future<Object?> api(String method, List<Object?> args) async {
      final action = args.first as String;
      calls.add(action);
      return switch (action) {
        'status' => {'ok': true, 'running': true, 'status': {'items': 2}},
        'list' => {
            'ok': true,
            'items': [
              {'id': 7, 'kind': 'files', 'preview': '캡처 · 스크린샷 1.png', 'size': 30,
                'cats': ['전체', '그림', '파일', '화면캡처']},
              {'id': 8, 'kind': 'text', 'preview': '보고서 제목', 'size': 12},
            ],
          },
        'settings_get' => {'ok': true, 'settings': {}},
        'thumb' => {'ok': true, 'id': 7, 'w': 1, 'h': 1, 'png': _png1x1},
        _ => {'ok': true},
      };
    }

    await tester.pumpWidget(MaterialApp(
      home: Scaffold(body: SizedBox(height: 760, child: KClipPanel(api: api))),
    ));
    await tester.pump(const Duration(milliseconds: 20));
    expect(find.text('캡처 · 스크린샷 1.png'), findsOneWidget);

    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await mouse.addPointer(location: Offset.zero);
    addTearDown(mouse.removePointer);
    await mouse.moveTo(tester.getCenter(find.text('캡처 · 스크린샷 1.png')));
    await tester.pump(const Duration(milliseconds: 20));
    expect(calls.where((c) => c == 'thumb'), hasLength(1));
    expect(find.byKey(const ValueKey('kclip-preview')), findsOneWidget);

    // 글 항목으로 옮기면 숨김, 다시 그림으로 오면 캐시로 바로(요청 없음).
    await mouse.moveTo(tester.getCenter(find.text('보고서 제목')));
    await tester.pump();
    expect(find.byKey(const ValueKey('kclip-preview')), findsNothing);
    await mouse.moveTo(tester.getCenter(find.text('캡처 · 스크린샷 1.png')));
    await tester.pump();
    expect(find.byKey(const ValueKey('kclip-preview')), findsOneWidget);
    expect(calls.where((c) => c == 'thumb'), hasLength(1));

    await mouse.moveTo(const Offset(1, 1));
    await tester.pump();
    expect(find.byKey(const ValueKey('kclip-preview')), findsNothing);
  });

  testWidgets('설정에 찾은 화면캡처 폴더를 보이고 폴더별로 끈다', (tester) async {
    final sets = <Map>[];
    Future<Object?> api(String method, List<Object?> args) async {
      final action = args.first as String;
      if (action == 'settings_set') sets.add((args[1] as Map)['settings'] as Map);
      return switch (action) {
        'status' => {'ok': true, 'running': true, 'status': {'items': 0}},
        'list' => {'ok': true, 'items': []},
        'settings_get' => {
            'ok': true,
            'settings': {'watch_screenshots': true, 'capture_dirs_off': []},
            'capture_dirs': [
              {'path': 'D:\\u\\Pictures\\Screenshots', 'label': 'Windows 스크린샷(캡처 도구·Win+PrtSc)', 'state': 'watching'},
              {'path': 'D:\\u\\Documents\\ALCapture', 'label': '알캡처', 'state': 'watching'},
            ],
          },
        _ => {'ok': true},
      };
    }

    await tester.pumpWidget(MaterialApp(
      home: Scaffold(body: SingleChildScrollView(child: SizedBox(height: 1200, child: KClipPanel(api: api)))),
    ));
    await tester.pump(const Duration(milliseconds: 20));
    expect(find.textContaining('알캡처 · 감시 중'), findsOneWidget);
    final box = find.byKey(const ValueKey('kclip-capture-dir-D:\\u\\Documents\\ALCapture'));
    await tester.ensureVisible(box);
    await tester.tap(box);
    await tester.pump(const Duration(milliseconds: 20));
    expect(sets.last, {'capture_dirs_off': ['d:\\u\\documents\\alcapture']});
  });
}
