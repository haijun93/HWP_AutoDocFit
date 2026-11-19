import 'dart:async';
import 'package:flutter/material.dart';

import '../../core/api/abbreviation_api.dart' show ApiCall;
import '../../core/theme/app_theme.dart';

/// Everything 대 몽땅검색 성능 측정 워크스페이스 화면.
class QmsBenchScreen extends StatefulWidget {
  const QmsBenchScreen({super.key, required this.api});

  final ApiCall api;

  @override
  State<QmsBenchScreen> createState() => _QmsBenchScreenState();
}

class _QmsBenchScreenState extends State<QmsBenchScreen> {
  // 환경 상태
  bool _detecting = false;
  Map<String, dynamic>? _detectReport;
  String? _detectError;

  // 옵션
  int _repeats = 10;
  String _querySetType = 'default';
  bool _includeRaw = false;

  // 실행 상태
  String _status = 'idle'; // idle, running, completed, failed, cancelled
  String? _errorMessage;
  Timer? _pollTimer;
  Map<String, dynamic>? _latestResult;
  List<Map<String, dynamic>> _events = [];

  // 최근 결과 목록
  List<Map<String, dynamic>> _recentResults = [];
  bool _loadingRecent = false;

  @override
  void initState() {
    super.initState();
    _refreshDetect();
    _loadRecentResults();
  }

  @override
  void dispose() {
    _pollTimer?.cancel();
    super.dispose();
  }

  Future<void> _refreshDetect() async {
    setState(() {
      _detecting = true;
      _detectError = null;
    });
    try {
      final res = await widget.api('qms_bench', [
        {'action': 'detect'}
      ]);
      if (mounted) {
        setState(() {
          _detecting = false;
          if (res is Map) {
            final data = res['data'] ?? res;
            if (data is Map<String, dynamic>) {
              _detectReport = data;
            } else if (data is Map) {
              _detectReport = Map<String, dynamic>.from(data);
            }
          }
        });
      }
    } catch (e) {
      if (mounted) {
        setState(() {
          _detecting = false;
          _detectError = e.toString();
        });
      }
    }
  }

  Future<void> _loadRecentResults() async {
    setState(() => _loadingRecent = true);
    try {
      final res = await widget.api('qms_bench', [
        {'action': 'list_results'}
      ]);
      if (mounted) {
        setState(() {
          _loadingRecent = false;
          if (res is Map) {
            final data = res['data'] ?? res;
            if (data is Map && data['results'] is List) {
              _recentResults = (data['results'] as List)
                  .whereType<Map>()
                  .map((m) => Map<String, dynamic>.from(m))
                  .toList();
            }
          }
        });
      }
    } catch (_) {
      if (mounted) setState(() => _loadingRecent = false);
    }
  }

  Future<void> _startBenchmark() async {
    setState(() {
      _status = 'running';
      _errorMessage = null;
      _events = [];
      _latestResult = null;
    });

    try {
      final res = await widget.api('qms_bench', [
        {
          'action': 'start',
          'repeats': _repeats,
          'query_set_type': _querySetType,
          'include_raw': _includeRaw,
        }
      ]);

      if (res is Map && res['ok'] == false) {
        final err = res['error'];
        final msg = err is Map ? err['message'] : '시작하지 못했습니다.';
        setState(() {
          _status = 'failed';
          _errorMessage = msg?.toString() ?? '시작하지 못했습니다.';
        });
        return;
      }

      _startPolling();
    } catch (e) {
      if (mounted) {
        setState(() {
          _status = 'failed';
          _errorMessage = e.toString();
        });
      }
    }
  }

  void _startPolling() {
    _pollTimer?.cancel();
    _pollTimer = Timer.periodic(const Duration(milliseconds: 300), (_) async {
      await _checkStatus();
    });
  }

  Future<void> _checkStatus() async {
    try {
      final res = await widget.api('qms_bench', [
        {'action': 'status'}
      ]);

      if (!mounted) return;

      if (res is Map) {
        final data = res['data'] ?? res;
        if (data is Map) {
          final st = data['status'] as String? ?? 'idle';
          final err = data['error'] as String?;
          final rawEvents = data['events'] as List?;
          final resultData = data['result'] as Map?;

          final parsedEvents = rawEvents != null
              ? rawEvents
                  .whereType<Map>()
                  .map((m) => Map<String, dynamic>.from(m))
                  .toList()
              : _events;

          setState(() {
            _status = st;
            _errorMessage = err;
            _events = parsedEvents;
            if (resultData != null) {
              _latestResult = Map<String, dynamic>.from(resultData);
            }
          });

          if (st != 'running') {
            _pollTimer?.cancel();
            _pollTimer = null;
            _loadRecentResults();
          }
        }
      }
    } catch (_) {}
  }

  Future<void> _cancelBenchmark() async {
    try {
      await widget.api('qms_bench', [
        {'action': 'cancel'}
      ]);
      await _checkStatus();
    } catch (_) {}
  }

  Future<void> _openResultsFolder() async {
    try {
      await widget.api('qms_bench', [
        {'action': 'open_folder'}
      ]);
    } catch (_) {}
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.colors;
    final theme = Theme.of(context);

    return Scaffold(
      backgroundColor: colors.canvas,
      body: SingleChildScrollView(
        padding: const EdgeInsets.all(24),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            // 상단 제목
            Row(
              children: [
                Icon(Icons.speed_outlined, size: 28, color: theme.colorScheme.primary),
                const SizedBox(width: 10),
                Text(
                  '검색 벤치마크 (개발자 모드)',
                  style: theme.textTheme.headlineSmall?.copyWith(
                    fontWeight: FontWeight.bold,
                    color: colors.text,
                  ),
                ),
              ],
            ),
            const SizedBox(height: 8),
            Text(
              'Everything과 몽땅검색의 질의 지연 시간, 결과 수, 시스템 자원 소모량을 동일한 환경에서 계측합니다.',
              style: TextStyle(color: colors.textSecondary, fontSize: 13),
            ),
            const SizedBox(height: 20),

            // 1. 환경 상태 카드
            _buildEnvironmentCard(context),
            const SizedBox(height: 16),

            // 2. 옵션 및 제어 카드
            _buildOptionsCard(context),
            const SizedBox(height: 16),

            // 3. 진행 상태 및 현재 질의
            if (_status == 'running' || _events.isNotEmpty || _errorMessage != null)
              _buildProgressCard(context),
            if (_status == 'running' || _events.isNotEmpty || _errorMessage != null)
              const SizedBox(height: 16),

            // 4. 결과 요약 표
            if (_latestResult != null) _buildResultSummaryCard(context),
            if (_latestResult != null) const SizedBox(height: 16),

            // 5. 최근 결과 목록
            _buildRecentResultsCard(context),
          ],
        ),
      ),
    );
  }

  Widget _buildEnvironmentCard(BuildContext context) {
    final colors = context.colors;
    final theme = Theme.of(context);

    final ev = _detectReport?['everything'] as Map?;
    final kf = _detectReport?['kfind'] as Map?;

    final evStatus = ev?['status'] as String? ?? '알 수 없음';
    final evGuidance = ev?['guidance'] as String? ?? '상태를 확인하고 있습니다.';
    final evVersion = ev?['version'] as String?;
    final evRunning = ev?['running'] == true;

    final kfRunning = kf?['running'] == true;
    final kfPipeExists = kf?['pipe_exists'] == true;

    String evStatusKorean;
    switch (evStatus) {
      case 'gui_client_running':
        evStatusKorean = 'Everything GUI 실행 중 (IPC 측정 가능)';
        break;
      case 'service_only':
        evStatusKorean = 'Everything 서비스만 실행 중 (GUI 앱 필요)';
        break;
      case 'not_running':
        evStatusKorean = 'Everything 미실행 (앱 실행 필요)';
        break;
      case 'not_installed':
        evStatusKorean = 'Everything 미설치 (몽땅검색 단독 측정)';
        break;
      default:
        evStatusKorean = evStatus;
    }

    return Card(
      key: const Key('qmsBenchEnvironmentCard'),
      elevation: 0,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(8),
        side: BorderSide(color: colors.line),
      ),
      color: colors.surface,
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              children: [
                Text(
                  '측정 환경 상태',
                  style: theme.textTheme.titleMedium?.copyWith(
                    fontWeight: FontWeight.bold,
                    color: colors.text,
                  ),
                ),
                TextButton.icon(
                  key: const Key('qmsBenchDetectRefreshButton'),
                  onPressed: _detecting ? null : _refreshDetect,
                  icon: _detecting
                      ? const SizedBox(
                          width: 14,
                          height: 14,
                          child: CircularProgressIndicator(strokeWidth: 2),
                        )
                      : const Icon(Icons.refresh, size: 16),
                  label: const Text('새로고침'),
                ),
              ],
            ),
            const Divider(height: 16),
            if (_detectError != null)
              Padding(
                padding: const EdgeInsets.only(bottom: 8),
                child: Text(
                  '탐지 오류: $_detectError',
                  style: TextStyle(color: theme.colorScheme.error, fontSize: 13),
                ),
              ),
            Row(
              children: [
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        'Everything 상태',
                        style: TextStyle(
                          fontSize: 12,
                          fontWeight: FontWeight.w600,
                          color: colors.textSecondary,
                        ),
                      ),
                      const SizedBox(height: 4),
                      Row(
                        children: [
                          Icon(
                            evRunning ? Icons.check_circle : Icons.warning_amber,
                            size: 16,
                            color: evRunning ? Colors.green : Colors.amber,
                          ),
                          const SizedBox(width: 6),
                          Expanded(
                            child: Text(
                              evStatusKorean,
                              style: TextStyle(
                                fontSize: 13,
                                fontWeight: FontWeight.bold,
                                color: colors.text,
                              ),
                            ),
                          ),
                        ],
                      ),
                      if (evVersion != null)
                        Padding(
                          padding: const EdgeInsets.only(top: 2),
                          child: Text(
                            '버전: $evVersion',
                            style: TextStyle(fontSize: 12, color: colors.textSecondary),
                          ),
                        ),
                      Padding(
                        padding: const EdgeInsets.only(top: 4),
                        child: Text(
                          evGuidance,
                          style: TextStyle(fontSize: 12, color: colors.textSecondary),
                        ),
                      ),
                    ],
                  ),
                ),
                const SizedBox(width: 16),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        '몽땅검색(kfind) 상태',
                        style: TextStyle(
                          fontSize: 12,
                          fontWeight: FontWeight.w600,
                          color: colors.textSecondary,
                        ),
                      ),
                      const SizedBox(height: 4),
                      Row(
                        children: [
                          Icon(
                            kfPipeExists ? Icons.check_circle : Icons.warning_amber,
                            size: 16,
                            color: kfPipeExists ? Colors.green : Colors.amber,
                          ),
                          const SizedBox(width: 6),
                          Text(
                            kfPipeExists
                                ? '색인 파이프 준비 완료'
                                : (kfRunning ? '프로세스 실행 중 (파이프 대기)' : '미실행'),
                            style: TextStyle(
                              fontSize: 13,
                              fontWeight: FontWeight.bold,
                              color: colors.text,
                            ),
                          ),
                        ],
                      ),
                      const SizedBox(height: 4),
                      Text(
                        '파일명 색인 파이프를 통해 고속 IPC로 측정합니다.',
                        style: TextStyle(fontSize: 12, color: colors.textSecondary),
                      ),
                    ],
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildOptionsCard(BuildContext context) {
    final colors = context.colors;
    final theme = Theme.of(context);

    final isRunning = _status == 'running';

    return Card(
      key: const Key('qmsBenchOptionsCard'),
      elevation: 0,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(8),
        side: BorderSide(color: colors.line),
      ),
      color: colors.surface,
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              '측정 옵션',
              style: theme.textTheme.titleMedium?.copyWith(
                fontWeight: FontWeight.bold,
                color: colors.text,
              ),
            ),
            const Divider(height: 16),
            Wrap(
              spacing: 24,
              runSpacing: 12,
              crossAxisAlignment: WrapCrossAlignment.center,
              children: [
                // 반복 횟수
                Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    const Text('반복 횟수: ', style: TextStyle(fontSize: 13)),
                    DropdownButton<int>(
                      key: const Key('qmsBenchRepeatsDropdown'),
                      value: _repeats,
                      onChanged: isRunning
                          ? null
                          : (val) {
                              if (val != null) setState(() => _repeats = val);
                            },
                      items: const [
                        DropdownMenuItem(value: 3, child: Text('3회 (빠름)')),
                        DropdownMenuItem(value: 5, child: Text('5회')),
                        DropdownMenuItem(value: 10, child: Text('10회 (표준)')),
                        DropdownMenuItem(value: 20, child: Text('20회 (정밀)')),
                      ],
                    ),
                  ],
                ),

                // 질의 세트
                Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    const Text('질의 세트: ', style: TextStyle(fontSize: 13)),
                    DropdownButton<String>(
                      key: const Key('qmsBenchQuerySetDropdown'),
                      value: _querySetType,
                      onChanged: isRunning
                          ? null
                          : (val) {
                              if (val != null) setState(() => _querySetType = val);
                            },
                      items: const [
                        DropdownMenuItem(value: 'default', child: Text('기본 질의 세트')),
                        DropdownMenuItem(value: 'my_files', child: Text('내 파일 표본 세트')),
                      ],
                    ),
                  ],
                ),

                // 원문 포함 체크박스
                Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Checkbox(
                      key: const Key('qmsBenchIncludeRawCheckbox'),
                      value: _includeRaw,
                      onChanged: isRunning
                          ? null
                          : (val) {
                              setState(() => _includeRaw = val ?? false);
                            },
                    ),
                    const Text('원문 포함 (개인정보 주의)', style: TextStyle(fontSize: 13)),
                  ],
                ),
              ],
            ),
            Padding(
              padding: const EdgeInsets.only(top: 8, bottom: 12),
              child: Text(
                '안내: 원문 포함을 켜면 결과 파일에 검색 질의 문자열이 저장됩니다. 끈 상태에서는 해시와 길이만 저장되어 안전합니다.',
                style: TextStyle(fontSize: 12, color: colors.textSecondary),
              ),
            ),
            Row(
              children: [
                FilledButton.icon(
                  key: const Key('qmsBenchStartButton'),
                  onPressed: isRunning ? null : _startBenchmark,
                  icon: const Icon(Icons.play_arrow, size: 18),
                  label: const Text('측정 시작'),
                ),
                const SizedBox(width: 12),
                OutlinedButton.icon(
                  key: const Key('qmsBenchCancelButton'),
                  onPressed: isRunning ? _cancelBenchmark : null,
                  icon: const Icon(Icons.stop, size: 18),
                  label: const Text('측정 취소'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildProgressCard(BuildContext context) {
    final colors = context.colors;
    final theme = Theme.of(context);

    final isRunning = _status == 'running';
    final lastEvent = _events.isNotEmpty ? _events.last : null;

    final phase = lastEvent?['phase'] as String? ?? '';
    final queryId = lastEvent?['query_id'] as String? ?? '';
    final repeat = lastEvent?['repeat'] as int? ?? 0;
    final latency = (lastEvent?['latency_ms'] as num?)?.toDouble() ?? 0.0;
    final resultCount = lastEvent?['result_count'] as int? ?? 0;

    String statusText;
    Color statusColor;
    switch (_status) {
      case 'running':
        statusText = '측정 진행 중...';
        statusColor = theme.colorScheme.primary;
        break;
      case 'completed':
        statusText = '측정 완료';
        statusColor = Colors.green;
        break;
      case 'cancelled':
        statusText = '사용자에 의해 취소됨';
        statusColor = Colors.amber;
        break;
      case 'failed':
        statusText = '측정 실패';
        statusColor = theme.colorScheme.error;
        break;
      default:
        statusText = _status;
        statusColor = colors.textSecondary;
    }

    return Card(
      key: const Key('qmsBenchProgressArea'),
      elevation: 0,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(8),
        side: BorderSide(color: colors.line),
      ),
      color: colors.surface,
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              children: [
                Row(
                  children: [
                    Text(
                      '진행 상태: ',
                      style: theme.textTheme.titleSmall?.copyWith(
                        fontWeight: FontWeight.bold,
                        color: colors.text,
                      ),
                    ),
                    Text(
                      statusText,
                      style: TextStyle(
                        fontWeight: FontWeight.bold,
                        color: statusColor,
                      ),
                    ),
                  ],
                ),
                Text(
                  '이벤트: ${_events.length}건',
                  style: TextStyle(fontSize: 12, color: colors.textSecondary),
                ),
              ],
            ),
            if (isRunning) ...[
              const SizedBox(height: 12),
              const LinearProgressIndicator(),
              const SizedBox(height: 12),
              Text(
                '현재 질의: $queryId (단계: $phase, 반복: $repeat/$_repeats) | 지연: ${latency.toStringAsFixed(2)}ms | 결과: $resultCount건',
                style: TextStyle(fontSize: 13, color: colors.text),
              ),
            ],
            if (_errorMessage != null) ...[
              const SizedBox(height: 8),
              Text(
                '오류: $_errorMessage',
                style: TextStyle(color: theme.colorScheme.error, fontSize: 13),
              ),
            ],
          ],
        ),
      ),
    );
  }

  Widget _buildResultSummaryCard(BuildContext context) {
    final colors = context.colors;
    final theme = Theme.of(context);

    final res = _latestResult!;
    final runMeta = res['run'] as Map? ?? {};
    final queries = res['queries'] as List? ?? [];
    final resources = res['resources'] as Map? ?? {};
    final evMeta = runMeta['everything'] as Map? ?? {};
    final unavailableReasons = (evMeta['unavailable_reasons'] as List?)
            ?.map((e) => e.toString())
            .toList() ??
        [];

    final runId = runMeta['run_id'] ?? 'unknown';
    final adapter = runMeta['adapter'] ?? 'unknown';

    return Card(
      key: const Key('qmsBenchResultSummaryTable'),
      elevation: 0,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(8),
        side: BorderSide(color: colors.line),
      ),
      color: colors.surface,
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              '최근 측정 결과 요약 ($runId)',
              style: theme.textTheme.titleMedium?.copyWith(
                fontWeight: FontWeight.bold,
                color: colors.text,
              ),
            ),
            const SizedBox(height: 4),
            Text(
              '어댑터: $adapter | 총 질의: ${queries.length}개',
              style: TextStyle(fontSize: 12, color: colors.textSecondary),
            ),
            const Divider(height: 16),

            // 측정 불가 사유 표시
            if (unavailableReasons.isNotEmpty)
              Container(
                margin: const EdgeInsets.only(bottom: 12),
                padding: const EdgeInsets.all(10),
                decoration: BoxDecoration(
                  color: colors.canvas,
                  borderRadius: BorderRadius.circular(6),
                  border: Border.all(color: Colors.amber.withOpacity(0.5)),
                ),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    const Text(
                      '측정 불가 / 폴백 사유:',
                      style: TextStyle(fontSize: 12, fontWeight: FontWeight.bold, color: Colors.amber),
                    ),
                    for (final reason in unavailableReasons)
                      Padding(
                        padding: const EdgeInsets.only(top: 2),
                        child: Text('• $reason', style: const TextStyle(fontSize: 12)),
                      ),
                  ],
                ),
              ),

            // 질의 결과 요약 표
            Table(
              border: TableBorder.all(color: colors.line, width: 1),
              columnWidths: const {
                0: FlexColumnWidth(1.5),
                1: FlexColumnWidth(2),
                2: FlexColumnWidth(1.2),
                3: FlexColumnWidth(1.2),
                4: FlexColumnWidth(1.2),
                5: FlexColumnWidth(1),
              },
              children: [
                TableRow(
                  decoration: BoxDecoration(color: colors.canvas),
                  children: const [
                    Padding(
                      padding: EdgeInsets.all(6),
                      child: Text('질의 ID', style: TextStyle(fontWeight: FontWeight.bold, fontSize: 12)),
                    ),
                    Padding(
                      padding: EdgeInsets.all(6),
                      child: Text('분류', style: TextStyle(fontWeight: FontWeight.bold, fontSize: 12)),
                    ),
                    Padding(
                      padding: EdgeInsets.all(6),
                      child: Text('p50 (ms)', style: TextStyle(fontWeight: FontWeight.bold, fontSize: 12)),
                    ),
                    Padding(
                      padding: EdgeInsets.all(6),
                      child: Text('p95 (ms)', style: TextStyle(fontWeight: FontWeight.bold, fontSize: 12)),
                    ),
                    Padding(
                      padding: EdgeInsets.all(6),
                      child: Text('결과 건수', style: TextStyle(fontWeight: FontWeight.bold, fontSize: 12)),
                    ),
                    Padding(
                      padding: EdgeInsets.all(6),
                      child: Text('오류', style: TextStyle(fontWeight: FontWeight.bold, fontSize: 12)),
                    ),
                  ],
                ),
                for (final q in queries.whereType<Map>())
                  TableRow(
                    children: [
                      Padding(
                        padding: const EdgeInsets.all(6),
                        child: Text(
                          q['digest']?['id']?.toString() ?? '',
                          style: const TextStyle(fontSize: 12),
                        ),
                      ),
                      Padding(
                        padding: const EdgeInsets.all(6),
                        child: Text(
                          q['digest']?['kind']?.toString() ?? '',
                          style: const TextStyle(fontSize: 12),
                        ),
                      ),
                      Padding(
                        padding: const EdgeInsets.all(6),
                        child: Text(
                          ((q['statistics']?['p50_ms'] as num?)?.toDouble() ?? 0.0).toStringAsFixed(2),
                          style: const TextStyle(fontSize: 12),
                        ),
                      ),
                      Padding(
                        padding: const EdgeInsets.all(6),
                        child: Text(
                          ((q['statistics']?['p95_ms'] as num?)?.toDouble() ?? 0.0).toStringAsFixed(2),
                          style: const TextStyle(fontSize: 12),
                        ),
                      ),
                      Padding(
                        padding: const EdgeInsets.all(6),
                        child: Text(
                          (q['result_count'] ?? 0).toString(),
                          style: const TextStyle(fontSize: 12),
                        ),
                      ),
                      Padding(
                        padding: const EdgeInsets.all(6),
                        child: Text(
                          (q['errors'] ?? 0).toString(),
                          style: TextStyle(
                            fontSize: 12,
                            color: (q['errors'] ?? 0) > 0 ? theme.colorScheme.error : null,
                          ),
                        ),
                      ),
                    ],
                  ),
              ],
            ),
            const SizedBox(height: 12),

            // 리소스 요약
            if (resources.isNotEmpty)
              Container(
                padding: const EdgeInsets.all(8),
                decoration: BoxDecoration(
                  color: colors.canvas,
                  borderRadius: BorderRadius.circular(6),
                ),
                child: Row(
                  children: [
                    Expanded(
                      child: Text(
                        '메모리(WS): ${_formatBytes(resources['working_set_peak_bytes'] as num?)}',
                        style: TextStyle(fontSize: 12, color: colors.textSecondary),
                      ),
                    ),
                    Expanded(
                      child: Text(
                        'Private Bytes: ${_formatBytes(resources['private_bytes_peak_bytes'] as num?)}',
                        style: TextStyle(fontSize: 12, color: colors.textSecondary),
                      ),
                    ),
                    Expanded(
                      child: Text(
                        'CPU 시간: ${(resources['cpu_user_ms'] as num? ?? 0) + (resources['cpu_kernel_ms'] as num? ?? 0)}ms',
                        style: TextStyle(fontSize: 12, color: colors.textSecondary),
                      ),
                    ),
                  ],
                ),
              ),
          ],
        ),
      ),
    );
  }

  Widget _buildRecentResultsCard(BuildContext context) {
    final colors = context.colors;
    final theme = Theme.of(context);

    return Card(
      key: const Key('qmsBenchRecentResultsList'),
      elevation: 0,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(8),
        side: BorderSide(color: colors.line),
      ),
      color: colors.surface,
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              children: [
                Text(
                  '최근 측정 결과 파일',
                  style: theme.textTheme.titleMedium?.copyWith(
                    fontWeight: FontWeight.bold,
                    color: colors.text,
                  ),
                ),
                FilledButton.tonalIcon(
                  key: const Key('qmsBenchOpenFolderButton'),
                  onPressed: _openResultsFolder,
                  icon: const Icon(Icons.folder_open, size: 16),
                  label: const Text('결과 폴더 열기'),
                ),
              ],
            ),
            const Divider(height: 16),
            if (_loadingRecent)
              const Center(child: Padding(padding: EdgeInsets.all(8), child: CircularProgressIndicator()))
            else if (_recentResults.isEmpty)
              Padding(
                padding: const EdgeInsets.all(12),
                child: Text('저장된 측정 결과가 없습니다.', style: TextStyle(color: colors.textSecondary, fontSize: 13)),
              )
            else
              ListView.separated(
                shrinkWrap: true,
                physics: const NeverScrollableScrollPhysics(),
                itemCount: _recentResults.length.clamp(0, 5),
                separatorBuilder: (_, _) => const Divider(height: 1),
                itemBuilder: (context, idx) {
                  final item = _recentResults[idx];
                  final runId = item['run_id'] ?? item['filename'];
                  final adapter = item['adapter'] ?? 'everything';
                  final repeats = item['repeats'] ?? 0;
                  final totalQ = item['total_queries'] ?? 0;
                  final completed = item['completed'] == true;

                  return ListTile(
                    dense: true,
                    contentPadding: EdgeInsets.zero,
                    leading: Icon(
                      completed ? Icons.check_circle_outline : Icons.pending_outlined,
                      size: 20,
                      color: completed ? Colors.green : Colors.amber,
                    ),
                    title: Text(runId.toString(), style: const TextStyle(fontWeight: FontWeight.w600, fontSize: 13)),
                    subtitle: Text(
                      '어댑터: $adapter | 반복: $repeats회 | 질의 수: $totalQ개',
                      style: TextStyle(fontSize: 12, color: colors.textSecondary),
                    ),
                  );
                },
              ),
          ],
        ),
      ),
    );
  }

  String _formatBytes(num? bytes) {
    if (bytes == null || bytes <= 0) return '0 B';
    final b = bytes.toDouble();
    if (b < 1024) return '${b.toStringAsFixed(0)} B';
    if (b < 1024 * 1024) return '${(b / 1024).toStringAsFixed(1)} KB';
    return '${(b / (1024 * 1024)).toStringAsFixed(1)} MB';
  }
}
