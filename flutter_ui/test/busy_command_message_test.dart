import 'package:flutter_test/flutter_test.dart';
import 'package:docfit_ui/main.dart';

void main() {
  test('busy responses tell the user to retry after the current work', () {
    expect(
      commandFailureMessage(Exception('앱이 바쁨: 진행 중')),
      '앱이 바쁩니다. 진행 중인 작업이 끝난 뒤 다시 시도해 주세요.',
    );
  });
}
