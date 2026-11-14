// 쉬는 시간 색인(IDLE1) 상태 줄 글자. 서비스 content_status 응답(trigger·processed·total·yielded·idle)만 읽는다.

/// 유휴 기준 시간 선택지(초). 서비스 허용 범위는 15~3600초.
const List<int> idleSecondsChoices = [30, 60, 120, 300, 600];

String idleSecondsLabel(int seconds) =>
    seconds < 60 ? '$seconds초' : (seconds % 60 == 0 ? '${seconds ~/ 60}분' : '$seconds초');

/// 쉬는 시간에 자동으로 도는 색인이면 상태 줄 글자, 아니면 null(기존 글자를 그대로 씀).
String? idleIndexingStatusText(Map<String, Object?> status) {
  if (status['trigger'] != 'idle') return null;
  final done = (status['processed'] as num?)?.toInt() ?? 0;
  final total = (status['total'] as num?)?.toInt();
  final count = total == null ? '목록 확인 중' : '$done/$total';
  if (status['yielded'] == true) {
    final idle = status['idle'];
    final reason = idle is Map ? idle['reason_text'] : null;
    final why = reason is String && reason.isNotEmpty && reason != '쉬는 시간' ? ' · $reason' : '';
    return '쉬는 시간 색인 잠시 멈춤$why $count';
  }
  return '쉬는 시간 색인 중 $count';
}

/// 쉬는 시간 색인 진행률(0~1). 전체 수를 모르면 null(움직이는 막대).
double? idleIndexingProgress(Map<String, Object?> status) {
  if (status['trigger'] != 'idle') return null;
  final total = (status['total'] as num?)?.toInt();
  if (total == null || total <= 0) return null;
  final done = (status['processed'] as num?)?.toInt() ?? 0;
  return (done / total).clamp(0.0, 1.0);
}
