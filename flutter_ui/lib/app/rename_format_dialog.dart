import 'package:flutter/material.dart';

/// 서식 이름·기관 이름을 고치는 창. 입력 상자는 창이 완전히 닫힐 때 정리한다.
class RenameFormatDialog extends StatefulWidget {
  const RenameFormatDialog({
    super.key,
    required this.name,
    required this.organization,
  });
  final String name, organization;
  @override
  State<RenameFormatDialog> createState() => _RenameFormatDialogState();
}

class _RenameFormatDialogState extends State<RenameFormatDialog> {
  late final name = TextEditingController(text: widget.name);
  late final organization = TextEditingController(text: widget.organization);

  @override
  void dispose() {
    name.dispose();
    organization.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: const Text('서식 이름 바꾸기'),
    content: SizedBox(
      width: 380,
      child: SingleChildScrollView(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(
              key: const Key('formatNameField'),
              controller: name,
              autofocus: true,
              decoration: const InputDecoration(labelText: '서식 이름'),
            ),
            const SizedBox(height: 12),
            TextField(
              key: const Key('formatOrganizationField'),
              controller: organization,
              decoration: const InputDecoration(
                labelText: '기관 이름 (선택)',
                helperText: '비워 두면 기관 없이 보여요.',
              ),
            ),
          ],
        ),
      ),
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: const Text('취소'),
      ),
      FilledButton(
        onPressed: () => Navigator.pop(context, (
          name.text.trim(),
          organization.text.trim(),
        )),
        child: const Text('저장'),
      ),
    ],
  );
}
