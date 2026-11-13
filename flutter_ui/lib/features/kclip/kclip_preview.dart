import 'dart:math' as math;
import 'dart:ui' show Offset, Size;

import 'kclip_category.dart';

/// 몽땅클립보드 그림 미리보기 상태 기계 — 빠른 목록(kagent `preview.rs`의 `step`)과 같은 규칙.
///
/// | 지금 \ 사건 | 가리킴 없음 | 그림 t(캐시 있음) | 그림 t(캐시 없음) | 준비(t) | 숨기기 |
/// |---|---|---|---|---|---|
/// | hidden | 그대로 | showing·보이기 | waiting·요청 | 무시 | 그대로 |
/// | waiting(u) | hidden·숨기기 | u==t 그대로 / showing·보이기 | u==t 그대로 / waiting·요청 | u==t: 성공 showing, 실패 hidden / 무시 | hidden |
/// | showing(u) | hidden·숨기기 | u==t 옮기기 / showing·보이기 | u==t 옮기기 / waiting·요청 | 무시 | hidden·숨기기 |
enum KClipPreviewPhase { hidden, waiting, showing }

enum KClipPreviewAction { nothing, request, show, move, hide }

class KClipPreviewTarget {
  const KClipPreviewTarget(this.id, this.size);
  final int id;
  final int size;

  /// 썸네일 캐시 키(번호가 다시 쓰여도 크기가 다르면 다른 그림).
  String get key => '$id/$size';

  @override
  bool operator ==(Object other) => other is KClipPreviewTarget && other.id == id && other.size == size;

  @override
  int get hashCode => Object.hash(id, size);

  @override
  String toString() => 'KClipPreviewTarget($id, $size)';
}

class KClipPreviewMachine {
  KClipPreviewPhase phase = KClipPreviewPhase.hidden;
  KClipPreviewTarget? target;

  KClipPreviewAction point(KClipPreviewTarget? next, {required bool cached}) {
    if (next == null) return hide();
    if (target == next && phase == KClipPreviewPhase.showing) return KClipPreviewAction.move;
    if (target == next && phase == KClipPreviewPhase.waiting) return KClipPreviewAction.nothing;
    target = next;
    phase = cached ? KClipPreviewPhase.showing : KClipPreviewPhase.waiting;
    return cached ? KClipPreviewAction.show : KClipPreviewAction.request;
  }

  KClipPreviewAction ready(KClipPreviewTarget done, {required bool ok}) {
    if (phase != KClipPreviewPhase.waiting || target != done) return KClipPreviewAction.nothing;
    if (ok) {
      phase = KClipPreviewPhase.showing;
      return KClipPreviewAction.show;
    }
    phase = KClipPreviewPhase.hidden;
    target = null;
    return KClipPreviewAction.hide;
  }

  KClipPreviewAction hide() {
    final was = phase;
    phase = KClipPreviewPhase.hidden;
    target = null;
    return was == KClipPreviewPhase.hidden ? KClipPreviewAction.nothing : KClipPreviewAction.hide;
  }
}

/// 미리보기할 그림 항목인지: 저장한 그림, 또는 그림 분류에 드는 파일 항목(화면캡처 폴더·탐색기 복사).
KClipPreviewTarget? kclipPreviewTarget(Map item) {
  final id = item['id'];
  if (id is! num) return null;
  final image = item['kind'] == 'image' || (item['kind'] == 'files' && kclipItemIn(item, '그림'));
  return image ? KClipPreviewTarget(id.toInt(), (item['size'] as num?)?.toInt() ?? -1) : null;
}

/// 미리보기 상자 위치: 기준점 오른쪽(right+gap)에, 넘치면 왼쪽(left−gap−폭)에. 세로는 화면 안으로.
Offset kclipPreviewPlace({
  required double right,
  required double left,
  required double top,
  required Size box,
  required Size screen,
  double gap = 12,
}) {
  var x = right + gap;
  if (x + box.width > screen.width) x = left - gap - box.width;
  x = math.max(0, math.min(x, screen.width - box.width));
  final y = math.max(0.0, math.min(top, screen.height - box.height));
  return Offset(x, y);
}
