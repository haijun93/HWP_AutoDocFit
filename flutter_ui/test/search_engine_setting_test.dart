import 'package:docfit_ui/features/settings/settings_dialog.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('search engine offers and saves legacy choice', (tester) async {
    List<Object?>? saved;
    Future<Object?> api(String method, List<Object?> args) async {
      if (method == 'save_settings') saved = args;
      return {'api_version':1, 'ok':true, 'data': {
        'revision':'r1', 'running':false,
        'fields':[{'key':'search_engine', 'label':'검색 엔진', 'type':'choice',
          'choices':['kfind','legacy'],
          'choice_labels':{'kfind':'새 검색 엔진 (기본)','legacy':'기존 검색 엔진'}}],
        'values':{'search_engine':'kfind'},
      }};
    }
    await tester.pumpWidget(MaterialApp(home: Scaffold(body: SettingsDialog(api: api))));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('search_engine')));
    await tester.pumpAndSettle();
    await tester.tap(find.text('기존 검색 엔진').last);
    await tester.pumpAndSettle();
    await tester.tap(find.text('변경사항 적용'));
    await tester.pumpAndSettle();
    expect(saved, [{'search_engine':'legacy'}, 'r1']);
  });
}
