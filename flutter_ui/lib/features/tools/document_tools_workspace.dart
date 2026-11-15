import 'dart:async';
import 'dart:convert';
import '../../core/close_guards.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import '../../core/api/abbreviation_api.dart';

class DocumentToolsWorkspace extends StatefulWidget {
  const DocumentToolsWorkspace({
    super.key,
    required this.api,
    this.initial = 'convert',
    this.loadSaved = false,
    this.isModal = true,
  });
  final ApiCall api;
  final String initial;
  final bool loadSaved;
  final bool isModal;
  @override
  State<DocumentToolsWorkspace> createState() => _ToolsState();
}

class _ToolsState extends State<DocumentToolsWorkspace> {
  final input = TextEditingController(), output = TextEditingController();

  // 개조식 정리 묶음: 'convert' (개조식 변환), 'labels' (라벨 텍스트 변환), 'clean' (AI 답변 기호 정리), 'none'
  String outlineOption = 'convert';

  // 일괄 옵션 체크 (다중 선택): 'commas', 'amount', 'weekdays', 'mask_rrn'
  final Set<String> extraOptions = <String>{};

  // 독립 계산 도구: 'date_diff', 'age', 'table_calc'
  String? calcMode;

  String status = '', calculation = 'col_sum';
  bool busy = false, closing = false;
  CloseLease? _closeLease;
  Timer? _debounceTimer;
  int _requestId = 0;

  static const int maxRealtimeBytes = 200 * 1024; // 200KB

  @override
  void initState() {
    super.initState();
    _initInitialMode();
    input.addListener(_onInputChanged);
    _loadSavedOptions();

    if (widget.isModal) {
      _closeLease = CloseGuards.register(() async {
        if (busy || !canCloseEditor(context)) return false;
        leave();
        return true;
      });
    }
  }

  void _initInitialMode() {
    if (widget.initial == 'date_diff' ||
        widget.initial == 'age' ||
        widget.initial == 'table_calc') {
      calcMode = widget.initial;
      outlineOption = 'none';
    } else if (widget.initial == 'clean' ||
        widget.initial == 'convert' ||
        widget.initial == 'labels') {
      outlineOption = widget.initial;
      calcMode = null;
    } else if (const ['commas', 'amount', 'weekdays', 'mask_rrn']
        .contains(widget.initial)) {
      outlineOption = 'none';
      extraOptions.add(widget.initial);
      calcMode = null;
    } else {
      outlineOption = 'convert';
      calcMode = null;
    }
  }

  Future<void> _loadSavedOptions() async {
    if (!widget.loadSaved || widget.initial != 'convert') return;
    try {
      final res = await widget.api('document_tool', ['get_options', {}]);
      if (res is Map && res['ok'] == true && res['data'] is Map) {
        final raw = res['data']['options'];
        if (raw is List) {
          final opts = raw.cast<String>();
          if (!mounted || opts.isEmpty) return;
          setState(() {
            extraOptions.clear();
            String? foundOutline;
            for (final opt in opts) {
              if (opt == 'convert' || opt == 'labels' || opt == 'clean') {
                foundOutline ??= opt;
              } else if (const ['commas', 'amount', 'weekdays', 'mask_rrn']
                  .contains(opt)) {
                extraOptions.add(opt);
              }
            }
            outlineOption = foundOutline ?? 'none';
          });
        }
      }
    } catch (_) {}
  }

  @override
  void dispose() {
    _debounceTimer?.cancel();
    input.removeListener(_onInputChanged);
    _closeLease?.release();
    input.dispose();
    output.dispose();
    super.dispose();
  }

  void _onInputChanged() {
    _debounceTimer?.cancel();
    final text = input.text;
    if (text.isEmpty) {
      if (output.text.isNotEmpty) setState(() => output.text = '');
      return;
    }
    final byteLen = utf8.encode(text).length;
    if (byteLen > maxRealtimeBytes) {
      setState(() {
        status = '입력이 커서 자동 변환을 멈췄습니다. 실행 버튼을 눌러 변환하세요.';
      });
      return;
    }
    // 150ms debounce
    _debounceTimer = Timer(const Duration(milliseconds: 150), () {
      if (mounted) {
        run();
      }
    });
  }

  Future<Map> call(String name, Map payload) async {
    final raw = await widget.api('document_tool', [name, payload]);
    if (raw is! Map || raw['ok'] != true) {
      final problem = raw is Map ? raw['error'] : null;
      throw Exception(
        problem is Map ? problem['message'] ?? '연결 응답을 확인해 주세요.' : '응답 형식 오류',
      );
    }
    return raw['data'] as Map;
  }

  Future<void> run() async {
    _debounceTimer?.cancel();
    final currentRequestId = ++_requestId;
    setState(() => busy = true);

    try {
      Map data;
      if (calcMode != null) {
        data = await call(calcMode!, {
          'text': input.text,
          'operation': calculation,
        });
      } else {
        final active = <String>[];
        if (outlineOption != 'none') {
          active.add(outlineOption);
        }
        for (final opt in const ['commas', 'amount', 'weekdays', 'mask_rrn']) {
          if (extraOptions.contains(opt)) {
            active.add(opt);
          }
        }

        if (active.length == 1) {
          data = await call(active.first, {
            'text': input.text,
            'operation': calculation,
          });
        } else {
          data = await call('pipeline', {
            'text': input.text,
            'options': active,
          });
        }
      }

      if (mounted && currentRequestId == _requestId) {
        setState(() {
          output.text = '${data['text'] ?? ''}';
          if (status.startsWith('입력이 커서')) status = '';
        });
      }
    } catch (e) {
      if (mounted && currentRequestId == _requestId) {
        setState(() => status = '$e');
      }
    } finally {
      if (mounted && currentRequestId == _requestId) {
        setState(() => busy = false);
      }
    }
  }

  Future<void> addDocument() async {
    if (busy) return;
    setState(() => busy = true);
    try {
      final result = await call('add_text', {
        'text': output.text.isEmpty ? input.text : output.text,
      });
      if (mounted) setState(() => status = '문서 목록에 추가: ${result['path']}');
    } catch (e) {
      if (mounted) setState(() => status = '$e');
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  void leave() {
    if (busy || closing) return;
    if (!Navigator.of(context).canPop()) return;
    setState(() => closing = true);
    Navigator.pop(context);
  }

  Widget editor() => TextField(
    controller: input,
    maxLines: null,
    expands: true,
    textAlignVertical: TextAlignVertical.top,
    decoration: const InputDecoration(
      labelText: '입력',
      alignLabelWithHint: true,
      border: OutlineInputBorder(),
    ),
  );

  Widget _buildOptionSection() {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Wrap(
          spacing: 6,
          runSpacing: 4,
          crossAxisAlignment: WrapCrossAlignment.center,
          children: [
            const Text(
              '개조식 정리:',
              style: TextStyle(fontWeight: FontWeight.bold, fontSize: 13),
            ),
            ChoiceChip(
              label: const Text('개조식 변환'),
              selected: calcMode == null && outlineOption == 'convert',
              onSelected: (selected) {
                setState(() {
                  calcMode = null;
                  outlineOption = selected ? 'convert' : 'none';
                });
                run();
              },
            ),
            ChoiceChip(
              label: const Text('라벨 텍스트 변환'),
              selected: calcMode == null && outlineOption == 'labels',
              onSelected: (selected) {
                setState(() {
                  calcMode = null;
                  outlineOption = selected ? 'labels' : 'none';
                });
                run();
              },
            ),
            ChoiceChip(
              label: const Text('AI 답변 기호 정리'),
              selected: calcMode == null && outlineOption == 'clean',
              onSelected: (selected) {
                setState(() {
                  calcMode = null;
                  outlineOption = selected ? 'clean' : 'none';
                });
                run();
              },
            ),
          ],
        ),
        const SizedBox(height: 6),
        Wrap(
          spacing: 6,
          runSpacing: 4,
          crossAxisAlignment: WrapCrossAlignment.center,
          children: [
            const Text(
              '일괄 옵션:',
              style: TextStyle(fontWeight: FontWeight.bold, fontSize: 13),
            ),
            FilterChip(
              label: const Text('숫자 쉼표'),
              selected: calcMode == null && extraOptions.contains('commas'),
              onSelected: (selected) {
                setState(() {
                  calcMode = null;
                  if (selected) {
                    extraOptions.add('commas');
                  } else {
                    extraOptions.remove('commas');
                  }
                });
                run();
              },
            ),
            FilterChip(
              label: const Text('금액 한글화'),
              selected: calcMode == null && extraOptions.contains('amount'),
              onSelected: (selected) {
                setState(() {
                  calcMode = null;
                  if (selected) {
                    extraOptions.add('amount');
                  } else {
                    extraOptions.remove('amount');
                  }
                });
                run();
              },
            ),
            FilterChip(
              label: const Text('날짜·요일'),
              selected: calcMode == null && extraOptions.contains('weekdays'),
              onSelected: (selected) {
                setState(() {
                  calcMode = null;
                  if (selected) {
                    extraOptions.add('weekdays');
                  } else {
                    extraOptions.remove('weekdays');
                  }
                });
                run();
              },
            ),
            FilterChip(
              label: const Text('주민번호 가리기'),
              selected: calcMode == null && extraOptions.contains('mask_rrn'),
              onSelected: (selected) {
                setState(() {
                  calcMode = null;
                  if (selected) {
                    extraOptions.add('mask_rrn');
                  } else {
                    extraOptions.remove('mask_rrn');
                  }
                });
                run();
              },
            ),
          ],
        ),
        const SizedBox(height: 6),
        Wrap(
          spacing: 6,
          runSpacing: 4,
          crossAxisAlignment: WrapCrossAlignment.center,
          children: [
            const Text(
              '계산 도구:',
              style: TextStyle(fontWeight: FontWeight.bold, fontSize: 13),
            ),
            ChoiceChip(
              label: const Text('날짜 차이 (두 날짜를 줄로 구분)'),
              selected: calcMode == 'date_diff',
              onSelected: (selected) {
                setState(() {
                  calcMode = selected ? 'date_diff' : null;
                });
                run();
              },
            ),
            ChoiceChip(
              label: const Text('만 나이'),
              selected: calcMode == 'age',
              onSelected: (selected) {
                setState(() {
                  calcMode = selected ? 'age' : null;
                });
                run();
              },
            ),
            ChoiceChip(
              label: const Text('표 합계'),
              selected: calcMode == 'table_calc',
              onSelected: (selected) {
                setState(() {
                  calcMode = selected ? 'table_calc' : null;
                });
                run();
              },
            ),
          ],
        ),
      ],
    );
  }

  @override
  Widget build(BuildContext context) => PopScope(
    canPop: closing || !busy,
    onPopInvokedWithResult: (didPop, _) {
      if (!didPop) leave();
    },
    child: Scaffold(
      appBar: AppBar(
        automaticallyImplyLeading: false,
        leading: !Navigator.of(context).canPop()
            ? null
            : IconButton(
                onPressed: busy ? null : leave,
                icon: const Icon(Icons.arrow_back),
              ),
        title: const Text('텍스트 입력'),
      ),
      body: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            _buildOptionSection(),
            if (calcMode == 'table_calc') ...[
              const SizedBox(height: 6),
              DropdownButtonFormField<String>(
                initialValue: calculation,
                items: const [
                  DropdownMenuItem(value: 'col_sum', child: Text('열 합계')),
                  DropdownMenuItem(value: 'col_avg', child: Text('열 평균')),
                  DropdownMenuItem(value: 'row_sum', child: Text('행 합계')),
                  DropdownMenuItem(value: 'col_ratio', child: Text('열 비율')),
                ],
                onChanged: busy
                    ? null
                    : (value) {
                        setState(() => calculation = value ?? 'col_sum');
                        run();
                      },
              ),
            ],
            const SizedBox(height: 8),
            Wrap(
              spacing: 8,
              runSpacing: 4,
              children: [
                FilledButton(
                  onPressed: busy ? null : run,
                  child: const Text('실행 / 미리보기'),
                ),
                TextButton(
                  onPressed: busy
                      ? null
                      : () =>
                            Clipboard.setData(ClipboardData(text: output.text)),
                  child: const Text('결과 복사'),
                ),
                OutlinedButton(
                  onPressed: busy ? null : addDocument,
                  child: const Text('문서 목록에 추가'),
                ),
              ],
            ),
            if (busy) const LinearProgressIndicator(),
            if (status.isNotEmpty) Text(status),
            const SizedBox(height: 8),
            Expanded(
              child: LayoutBuilder(
                builder: (context, box) {
                  final result = TextField(
                    controller: output,
                    readOnly: true,
                    maxLines: null,
                    expands: true,
                    textAlignVertical: TextAlignVertical.top,
                    decoration: const InputDecoration(
                      labelText: '변환 결과',
                      alignLabelWithHint: true,
                      border: OutlineInputBorder(),
                    ),
                  );
                  if (box.maxWidth < 760) {
                    return Column(
                      children: [
                        Expanded(child: editor()),
                        const SizedBox(height: 8),
                        Expanded(child: result),
                      ],
                    );
                  }
                  return Row(
                    children: [
                      Expanded(child: editor()),
                      const SizedBox(width: 12),
                      Expanded(child: result),
                    ],
                  );
                },
              ),
            ),
          ],
        ),
      ),
    ),
  );
}
