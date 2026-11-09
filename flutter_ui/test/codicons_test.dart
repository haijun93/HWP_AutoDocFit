import 'package:flutter_test/flutter_test.dart';
import 'package:docfit_ui/core/codicons.dart';

void main() {
  test('Codicons font family and codePoint verification', () {
    expect(Codicons.save.fontFamily, 'Codicon');
    expect(Codicons.save.codePoint, 0xEB4B);
    expect(Codicons.folder.fontFamily, 'Codicon');
    expect(Codicons.folder.codePoint, 0xEA83);
    expect(Codicons.fileText.fontFamily, 'Codicon');
    expect(Codicons.fileText.codePoint, 0xEC5E);
    expect(Codicons.table.fontFamily, 'Codicon');
    expect(Codicons.table.codePoint, 0xEBB7);
    expect(Codicons.menu.fontFamily, 'Codicon');
    expect(Codicons.menu.codePoint, 0xEB94);
    expect(Codicons.gear.fontFamily, 'Codicon');
    expect(Codicons.gear.codePoint, 0xEAF8);
  });
}
