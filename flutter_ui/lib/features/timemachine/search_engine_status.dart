import 'package:flutter/material.dart';

/// content_status의 명시적 엔진 상태를 읽는다. 응답이 없는 동안 상태를 추측하지 않는다.
String? searchEngineStatusText(Map status) {
  final bundle = status['search_engine_status'];
  if (bundle is! Map) return null;
  return switch (bundle['transition']) {
    'legacy' => '검색: 기존 엔진 (설정)',
    'ready' => '검색: 새 엔진 · 동기화 완료',
    'seeding' => '검색: 기존 엔진 · 새 엔진 색인 동기화 중',
    'pending' => '검색: 기존 엔진 · 새 엔진 동기화 대기',
    'fallback' => '검색: 기존 엔진으로 자동 대체',
    _ => null,
  };
}

/// 엔진 상태 이유(내부 값)를 사람이 읽는 말로 바꾼다. 모르는 값은 그대로 보여 점검할 수 있게 한다.
String searchEngineReasonText(Object? reason, String fallback) {
  final key = '${reason ?? ''}';
  if (key.isEmpty) return fallback;
  return switch (key) {
    'not_connected' => '새 검색 엔진(색인 프로그램)에 아직 연결되지 않았어요. 연결되면 자동으로 바뀌며, 그동안은 기존 엔진으로 검색해요.',
    'kfind_unreachable' => '새 검색 엔진에 연결하지 못했어요. 1분 간격으로 다시 시도 중이며, 그동안은 기존 엔진으로 검색해요.',
    'kfind_pending' => '문서가 새로 색인돼 새 엔진 동기화를 기다리고 있어요.',
    'kfind_seeding' => '새 엔진에 색인을 옮기는 중이에요.',
    'kfind_seed_cancelled' => '새 엔진 동기화가 중단됐어요. 다음 색인 때 이어서 해요.',
    'configured_legacy' => '설정에서 기존 엔진을 쓰도록 되어 있어요.',
    'shadow' => '새 엔진 결과를 비교만 하고 기존 엔진 결과를 보여 주고 있어요.',
    _ => key,
  };
}

class SearchEngineStatus extends StatelessWidget {
  const SearchEngineStatus({super.key, required this.status});
  final Map status;

  @override
  Widget build(BuildContext context) {
    final text = searchEngineStatusText(status);
    if (text == null) return const SizedBox.shrink();
    final bundle = status['search_engine_status'] as Map;
    return Tooltip(
      message: searchEngineReasonText(bundle['engine_reason'], text),
      child: Text(text, key: const ValueKey('search-engine-status'),
          style: Theme.of(context).textTheme.bodySmall),
    );
  }
}
