import 'dart:io';

import 'package:docfit_ui/main.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter/rendering.dart';
import 'dart:ui' as ui;

const _font = 'assets/NotoSansKR.ttf';
const _screens = ['document', 'table_templates', 'library', 'timemachine', 'kclip'];

Future<void> _loadSnapshotFonts() async {
  final material = File(
    Platform.environment['FLUTTER_ROOT'] == null
        ? r'C:\Users\haiju\AppData\Local\DocFitTools\flutter\bin\cache\artifacts\material_fonts\materialicons-regular.otf'
        : '${Platform.environment['FLUTTER_ROOT']}\\bin\\cache\\artifacts\\material_fonts\\materialicons-regular.otf',
  );
  final materialBytes = await material.readAsBytes();
  await (FontLoader('MaterialIcons')
        ..addFont(Future<ByteData>.value(ByteData.sublistView(materialBytes))))
      .load();
  for (final (family, asset) in [
    ('NotoSansKR', _font),
    ('NotoSansSymbols2', 'assets/fonts/NotoSansSymbols2-Regular.ttf'),
    ('STIXTwoMath', 'assets/fonts/STIX2Math.otf'),
  ]) {
    final data = await rootBundle.load(asset);
    await (FontLoader(family)..addFont(Future<ByteData>.value(data))).load();
  }
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  setUpAll(_loadSnapshotFonts);
  final side = Platform.environment['UI_SNAPSHOT_SIDE'] ?? 'after';
  final output = Platform.environment['UI_SNAPSHOT_DIR'] ?? '../reports/ui';

  for (final screen in _screens) {
    testWidgets('capture $side $screen at 1280x800', (tester) async {
      tester.view.devicePixelRatio = 1;
      tester.view.physicalSize = const Size(1280, 800);
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);

      Future<Object?> api(String method, List<Object?> args) async {
        switch (method) {
          case 'get_state':
            return <String, Object?>{
              'api_version': 1,
              'ok': true,
              'data': <String, Object?>{
                'developer_mode': true,
                'sidebar_collapsed': false,
                'engine_ready': true,
                'can_start': false,
                'files': <Object?>[],
                'profiles': <Object?>[],
                'range': {'enabled': false, 'start': 1, 'end': 1},
              },
            };
          case 'get_settings':
            return <String, Object?>{
              'api_version': 1,
              'ok': true,
              'data': {
                'fields': <Object?>[],
                'values': {'ui_font_scale': 'normal'},
                'revision': 'snapshot',
              },
            };
          case 'kclip':
            return {'enabled': false, 'items': <Object?>[]};
          default:
            return {'api_version': 1, 'ok': true, 'data': <String, Object?>{}};
        }
      }

      await tester.pumpWidget(
        RepaintBoundary(
          key: const ValueKey('snapshot-boundary'),
          child: MaterialApp(home: DocFitApp(api: api)),
        ),
      );
      await tester.pump(const Duration(milliseconds: 100));
      await tester.pump(const Duration(milliseconds: 100));

      if (screen == 'table_templates') {
        await tester.tap(find.byKey(const ValueKey('workspaceTab-1')));
      } else if (screen == 'library') {
        await tester.tap(find.byKey(const ValueKey('library-open')));
      } else if (screen == 'timemachine') {
        await tester.tap(find.byKey(const ValueKey('workspaceTab-7')));
      } else if (screen == 'kclip') {
        await tester.tap(find.byKey(const ValueKey('kclip-open')));
      }
      await tester.pump(const Duration(milliseconds: 100));
      await tester.pump(const Duration(milliseconds: 100));
      final png = await tester.runAsync<Uint8List>(() async {
        final boundary = tester.renderObject<RenderRepaintBoundary>(
          find.byKey(const ValueKey('snapshot-boundary')),
        );
        final image = await boundary.toImage(pixelRatio: 1);
        final bytes = await image.toByteData(format: ui.ImageByteFormat.png);
        return bytes!.buffer.asUint8List();
      });
      await tester.runAsync(() async {
        await Directory(output).create(recursive: true);
        await File('$output/${side}_$screen.png').writeAsBytes(png!);
      });
    });
  }
}
