import 'dart:async';
import 'dart:io';

import 'package:docfit_ui/features/outliner/outliner_ui_gate.dart';
import 'package:docfit_ui/features/outliner/outliner_workspace.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

/// OUTPANEL1: 아웃라이너 왼쪽·오른쪽 패널과 역링크·역참조 접기.

class _Recent implements RecentPagesStore {
  @override
  Future<List<String>> load() async => [];
  @override
  Future<void> save(List<String> pages) async {}
}

class _Panels implements OutlinerPanelStore {
  _Panels([Map<String, bool>? initial]) : data = {...?initial};
  Map<String, bool> data;
  final saves = <Map<String, bool>>[];
  Completer<void>? gate; // 있으면 읽기를 늦춘다.

  @override
  Future<Map<String, bool>> load() async {
    final snapshot = Map<String, bool>.of(data);
    await gate?.future;
    return snapshot;
  }

  @override
  Future<void> save(Map<String, bool> collapsed) async {
    data = Map.of(collapsed);
    saves.add(Map.of(collapsed));
  }
}

class _BrokenPanels implements OutlinerPanelStore {
  @override
  Future<Map<String, bool>> load() async => throw const FormatException('x');
  @override
  Future<void> save(Map<String, bool> collapsed) async =>
      throw const FileSystemException('x');
}

class _Api {
  _Api({this.backlinksFail = false, this.openFails = false});
  final bool backlinksFail;
  final bool openFails;
  final actions = <String>[];

  static const _blocks = [
    {'id': 1, 'text': 'one', 'indent': 0, 'folded': false},
    {'id': 2, 'text': 'two', 'indent': 1, 'folded': false},
  ];

  Future<Object?> call(String method, [List<Object?> args = const []]) async {
    final p = args.first as Map;
    final action = p['action'] as String;
    actions.add(action);
    Map ok(Map data) => {'ok': true, 'data': data};
    switch (action) {
      case 'ui_mode':
        return ok({'mode': 'flutter'});
      case 'open_native':
        return openFails
            ? {'ok': false, 'error': {'message': '창 프로그램을 찾을 수 없습니다.'}}
            : ok({});
      case 'list':
        return ok({'pages': [{'name': 'Home'}]});
      case 'daily':
      case 'load':
        return ok({
          'page': 'Today',
          'markdown': '- one\n  - two\n',
          'version': 'v1',
          'document': {'blocks': _blocks},
        });
      case 'backlinks':
        if (backlinksFail) return {'ok': false};
        return ok({'block_ids': [7, 8, 9]});
      case 'block_backlinks':
        return ok({'backlinks': [4]});
      case 'key':
        return ok({'handled': false});
    }
    return ok({});
  }
}

void _size(WidgetTester tester, double width, [double height = 900]) {
  tester.view.physicalSize = Size(width, height);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
}

Future<void> _pump(
  WidgetTester tester,
  _Api api, {
  OutlinerPanelStore? store,
  double width = 1400,
}) async {
  _size(tester, width);
  await tester.pumpWidget(const SizedBox());
  await tester.pumpWidget(
    MaterialApp(
      home: Scaffold(
        body: OutlinerWorkspace(
          api: api.call,
          recentPagesStore: _Recent(),
          panelStore: store,
        ),
      ),
    ),
  );
  await tester.pumpAndSettle();
}

const _left = ValueKey('outliner-left-toggle');
const _right = ValueKey('outliner-right-toggle');
const _backlinks = ValueKey('outliner-backlinks-header');
const _blockRefs = ValueKey('outliner-blockRefs-header');

Future<void> _tap(WidgetTester tester, Key key) async {
  await tester.tap(find.byKey(key));
  await tester.pumpAndSettle();
}

double _bodyWidth(WidgetTester tester) =>
    tester.getSize(find.byKey(const ValueKey('outliner-body'))).width;

bool _leftOpen() => find.text('오늘의 일지').evaluate().isNotEmpty;
bool _rightOpen() => find.text('역링크').evaluate().isNotEmpty;

String? _count(WidgetTester tester, String key) {
  final finder = find.byKey(ValueKey('outliner-$key-count'));
  return finder.evaluate().isEmpty ? null : tester.widget<Text>(finder).data;
}

void main() {
  testWidgets('양쪽 패널을 접으면 본문이 그만큼 넓어지고 펴는 단추가 남는다', (tester) async {
    final api = _Api();
    await _pump(tester, api);
    final wide = _bodyWidth(tester);
    final calls = api.actions.length;

    await _tap(tester, _right);
    expect(_rightOpen(), isFalse);
    expect(find.byTooltip('오른쪽 패널 펴기'), findsOneWidget);
    expect(_bodyWidth(tester), wide + 220 - 36);

    await _tap(tester, _left);
    expect(_leftOpen(), isFalse);
    expect(find.byTooltip('왼쪽 패널 펴기'), findsOneWidget);
    expect(_bodyWidth(tester), wide + 220 - 36 + 248 - 36);

    await _tap(tester, _right);
    await _tap(tester, _left);
    expect(_leftOpen() && _rightOpen(), isTrue);
    expect(_bodyWidth(tester), wide);
    expect(api.actions.length, calls, reason: '접고 펴기는 엔진을 부르지 않는다');
  });

  testWidgets('접은 채로 둔 왼쪽 패널의 검색어는 다시 펴도 남는다', (tester) async {
    await _pump(tester, _Api());
    final search = find.widgetWithText(TextField, '페이지·블록 검색');
    await tester.enterText(search, 'ho');
    await _tap(tester, _left);
    await _tap(tester, _left);
    expect(find.text('ho'), findsOneWidget);
  });

  testWidgets('역링크·역참조를 제목으로 각각 접고 펴며 읽은 개수는 접어도 보인다', (tester) async {
    final api = _Api();
    await _pump(tester, api);
    expect(_count(tester, 'backlinks'), '3');
    expect(_count(tester, 'blockRefs'), isNull, reason: '선택한 블록이 없으면 읽지 않았다');
    final calls = api.actions.length;

    await _tap(tester, _backlinks);
    expect(find.text('블록 7'), findsNothing);
    expect(find.text('선택된 블록이 없습니다.'), findsOneWidget);
    expect(_count(tester, 'backlinks'), '3');
    await _tap(tester, _blockRefs);
    expect(find.text('선택된 블록이 없습니다.'), findsNothing);
    expect(api.actions.length, calls);

    // 접은 채 블록을 선택해도 조회는 원래대로이고, 읽은 개수가 제목에 보인다.
    await tester.tap(find.byKey(const ValueKey('outliner-block-1')));
    await tester.pump();
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pumpAndSettle();
    expect(_count(tester, 'blockRefs'), '1');
    expect(find.text('블록 4 에서 참조'), findsNothing);
    await _tap(tester, _blockRefs);
    expect(find.text('블록 4 에서 참조'), findsOneWidget);
    await _tap(tester, _backlinks);
    expect(find.text('블록 7'), findsOneWidget);
  });

  testWidgets('오른쪽 패널을 접어도 펼친 때와 같은 조회를 하고 펴면 바로 보인다', (tester) async {
    final open = _Api();
    await _pump(tester, open, store: _Panels({'right': false}));
    final closed = _Api();
    await _pump(tester, closed, store: _Panels({'right': true}));
    expect(closed.actions, open.actions);
    final calls = closed.actions.length;
    await _tap(tester, _right);
    expect(find.text('블록 7'), findsOneWidget);
    expect(closed.actions.length, calls);
  });

  testWidgets('조회가 거절되면 개수를 지어내지 않는다', (tester) async {
    await _pump(tester, _Api(backlinksFail: true));
    expect(_count(tester, 'backlinks'), isNull);
  });

  testWidgets('누른 것만 저장하고 다시 열면 복원한다', (tester) async {
    final store = _Panels();
    await _pump(tester, _Api(), store: store);
    expect(store.saves, isEmpty);
    await _tap(tester, _left);
    expect(store.data, {'left': true});
    await _tap(tester, _backlinks);
    expect(store.data, {'left': true, 'backlinks': true});

    await _pump(tester, _Api(), store: store);
    expect(_leftOpen(), isFalse);
    expect(_rightOpen(), isTrue);
    expect(find.text('블록 7'), findsNothing);
    expect(find.text('선택된 블록이 없습니다.'), findsOneWidget);
  });

  testWidgets('저장 값을 읽기 전에 누른 것이 이기고 저장해 둔 다른 값은 지우지 않는다', (tester) async {
    final store = _Panels({'left': false, 'blockRefs': true})
      ..gate = Completer<void>();
    await _pump(tester, _Api(), store: store);
    await _tap(tester, _left);
    expect(store.saves, isEmpty);
    store.gate!.complete();
    await tester.pumpAndSettle();
    expect(_leftOpen(), isFalse);
    expect(store.data, {'left': true, 'blockRefs': true});
  });

  testWidgets('좁으면 저장 값이 없을 때만 오른쪽을 접은 채 시작한다', (tester) async {
    // 본문 = 폭 - 248 - 220 - 선 2. 890 에서 420.
    await _pump(tester, _Api(), store: _Panels(), width: 889);
    expect(_rightOpen(), isFalse);
    expect(_leftOpen(), isTrue);

    await _pump(tester, _Api(), store: _Panels(), width: 890);
    expect(_rightOpen(), isTrue);
    expect(_bodyWidth(tester), 420);

    await _pump(tester, _Api(), store: _Panels({'right': false}), width: 700);
    expect(_rightOpen(), isTrue);

    await _pump(tester, _Api(), store: _Panels({'right': true}), width: 1500);
    expect(_rightOpen(), isFalse);

    // 왼쪽을 접어 둔 사람은 그만큼 본문이 넓으므로 오른쪽을 접지 않는다.
    await _pump(tester, _Api(), store: _Panels({'left': true}), width: 700);
    expect(_rightOpen(), isTrue);
  });

  testWidgets('좁은 시작 값은 저장하지 않고 창 크기를 따라 바뀌지 않는다', (tester) async {
    final store = _Panels();
    await _pump(tester, _Api(), store: store, width: 700);
    tester.view.physicalSize = const Size(1500, 900);
    await tester.pumpAndSettle();
    expect(_rightOpen(), isFalse);
    await _tap(tester, _left);
    expect(store.data, {'left': true});
  });

  testWidgets('저장소 읽기·쓰기가 실패해도 화면은 그대로 움직인다', (tester) async {
    await _pump(tester, _Api(), store: _BrokenPanels());
    await _tap(tester, _right);
    await _tap(tester, _right);
    await _tap(tester, _left);
    expect(_rightOpen(), isTrue);
    expect(_leftOpen(), isFalse);
    expect(find.text('one'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  test('파일 저장소는 주어진 폴더만 쓰고 모르는 값은 버린다', () async {
    final root = await Directory.systemTemp.createTemp('outliner-panels-');
    addTearDown(() => root.delete(recursive: true));
    final directory = Directory('${root.path}/data');
    final store = FileOutlinerPanelStore(() async => directory);
    await expectLater(store.load(), throwsA(isA<FileSystemException>()));
    await store.save({'left': true, 'right': false});
    expect(await store.load(), {'left': true, 'right': false});
    final file = File('${directory.path}/outliner_panels.json');
    await file.writeAsString('{"left":"yes","backlinks":true,"other":true}');
    expect(await store.load(), {'backlinks': true});
    await file.writeAsString('[1]');
    expect(await store.load(), isEmpty);
  });

  testWidgets('접고 펴도 편집 중인 글·커서·포커스가 남고 저장 명령도 없다', (tester) async {
    final api = _Api();
    await _pump(tester, api, store: _Panels());
    final field = find.byKey(const ValueKey('outliner-block-1'));
    await tester.tap(field, kind: PointerDeviceKind.mouse);
    await tester.pumpAndSettle();
    await tester.enterText(field, '확정 전 글');
    EditableText editable() => tester.widget<EditableText>(
      find.descendant(of: field, matching: find.byType(EditableText)),
    );
    editable().controller.selection = const TextSelection(
      baseOffset: 1,
      extentOffset: 3,
    );
    await tester.pump();
    final state = tester.state(
      find.descendant(of: field, matching: find.byType(EditableText)),
    );
    final calls = api.actions.length;

    for (final key in [
      _right, _right, _left, _left, _backlinks, _backlinks, _blockRefs,
      _blockRefs, _left, _right, //
    ]) {
      await tester.tap(find.byKey(key), kind: PointerDeviceKind.mouse);
      await tester.pumpAndSettle();
      expect(editable().controller.text, '확정 전 글', reason: '$key');
      expect(
        editable().controller.selection,
        const TextSelection(baseOffset: 1, extentOffset: 3),
        reason: '$key',
      );
      expect(editable().focusNode.hasFocus, isTrue, reason: '$key');
    }
    expect(
      tester.state(
        find.descendant(of: field, matching: find.byType(EditableText)),
      ),
      same(state),
    );
    expect(api.actions.length, calls);
  }, variant: TargetPlatformVariant.only(TargetPlatform.windows));

  testWidgets('폭 700·1100·1500 의 모든 접힘 조합에서 넘치지 않는다', (tester) async {
    for (final width in [700.0, 1100.0, 1500.0]) {
      for (var mask = 0; mask < 4; mask++) {
        await _pump(
          tester,
          _Api(),
          store: _Panels({'left': mask & 1 != 0, 'right': mask & 2 != 0}),
          width: width,
        );
        expect(tester.takeException(), isNull, reason: '$width/$mask');
      }
    }
  });

  group('문지기', () {
    Future<void> pumpGate(
      WidgetTester tester,
      _Api api, {
      OutlinerPanelStore? panels,
      double width = 1100,
    }) async {
      _size(tester, width);
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: OutlinerUiGate(
              api: api.call,
              recentPagesStore: _Recent(),
              panelStore: panels,
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
    }

    const open = ValueKey('outliner-open-native');

    testWidgets('새 창 단추는 제목 줄의 아이콘이고 별도 줄을 만들지 않는다', (tester) async {
      await pumpGate(tester, _Api(), panels: _Panels());
      expect(find.byTooltip('새 창(Win32)으로 열기'), findsOneWidget);
      expect(find.text('새 창(Win32)으로 열기'), findsNothing);
      expect(tester.getTopLeft(find.byType(OutlinerWorkspace)).dy, 0);
      expect(
        tester.getCenter(find.byKey(open)).dy,
        tester.getCenter(find.byTooltip('새 블록')).dy,
      );
      expect(tester.takeException(), isNull);
    });

    testWidgets('창을 못 열면 이유를 제목 줄 아래에 보이고 화면을 유지한다', (tester) async {
      final api = _Api(openFails: true);
      await pumpGate(tester, api, panels: _Panels(), width: 700);
      await _tap(tester, open);
      expect(find.textContaining('창 프로그램을 찾을 수 없습니다.'), findsOneWidget);
      expect(find.text('one'), findsOneWidget);
      expect(tester.takeException(), isNull);
    });

    testWidgets('저장소를 넣지 않으면 아웃라이너 데이터 폴더의 파일을 쓴다', (tester) async {
      // 실제 사용자 파일은 건드리지 않는다(File 생성을 막는다).
      await IOOverrides.runZoned(
        () => pumpGate(tester, _Api(), width: 700),
        createFile: (path) => throw FileSystemException('시험에서 막음', path),
      );
      final workspace = tester.widget<OutlinerWorkspace>(
        find.byType(OutlinerWorkspace),
      );
      expect(workspace.panelStore, isA<FileOutlinerPanelStore>());
      expect(_rightOpen(), isFalse);
    });
  });
}
