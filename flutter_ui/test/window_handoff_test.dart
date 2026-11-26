import 'package:flutter/gestures.dart' show kSecondaryButton;
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:docfit_ui/main.dart';

import 'workspace_test.dart' show initialState;

// RUSTSIDE2: 메뉴 하나만 담은 창끼리 파일 넘기기와 닫기 보호. 실제 파일은 없다.

const _png =
    'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==';
const _pdf = r'C:\d\alpha.pdf';

class _Api {
  final sent = <Map<String, Object?>>[];
  int takes = 0;
  List<String> incoming = [];
  final pdfInfo = <String>[];
  Map<String, Object?>? pending;
  final answers = <bool>[];

  Future<Object?> call(String method, List<Object?> args) async {
    switch (method) {
      case 'get_state':
        return {...initialState(), 'pending_dialog': pending};
      case 'get_pending_dialog':
        return pending;
      case 'resolve_dialog':
        answers.add(args[1] as bool);
        pending = null;
        return {'accepted': args[1]};
      case 'handoff':
        final payload = Map<String, Object?>.from(args.single as Map);
        if (payload['action'] == 'send') {
          sent.add(payload);
          return {'ok': true, 'data': {'sent': 1}};
        }
        takes++;
        final paths = incoming;
        incoming = [];
        return {'ok': true, 'data': {'paths': paths, 'photo_paste': false, 'rejected': 0}};
      case 'pdf_edit':
        final payload = args.single as Map;
        if (payload['action'] == 'info') {
          pdfInfo.add('${payload['source']}');
          return {'ok': true, 'data': {'pages': [1, 2]}};
        }
        return {'ok': true, 'data': {'data': _png}};
      case 'doc_grid':
        if (args.first == 'list_default') {
          return {
            'ok': true,
            'documents': [
              {'path': _pdf, 'name': 'alpha.pdf', 'ext': 'pdf', 'size': 1},
            ],
          };
        }
        return {'ok': true, 'data': _png};
      case 'docview_window':
        return {'ok': true, 'data': {'mode': 'flutter'}};
      case 'library':
        final cmd = (args.single as Map)['cmd'];
        if (cmd == 'box_list') {
          return {'ok': true, 'boxes': [{'id': 1, 'name': '기존 폴더'}]};
        }
        return {'ok': true, 'items': <Object>[], 'relations': <Object>[], 'paths': <String>[]};
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

Future<_Api> _solo(WidgetTester tester, int? screen) async {
  tester.view.physicalSize = const Size(1600, 1200);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  final api = _Api();
  await tester.pumpWidget(DocFitApp(api: api.call, soloTab: screen));
  await _settle(tester);
  addTearDown(() async => tester.pumpWidget(const SizedBox()));
  return api;
}

Future<void> _requestClose(WidgetTester tester, _Api api, String id) async {
  api.pending = {'id': id, 'kind': 'app_close'};
  await tester.pump(const Duration(seconds: 1));
  await _settle(tester);
}

Future<void> _editPdf(WidgetTester tester, _Api api) async {
  api.incoming = [_pdf];
  await _settle(tester);
  await tester.tap(find.text('쪽 1').first);
  await _settle(tester);
  await tester.tap(find.byTooltip('쪽 삭제').first);
  await _settle(tester);
}

void main() {
  testWidgets('문서뷰 전용 창에서 PDF 편집을 고르면 그 창으로 보낸다', (tester) async {
    final api = await _solo(tester, 8);
    await tester.tap(find.byKey(const ValueKey('docCard-$_pdf')), buttons: kSecondaryButton);
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('fileMenu-pdfEdit')));
    await _settle(tester);
    expect(api.sent, [
      {'action': 'send', 'menu': 11, 'paths': [_pdf]},
    ]);
    expect(find.text('파일 1개를 보냈습니다.'), findsOneWidget);
    expect(api.takes, 0, reason: '문서뷰 창은 파일을 받지 않는다');
  });

  testWidgets('PDF 편집 전용 창은 온 파일을 연다', (tester) async {
    final api = await _solo(tester, 11);
    expect(api.takes, greaterThan(0));
    api.incoming = [_pdf];
    await _settle(tester);
    expect(api.pdfInfo, [_pdf]);
    expect(api.sent, isEmpty);
  });

  testWidgets('가상 폴더 전용 창은 열린 뒤에 온 파일도 넣을 곳을 묻는다', (tester) async {
    final api = await _solo(tester, 12);
    api.incoming = [_pdf];
    await _settle(tester);
    expect(find.byKey(const ValueKey('boxPickDialog')), findsOneWidget);
  });

  testWidgets('보통 앱은 수신함을 보지 않는다', (tester) async {
    final api = await _solo(tester, null);
    expect(api.takes, 0);
  });

  testWidgets('저장 안 한 PDF 편집이 있으면 닫기 전에 묻는다', (tester) async {
    final api = await _solo(tester, 11);
    await _editPdf(tester, api);

    await _requestClose(tester, api, 'a');
    expect(find.byKey(const ValueKey('closeUnsavedDialog')), findsOneWidget);
    expect(find.textContaining('PDF 편집 내용'), findsOneWidget);
    await tester.tap(find.text('계속 작업'));
    await _settle(tester);
    expect(api.answers, [false]);

    await _requestClose(tester, api, 'b');
    await tester.tap(find.text('버리고 닫기'));
    await _settle(tester);
    expect(api.answers, [false, true]);
  });

  testWidgets('잃을 것이 없으면 묻지 않고 닫는다', (tester) async {
    final api = await _solo(tester, 11);
    await _requestClose(tester, api, 'c');
    expect(find.byKey(const ValueKey('closeUnsavedDialog')), findsNothing);
    expect(api.answers, [true]);
  });
}
