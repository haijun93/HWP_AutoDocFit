import 'package:flutter/gestures.dart' show kSecondaryButton;
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:docfit_ui/features/common/file_handoff.dart';
import 'package:docfit_ui/features/workspace/tab_navigation_shell.dart';
import 'package:docfit_ui/main.dart';

import 'workspace_test.dart' show initialState;

const _paths = [
  r'C:\synthetic\sheet.XLS',
  r'C:\synthetic\slides.pptx',
  r'C:\synthetic\document.hml',
  r'C:\synthetic\unsupported.ppt',
];

class _ContextApi {
  final state = initialState();
  final added = <List<String>>[];

  Future<Object?> call(String method, List<Object?> args) async {
    switch (method) {
      case 'get_state':
        return Map<String, dynamic>.from(state);
      case 'set_mode':
        state['mode'] = args.single;
        return Map<String, dynamic>.from(state);
      case 'add_paths':
        final paths = (args.single as List).cast<String>();
        added.add(paths);
        state['files'] = [
          for (final path in paths)
            {
              'path': path,
              'name': path.split(r'\').last,
              'folder': r'C:\synthetic',
            },
        ];
        return Map<String, dynamic>.from(state);
      case 'docview_window':
      case 'conversion_window':
        return {
          'ok': true,
          'data': {'mode': 'flutter'},
        };
      case 'doc_grid':
        if (args.first == 'list_default') {
          return {
            'ok': true,
            'documents': [
              for (final path in _paths)
                {
                  'path': path,
                  'name': path.split(r'\').last,
                  'ext': path.split('.').last.toLowerCase(),
                  'size': 1,
                },
            ],
          };
        }
        return {'ok': true};
      default:
        return {'ok': true};
    }
  }
}

Future<void> _settle(WidgetTester tester) async {
  for (var i = 0; i < 12; i++) {
    await tester.pump(const Duration(milliseconds: 200));
  }
}

Finder _menu(String id) => find.byKey(ValueKey('fileMenu-$id'));
Finder _doc(String path) => find.byKey(ValueKey('docCard-$path'));

Future<_ContextApi> _start(WidgetTester tester) async {
  tester.view.physicalSize = const Size(1600, 1200);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  final api = _ContextApi();
  await tester.pumpWidget(DocFitApp(api: api.call));
  await _settle(tester);
  await tester.tap(find.byKey(Key('workspaceTab-$kTabDocGrid')));
  await _settle(tester);
  addTearDown(() async => tester.pumpWidget(const SizedBox()));
  return api;
}

void main() {
  for (final target in [FileTarget.documentEditor, FileTarget.conversion]) {
    for (final path in _paths.take(3)) {
      testWidgets('${path.split('.').last} → ${target.name}: 메뉴·대상 목록 연결', (
        tester,
      ) async {
        final api = await _start(tester);
        await tester.tap(_doc(path), buttons: kSecondaryButton);
        await tester.pumpAndSettle();
        expect(
          tester.widget<PopupMenuItem<String>>(_menu(target.name)).enabled,
          isTrue,
        );
        await tester.tap(_menu(target.name));
        await _settle(tester);
        expect(api.added.single, [path]);
        expect(find.text(path.split(r'\').last).hitTestable(), findsOneWidget);
        if (target == FileTarget.conversion) {
          expect(
            find.byKey(const Key('conversionRemove-0')).hitTestable(),
            findsOneWidget,
          );
        } else {
          expect(find.text('선택한 문서 1개'), findsOneWidget);
        }
      });
    }
  }

  testWidgets('PPT를 섞어 선택하면 두 메뉴가 비활성이고 이유를 보인다', (tester) async {
    final api = await _start(tester);
    for (final path in [_paths.first, _paths.last]) {
      await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
      await tester.tap(_doc(path));
      await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
      await tester.pump(const Duration(milliseconds: 500));
    }
    await tester.tap(_doc(_paths.last), buttons: kSecondaryButton);
    await tester.pumpAndSettle();
    for (final target in [FileTarget.documentEditor, FileTarget.conversion]) {
      expect(
        tester.widget<PopupMenuItem<String>>(_menu(target.name)).enabled,
        isFalse,
      );
      expect(
        fileTargetReason(target, [_paths.first, _paths.last]),
        contains('.ppt'),
      );
    }
    expect(find.text('지원하지 않는 형식이 있습니다(.ppt)'), findsNWidgets(2));
    expect(api.added, isEmpty);
  });

  test('지원 입력 혼합·대문자 확장자 허용, PPT·DOC 변환 제한 보존', () {
    for (final target in [FileTarget.documentEditor, FileTarget.conversion]) {
      expect(fileTargetReason(target, _paths.take(3).toList()), isNull);
      expect(fileTargetReason(target, [_paths.last]), contains('.ppt'));
    }
    expect(
      fileTargetReason(FileTarget.conversion, [r'C:\synthetic\old.doc']),
      contains('.doc'),
    );
  });
}
