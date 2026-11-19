import 'dart:async';

import 'package:flutter/material.dart';

import '../../core/api/abbreviation_api.dart';
import 'tab_navigation_shell.dart';
import 'dock_mode_controller.dart';

/// 사용자가 끈 왼쪽 메뉴 번호. 비어 있으면 모두 켠 상태다.
/// 저장값은 시작 때 get_state의 hidden_tabs로 채우고, 선택 창에서 저장하면 바로 바꾼다.
final ValueNotifier<Set<int>> hiddenTabsNotifier = ValueNotifier<Set<int>>(
  const <int>{},
);

/// 사이드바/Dock 선택과 실제 프로세스 전환을 연결하는 설정 컨트롤.
class MenuDisplayModeSetting extends StatefulWidget {
  const MenuDisplayModeSetting({
    super.key,
    required this.api,
    required this.controller,
  });
  final ApiCall api;
  final DockModeController controller;
  @override
  State<MenuDisplayModeSetting> createState() => _MenuDisplayModeSettingState();
}

class _MenuDisplayModeSettingState extends State<MenuDisplayModeSetting> {
  String _mode = 'sidebar';
  String? _error;
  bool _busy = true;
  @override
  void initState() {
    super.initState();
    unawaited(
      widget
          .api('get_state', [])
          .then((value) async {
            final mode = value is Map
                ? value['menu_display_mode'] as String?
                : null;
            if (mounted) {
              setState(() {
                _mode = mode == 'dock' ? 'dock' : 'sidebar';
                _busy = false;
              });
            }
          })
          .catchError((Object error) {
            if (mounted) {
              setState(() {
                _busy = false;
                _error = '설정 상태를 읽지 못했어요: $error';
              });
            }
          }),
    );
  }

  Future<void> _change(String? value) async {
    if (value == null || value == _mode) return;
    setState(() {
      _busy = true;
      _error = null;
    });
    final message = await widget.controller.select(value);
    if (mounted) {
      setState(() {
        _mode = message == null ? value : 'sidebar';
        _error = message;
        _busy = false;
      });
    }
  }

  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      ListTile(
        key: const Key('menuDisplayModeSetting'),
        contentPadding: EdgeInsets.zero,
        leading: const Icon(Icons.dock_outlined),
        title: const Text('메뉴 표시 방식'),
        subtitle: const Text('사이드바 또는 하단 Dock을 선택해요. Dock은 준비된 뒤 사이드바를 전환해요.'),
        trailing: DropdownButton<String>(
          key: const Key('menuDisplayModePicker'),
          value: _mode,
          items: const [
            DropdownMenuItem(value: 'sidebar', child: Text('사이드바')),
            DropdownMenuItem(value: 'dock', child: Text('하단 Dock')),
          ],
          onChanged: _busy ? null : _change,
        ),
      ),
      if (_error != null)
        Padding(
          padding: const EdgeInsets.only(bottom: 8),
          child: Text(
            _error!,
            key: const Key('menuDisplayModeError'),
            style: TextStyle(color: Theme.of(context).colorScheme.error),
          ),
        ),
    ],
  );
}

/// 저장값(JSON 목록)을 끌 수 있는 메뉴 번호 집합으로 정리한다. 0(문서편집)은 항상 켠다.
Set<int> parseHiddenTabs(Object? raw) {
  if (raw is! List) return const <int>{};
  return {
    for (final v in raw)
      if (v is int && v >= 1 && v < kTabCount && v != 12 && v != 13) v,
  };
}

/// 선택 창에 보일 메뉴: (탭 번호, 이름, 한 줄 설명). 문서편집(0)은 끌 수 없어 목록에서 빼고 안내만 한다.
const List<({int tab, String label, String hint})> kSidebarMenuChoices = [
  (tab: 1, label: '표템플릿', hint: '자주 쓰는 표 서식 고르기'),
  (tab: 2, label: '표그림판', hint: '그림 그리듯 새 표 만들기'),
  (tab: 3, label: '다이어그램', hint: '.drawio · .vsdx'),
  (tab: 4, label: '마인드맵', hint: '.xmind'),
  (tab: kTabPhotoEdit, label: '붙임사진 편집', hint: '붙임 사진 편집'),
  (tab: kTabFileRename, label: '파일명 일괄변경', hint: '여러 파일 이름 한 번에 바꾸기'),
  (tab: kTabTimemachine, label: '몽땅검색', hint: '작업 파일 시간순 찾기·위계검색'),
  (tab: kTabDocGrid, label: '문서뷰', hint: '문서를 모아 보기'),
  (tab: kTabPhotoAlbum, label: '사진뷰', hint: '사진 모아 보기'),
  (tab: kTabFileConversion, label: '파일변환', hint: 'HWPX · PDF · DOCX · MD 등'),
  (tab: kTabPdfEdit, label: 'PDF 편집', hint: '합치기·나누기·용량 줄이기'),
  (tab: kTabOutliner, label: '아웃라이너', hint: 'Logseq식 블록 메모와 일지'),
  (tab: kTabQmsBench, label: '검색 벤치마크', hint: 'Everything 대 몽땅검색 성능 측정'),
];

/// '쓸 메뉴를 고르세요' 창. [저장]을 누르면 설정에 저장하고 왼쪽 메뉴에 바로 반영한다.
/// 끈 메뉴는 화면에서만 빠지며 열려 있는 작업은 닫지 않는다.
Future<void> showSidebarMenuDialog(BuildContext context, ApiCall api) =>
    showDialog<void>(
      context: context,
      builder: (_) => _SidebarMenuDialog(api: api),
    );

class _SidebarMenuDialog extends StatefulWidget {
  const _SidebarMenuDialog({required this.api});
  final ApiCall api;

  @override
  State<_SidebarMenuDialog> createState() => _SidebarMenuDialogState();
}

class _SidebarMenuDialogState extends State<_SidebarMenuDialog> {
  late final Set<int> _hidden = {...hiddenTabsNotifier.value};
  bool _saving = false;
  String? _error;

  Future<void> _save() async {
    setState(() {
      _saving = true;
      _error = null;
    });
    try {
      final saved = await widget.api('set_hidden_tabs', [
        (_hidden.toList()..sort()),
      ]);
      hiddenTabsNotifier.value = parseHiddenTabs(saved);
      if (mounted) Navigator.of(context).pop();
    } catch (e) {
      if (mounted) {
        setState(() {
          _saving = false;
          _error = '저장하지 못했어요: $e';
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final muted = Theme.of(context).colorScheme.onSurfaceVariant;
    return AlertDialog(
      key: const Key('sidebarMenuDialog'),
      title: const Text('쓸 메뉴를 고르세요'),
      content: SizedBox(
        width: 560,
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                '이제 쓰지 않는 메뉴를 끌 수 있어요. 끈 메뉴는 왼쪽 메뉴에서 빠지고, 열려 있는 작업은 닫히지 않아요. '
                '나중에 설정에서 다시 바꿀 수 있어요. 문서편집은 항상 켜져 있어요.',
                style: TextStyle(fontSize: 13, color: muted),
              ),
              const SizedBox(height: 12),
              Wrap(
                spacing: 16,
                runSpacing: 4,
                children: [
                  for (final item in kSidebarMenuChoices)
                    SizedBox(
                      width: 260,
                      child: SwitchListTile(
                        key: Key('sidebarMenuSwitch-${item.tab}'),
                        dense: true,
                        contentPadding: EdgeInsets.zero,
                        title: Text(item.label),
                        subtitle: Text(
                          item.hint,
                          style: const TextStyle(fontSize: 12),
                        ),
                        value: !_hidden.contains(item.tab),
                        onChanged: _saving
                            ? null
                            : (on) => setState(() {
                                if (on) {
                                  _hidden.remove(item.tab);
                                } else {
                                  _hidden.add(item.tab);
                                }
                              }),
                      ),
                    ),
                ],
              ),
              if (_error != null)
                Padding(
                  padding: const EdgeInsets.only(top: 8),
                  child: Text(
                    _error!,
                    style: TextStyle(
                      color: Theme.of(context).colorScheme.error,
                    ),
                  ),
                ),
            ],
          ),
        ),
      ),
      actions: [
        TextButton(
          key: const Key('sidebarMenuClose'),
          onPressed: _saving ? null : () => Navigator.of(context).pop(),
          child: const Text('닫기'),
        ),
        FilledButton(
          key: const Key('sidebarMenuSave'),
          onPressed: _saving ? null : _save,
          child: const Text('저장'),
        ),
      ],
    );
  }
}
