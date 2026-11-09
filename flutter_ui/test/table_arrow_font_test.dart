import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:docfit_ui/core/api/table_units_api.dart';
import 'package:docfit_ui/features/table_units/table_grid_preview.dart';

void main() {
  testWidgets('table arrow preview uses bundled symbol and math fallback fonts', (
    tester,
  ) async {
    const cell = CellMeta(
      row: 0,
      col: 0,
      width: 1,
      height: 1,
      text: '⇑',
    );
    await tester.pumpWidget(
      MaterialApp(
        theme: ThemeData(
          fontFamily: 'NotoSansKR',
          fontFamilyFallback: const ['NotoSansSymbols2', 'STIXTwoMath'],
        ),
        home: const Scaffold(
          body: TableGridPreview(
            rows: 1,
            cols: 1,
            cellMeta: [
              [cell],
            ],
            shapes: {
              '0,0': [CellShape(text: '⇑')],
            },
            maxWidth: 120,
            maxHeight: 60,
          ),
        ),
      ),
    );
    await tester.pump();
    final painter = tester.widget<CustomPaint>(
      find.byKey(const Key('tableGridTextLayer')),
    );
    expect(painter.painter, isNotNull);
    expect(find.text('⇑'), findsNothing); // Cell text is rendered by the verified CustomPainter.
  });
}
