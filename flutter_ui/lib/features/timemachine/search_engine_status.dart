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

class SearchEngineStatus extends StatelessWidget {
  const SearchEngineStatus({super.key, required this.status});
  final Map status;

  @override
  Widget build(BuildContext context) {
    final text = searchEngineStatusText(status);
    if (text == null) return const SizedBox.shrink();
    final bundle = status['search_engine_status'] as Map;
    return Tooltip(
      message: '${bundle['engine_reason'] ?? text}',
      child: Text(text, key: const ValueKey('search-engine-status'),
          style: Theme.of(context).textTheme.bodySmall),
    );
  }
}
