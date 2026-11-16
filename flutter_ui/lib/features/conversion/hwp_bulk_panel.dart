import 'package:flutter/material.dart';

import '../../core/theme/app_theme.dart';

/// HWP → HWPX 일괄 변환(2.0.7): 파일 하나·폴더·로컬 드라이브 전체, 암호 문서는 암호를 차례로 넣어 연다.
/// [call]은 백엔드 명령을 그대로 부르고 결과를 돌려준다(상태는 1초마다 get_state의 hwp_bulk로 읽는다).
typedef BulkCall = Future<Object?> Function(String method, List<Object?> args);

Future<void> showHwpBulkDialog(
  BuildContext context,
  BulkCall call, {
  bool allComputer = false,
}) {
  return showDialog<void>(
    context: context,
    builder: (_) => HwpBulkDialog(call: call, allComputer: allComputer),
  );
}

class HwpBulkDialog extends StatefulWidget {
  const HwpBulkDialog({
    super.key,
    required this.call,
    this.allComputer = false,
  });

  final BulkCall call;
  final bool allComputer;

  @override
  State<HwpBulkDialog> createState() => _HwpBulkDialogState();
}

class _HwpBulkDialogState extends State<HwpBulkDialog> {
  String scope = 'folder'; // folder | drive
  String? folder;
  List<String> drives = [];
  final chosenDrives = <String>{};
  bool recursive = true;
  final passwords = <String>[];
  final passwordField = TextEditingController();
  String? message;
  bool starting = false;
  Map? scanSummary;

  @override
  void initState() {
    super.initState();
    if (widget.allComputer) scope = 'drive';
    widget
        .call('hwp_bulk', ['drives'])
        .then((value) {
          if (!mounted || value is! Map) return;
          setState(() {
            drives = [for (final d in value['drives'] as List? ?? []) '$d'];
            if (widget.allComputer) chosenDrives.addAll(drives);
          });
        })
        .catchError((_) {});
  }

  @override
  void dispose() {
    passwordField.dispose();
    super.dispose();
  }

  Future<void> _pickFolder() async {
    final value = await widget.call('hwp_bulk', ['pick_folder']);
    if (!mounted) return;
    if (value is Map && value['folder'] is String) {
      setState(() => folder = value['folder'] as String);
    }
  }

  void _addPassword() {
    final text = passwordField.text;
    if (text.isEmpty || passwords.length >= 20) return;
    setState(() {
      if (!passwords.contains(text)) passwords.add(text);
      passwordField.clear();
    });
  }

  List<String> get roots => scope == 'folder'
      ? [?folder]
      : [
          for (final d in drives)
            if (chosenDrives.contains(d)) d,
        ];

  Future<void> _start() async {
    if (passwordField.text.isNotEmpty) _addPassword();
    if (roots.isEmpty) {
      setState(
        () =>
            message = scope == 'folder' ? '폴더를 먼저 고르세요.' : '드라이브를 하나 이상 고르세요.',
      );
      return;
    }
    setState(() => starting = true);
    final value = await widget.call('hwp_bulk', [
      widget.allComputer ? 'start_all' : 'start',
      {
        'roots': roots,
        'recursive': scope == 'drive' || recursive,
        'passwords': List.of(passwords),
      },
    ]);
    if (!mounted) return;
    passwords.clear();
    if (value is Map && value['ok'] == true) {
      Navigator.pop(context);
    } else {
      setState(() {
        starting = false;
        message = value is Map ? '${value['message'] ?? ''}' : '시작하지 못했어요.';
      });
    }
  }

  Future<void> _scanAll() async {
    if (roots.isEmpty) {
      setState(() => message = '로컬 드라이브를 찾지 못했습니다.');
      return;
    }
    setState(() {
      starting = true;
      message = null;
    });
    try {
      final response = await widget.call('hwp_bulk', [
        'scan_all',
        {'roots': roots},
      ]);
      if (!mounted) return;
      if (response is Map &&
          response['ok'] == true &&
          response['summary'] is Map) {
        setState(() {
          scanSummary = Map.from(response['summary'] as Map);
          starting = false;
        });
      } else {
        setState(() {
          starting = false;
          message = response is Map
              ? '${response['message'] ?? '사전 스캔에 실패했어요.'}'
              : '사전 스캔에 실패했어요.';
        });
      }
    } catch (e) {
      if (mounted) {
        setState(() {
          starting = false;
          message = '$e';
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      key: const Key('hwpBulkDialog'),
      title: Text(
        widget.allComputer ? '이 컴퓨터 전체 HWP → HWPX 변환' : 'HWP 찾아 HWPX로 바꾸기',
        style: const TextStyle(fontSize: 16),
      ),
      content: SizedBox(
        width: 440,
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              if (!widget.allComputer)
                SegmentedButton<String>(
                  segments: const [
                    ButtonSegment(
                      value: 'folder',
                      label: Text('폴더'),
                      icon: Icon(Icons.folder_outlined),
                    ),
                    ButtonSegment(
                      value: 'drive',
                      label: Text('드라이브 전체'),
                      icon: Icon(Icons.storage_outlined),
                    ),
                  ],
                  selected: {scope},
                  onSelectionChanged: (v) => setState(() {
                    scope = v.first;
                    message = null;
                  }),
                ),
              const SizedBox(height: 12),
              if (scope == 'folder') ...[
                Row(
                  children: [
                    Expanded(
                      child: Text(
                        folder ?? '고른 폴더 없음',
                        key: const Key('hwpBulkFolder'),
                        overflow: TextOverflow.ellipsis,
                      ),
                    ),
                    OutlinedButton(
                      key: const Key('hwpBulkPickFolder'),
                      onPressed: _pickFolder,
                      child: const Text('폴더 고르기'),
                    ),
                  ],
                ),
                CheckboxListTile(
                  key: const Key('hwpBulkRecursive'),
                  contentPadding: EdgeInsets.zero,
                  dense: true,
                  value: recursive,
                  onChanged: (v) => setState(() => recursive = v ?? true),
                  title: const Text('하위 폴더까지 찾기'),
                ),
              ] else ...[
                Wrap(
                  spacing: 8,
                  children: [
                    for (final d in drives)
                      FilterChip(
                        key: Key('hwpBulkDrive-$d'),
                        label: Text(d),
                        selected: chosenDrives.contains(d),
                        onSelected: (on) => setState(
                          () =>
                              on ? chosenDrives.add(d) : chosenDrives.remove(d),
                        ),
                      ),
                    if (drives.isEmpty) const Text('드라이브를 찾는 중…'),
                  ],
                ),
                const SizedBox(height: 6),
                Text(
                  'Windows·Program Files·AppData 같은 시스템 폴더는 건너뜁니다. 드라이브가 크면 오래 걸려요.',
                  style: TextStyle(fontSize: 12, color: context.colors.muted),
                ),
              ],
              if (widget.allComputer && scanSummary != null) ...[
                const Divider(height: 24),
                Text(
                  '사전 스캔 결과: ${scanSummary!['total'] ?? 0}개',
                  key: const Key('hwpBulkScanSummary'),
                  style: const TextStyle(fontWeight: FontWeight.w700),
                ),
                const SizedBox(height: 6),
                Text(
                  '정상 HWP5 ${scanSummary!['counts']?['normal'] ?? 0} · 암호 ${scanSummary!['counts']?['password'] ?? 0} · 배포용 ${scanSummary!['counts']?['distributed'] ?? 0} · DRM ${scanSummary!['counts']?['drm'] ?? 0} · HWP3 ${scanSummary!['counts']?['hwp3'] ?? 0} · 손상 ${scanSummary!['counts']?['corrupt'] ?? 0} · 이미 HWPX 있음 ${scanSummary!['counts']?['existing'] ?? 0}',
                ),
                Text(
                  '예상 시간 약 ${_duration(scanSummary!['estimated_seconds'])} · 병렬 작업자 ${scanSummary!['workers'] ?? 1}개',
                ),
                Text(
                  '건너뛸 폴더: ${(scanSummary!['skipped_folders'] as List? ?? []).join(', ')}',
                ),
                Text('네트워크 드라이브와 클라우드 전용 파일은 제외합니다.'),
              ],
              if (!widget.allComputer) const Divider(height: 24),
              if (!widget.allComputer)
                Row(
                  children: [
                    Expanded(
                      child: TextField(
                        key: const Key('hwpBulkPassword'),
                        controller: passwordField,
                        obscureText: true,
                        decoration: const InputDecoration(
                          isDense: true,
                          labelText: '암호 문서의 암호 (있으면)',
                        ),
                        onSubmitted: (_) => _addPassword(),
                      ),
                    ),
                    TextButton(
                      key: const Key('hwpBulkAddPassword'),
                      onPressed: _addPassword,
                      child: const Text('암호 추가'),
                    ),
                  ],
                ),
              if (!widget.allComputer && passwords.isNotEmpty)
                Wrap(
                  spacing: 6,
                  children: [
                    for (var i = 0; i < passwords.length; i++)
                      InputChip(
                        label: Text('암호 ${i + 1} ●●●●'),
                        onDeleted: () => setState(() => passwords.removeAt(i)),
                      ),
                  ],
                ),
              const SizedBox(height: 8),
              Text(
                widget.allComputer
                    ? '· 정상 HWP5만 병렬 변환합니다. 원본은 그대로 두고, 이미 HWPX가 있으면 덮어쓰지 않습니다.\n· 보호 문서·HWP3·손상 문서는 변환하지 않고 결과표에 분류해 남깁니다.'
                    : '· 결과는 원본 옆에 같은 이름의 .hwpx로 저장하고, 이미 있으면 건너뜁니다(원본은 그대로).\n'
                          '· 암호 문서는 넣은 암호를 차례로 시도하고, 결과 HWPX도 같은 암호로 잠깁니다.\n'
                          '· 배포용 문서는 한/글이 저장을 허용하면 바꾸고, 기관 보안(DRM) 문서는 이 PC의 보안 프로그램이 열어 줄 때만 바뀝니다.\n'
                          '· 끝나면 결과표(CSV)에 변환·실패 이유를 남깁니다. 암호는 저장하지 않습니다.',
                style: TextStyle(
                  fontSize: 12,
                  color: context.colors.textSecondary,
                ),
              ),
              if (message != null) ...[
                const SizedBox(height: 8),
                Text(
                  message!,
                  key: const Key('hwpBulkMessage'),
                  style: TextStyle(color: context.colors.danger),
                ),
              ],
            ],
          ),
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('닫기'),
        ),
        FilledButton(
          key: const Key('hwpBulkStart'),
          onPressed: starting
              ? null
              : (widget.allComputer && scanSummary == null ? _scanAll : _start),
          child: Text(
            starting
                ? '스캔 중…'
                : widget.allComputer
                ? (scanSummary == null ? '전체 사전 스캔' : '병렬 변환 시작')
                : '찾아서 바꾸기',
          ),
        ),
      ],
    );
  }

  String _duration(Object? value) {
    final seconds = (value as num? ?? 0).toInt().clamp(0, 100000000);
    if (seconds >= 3600) {
      return '${seconds ~/ 3600}시간 ${(seconds % 3600) ~/ 60}분';
    }
    if (seconds >= 60) return '${seconds ~/ 60}분 ${seconds % 60}초';
    return '$seconds초';
  }
}

/// 변환 결과 칸 맨 위에 보이는 일괄 변환 진행·결과 카드.
class HwpBulkCard extends StatelessWidget {
  const HwpBulkCard({super.key, required this.bulk, required this.call});

  final Map bulk;
  final BulkCall call;

  @override
  Widget build(BuildContext context) {
    final phase = '${bulk['phase'] ?? ''}';
    final found = bulk['found'] ?? 0;
    final done = bulk['done'] ?? 0;
    final protected = bulk['protected'] ?? 0;
    final active = phase == 'scan' || phase == 'convert';
    final failures = (bulk['failures'] as List? ?? [])
        .whereType<Map>()
        .toList();
    final title = switch (phase) {
      'scan' => 'HWP 찾는 중… $found개 찾음',
      'convert' =>
        'HWPX로 바꾸는 중 $done / $found'
            '${protected is num && protected > 0 ? ' · 보호 문서 $protected개 건너뜀' : ''}',
      'error' => 'HWPX 일괄 변환을 하지 못했어요',
      'stopped' => 'HWPX 일괄 변환을 멈췄어요',
      _ =>
        'HWPX 일괄 변환 끝: 변환 ${bulk['succeeded'] ?? 0} · 실패 ${bulk['failed'] ?? 0}'
            '${protected is num && protected > 0 ? ' · 보호 문서 $protected개 건너뜀' : ''}'
            ' · 이미 있어 건너뜀 ${bulk['skipped_existing'] ?? 0}',
    };
    final current = '${bulk['current'] ?? ''}';
    return Card(
      key: const Key('hwpBulkCard'),
      margin: const EdgeInsets.only(bottom: 8),
      child: Padding(
        padding: const EdgeInsets.all(10),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text(title, style: const TextStyle(fontWeight: FontWeight.w600)),
            if (active) ...[
              const SizedBox(height: 6),
              LinearProgressIndicator(
                value: phase == 'convert' && found is num && found > 0
                    ? (done as num) / found
                    : null,
              ),
              if (current.isNotEmpty)
                Text(
                  current,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(fontSize: 11, color: context.colors.muted),
                ),
              Text(
                '처리 속도 ${(bulk['rate_per_minute'] as num? ?? 0).toStringAsFixed(1)}개/분 · 남은 시간 ${_remaining(bulk['remaining_seconds'])}',
              ),
            ],
            if (phase == 'error' && '${bulk['error'] ?? ''}'.isNotEmpty)
              Text('${bulk['error']}', style: const TextStyle(fontSize: 12)),
            for (final f in failures.take(5))
              Text(
                '✕ ${'${f['source'] ?? ''}'.split(RegExp(r'[\\/]')).last}: ${f['message'] ?? ''}',
                maxLines: 2,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                  fontSize: 11,
                  color: context.colors.dangerDeep,
                ),
              ),
            if (failures.length > 5)
              Text(
                '… 실패 ${failures.length - 5}개 더(결과표에 모두 있어요)',
                style: const TextStyle(fontSize: 11),
              ),
            Wrap(
              alignment: WrapAlignment.end,
              children: [
                if (active)
                  TextButton(
                    key: const Key('hwpBulkStop'),
                    onPressed: () => call('hwp_bulk', ['stop']),
                    child: const Text('멈추기'),
                  ),
                if (!active && '${bulk['report'] ?? ''}'.isNotEmpty) ...[
                  TextButton(
                    key: const Key('hwpBulkOpenReport'),
                    onPressed: () => call('hwp_bulk', ['open_report']),
                    child: const Text('결과표 열기'),
                  ),
                  TextButton(
                    key: const Key('hwpBulkShowReport'),
                    onPressed: () => call('hwp_bulk', ['show_report']),
                    child: const Text('결과표 폴더'),
                  ),
                ],
                if (!active && failures.isNotEmpty)
                  TextButton(
                    key: const Key('hwpBulkRetryFailed'),
                    onPressed: () => call('hwp_bulk', ['retry_failed']),
                    child: const Text('실패분 다시 시도'),
                  ),
              ],
            ),
          ],
        ),
      ),
    );
  }

  String _remaining(Object? value) {
    if (value is! num) return '계산 중';
    final seconds = value.toInt().clamp(0, 100000000);
    if (seconds >= 3600) {
      return '${seconds ~/ 3600}시간 ${(seconds % 3600) ~/ 60}분';
    }
    if (seconds >= 60) return '${seconds ~/ 60}분 ${seconds % 60}초';
    return '$seconds초';
  }
}
