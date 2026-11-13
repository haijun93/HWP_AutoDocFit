import 'package:flutter_test/flutter_test.dart';
import 'package:docfit_ui/features/timemachine/idle_indexing_status.dart';

void main() {
  test('수동·자동(곧바로) 색인은 기존 글자를 쓴다', () {
    expect(idleIndexingStatusText({'trigger': 'manual', 'processed': 1, 'total': 2}), isNull);
    expect(idleIndexingStatusText({'trigger': 'auto'}), isNull);
    expect(idleIndexingStatusText({}), isNull);
    expect(idleIndexingProgress({'trigger': 'manual', 'processed': 1, 'total': 2}), isNull);
  });

  test('쉬는 시간 색인 중 n/m', () {
    expect(idleIndexingStatusText({'trigger': 'idle', 'processed': 3, 'total': 10}), '쉬는 시간 색인 중 3/10');
    expect(idleIndexingProgress({'trigger': 'idle', 'processed': 3, 'total': 10}), closeTo(0.3, 1e-9));
  });

  test('전체 수를 모르면 목록 확인 중, 막대는 움직임', () {
    expect(idleIndexingStatusText({'trigger': 'idle', 'processed': 0, 'total': null}), '쉬는 시간 색인 중 목록 확인 중');
    expect(idleIndexingProgress({'trigger': 'idle', 'total': null}), isNull);
    expect(idleIndexingProgress({'trigger': 'idle', 'total': 0}), isNull);
  });

  test('양보 중이면 멈춤과 이유', () {
    final status = {
      'trigger': 'idle', 'processed': 1, 'total': 4, 'yielded': true,
      'idle': {'state': 'active', 'reason_text': '사용 중'},
    };
    expect(idleIndexingStatusText(status), '쉬는 시간 색인 잠시 멈춤 · 사용 중 1/4');
    expect(idleIndexingStatusText({...status, 'idle': null}), '쉬는 시간 색인 잠시 멈춤 1/4');
  });

  test('유휴 기준 글자', () {
    expect(idleSecondsLabel(30), '30초');
    expect(idleSecondsLabel(120), '2분');
    expect(idleSecondsLabel(90), '90초');
    expect(idleSecondsChoices, contains(120));
  });
}
