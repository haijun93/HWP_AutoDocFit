import 'package:flutter/material.dart';

/// 문서뷰·사진뷰에서 고른 파일을 앱의 다른 기능으로 넘기는 통로(CTXMENU1).

enum FileTarget { documentEditor, virtualFolder, notebook, conversion, pdfEdit }

/// 대상별로 받을 수 있는 형식(null 이면 형식 제한 없음). 대상 기능의 기존 진입 경로 규칙을 따른다.
const Map<FileTarget, Set<String>?> kFileTargetExtensions = {
  // add_paths 가 받는 문서 형식.
  FileTarget.documentEditor: {'hwp', 'hwpx', 'pdf', 'doc', 'docx', 'txt', 'md'},
  FileTarget.virtualFolder: null,
  // notebook_service.SOURCE_EXTENSIONS.
  FileTarget.notebook: {'hwpx', 'hwp', 'docx', 'pdf', 'xlsx', 'pptx', 'txt', 'md'},
  // add_paths 가 받고 파일변환 입력(CONVERSION_INPUTS)이기도 한 형식.
  FileTarget.conversion: {
    'hwp', 'hwpx', 'pdf', 'docx', 'txt', 'md', //
    'png', 'jpg', 'jpeg', 'webp', 'bmp', 'tif', 'tiff', 'gif',
  },
  FileTarget.pdfEdit: {'pdf'},
};

/// 한 번에 넘길 수 있는 개수.
const Map<FileTarget, int> kFileTargetLimits = {
  FileTarget.documentEditor: 1000,
  FileTarget.virtualFolder: 200,
  FileTarget.notebook: 200,
  FileTarget.conversion: 1000,
  FileTarget.pdfEdit: 100,
};

String _extensionOf(String path) {
  final name = path.split(RegExp(r'[\\/]')).last;
  final dot = name.lastIndexOf('.');
  return dot < 0 ? '' : name.substring(dot + 1).toLowerCase();
}

/// [paths]를 [target]에 넘길 수 없는 이유. 넘길 수 있으면 null.
String? fileTargetReason(FileTarget target, List<String> paths) {
  if (paths.isEmpty) return '선택한 파일이 없습니다';
  final limit = kFileTargetLimits[target]!;
  if (paths.length > limit) return '한 번에 $limit개까지 넘길 수 있습니다';
  final allowed = kFileTargetExtensions[target];
  if (allowed == null) return null;
  final bad = {
    for (final path in paths)
      if (!allowed.contains(_extensionOf(path))) _extensionOf(path),
  };
  if (bad.isEmpty) return null;
  if (target == FileTarget.pdfEdit) return 'PDF 파일만 편집할 수 있습니다';
  final names = [for (final ext in bad) ext.isEmpty ? '확장자 없음' : '.$ext'];
  return '지원하지 않는 형식이 있습니다(${names.join(', ')})';
}

/// 대상 화면이 가져갈 파일 목록. 화면은 만들어질 때 또는 알림을 받고 [take]로 한 번만 가져간다.
class FileInbox extends ChangeNotifier {
  List<String> _paths = const [];

  void put(List<String> paths) {
    _paths = List.unmodifiable(paths);
    notifyListeners();
  }

  List<String> take() {
    final paths = _paths;
    _paths = const [];
    return paths;
  }
}

/// 앱 껍데기가 제공하는 전달 동작. 없으면(화면만 따로 쓸 때) 연결 메뉴를 만들지 않는다.
class FileHandoff extends InheritedWidget {
  const FileHandoff({
    super.key,
    required this.send,
    required this.blocked,
    required super.child,
  });

  /// 파일을 넘기고 그 화면을 연다. 넘기지 못했거나 알릴 것이 있으면 안내 문장을 돌려준다.
  final Future<String?> Function(FileTarget target, List<String> paths) send;

  /// 대상 화면을 지금 열 수 없는 이유(숨긴 메뉴 등). 열 수 있으면 null.
  final String? Function(FileTarget target) blocked;

  static FileHandoff? maybeOf(BuildContext context) =>
      context.getInheritedWidgetOfExactType<FileHandoff>();

  @override
  bool updateShouldNotify(FileHandoff oldWidget) => false;
}

/// 오른쪽 클릭 메뉴의 한 줄. [disabledReason]이 있으면 누를 수 없고 이유를 함께 보인다.
class FileMenuEntry {
  const FileMenuEntry({
    required this.id,
    required this.label,
    required this.icon,
    required this.onSelected,
    this.disabledReason,
  });

  final String id;
  final String label;
  final IconData icon;
  final VoidCallback onSelected;
  final String? disabledReason;
}

/// 연결 대상 메뉴 한 줄을 만든다. 형식·개수·숨긴 화면이면 비활성으로 둔다.
FileMenuEntry handoffEntry(
  BuildContext context,
  FileHandoff handoff,
  FileTarget target,
  List<String> paths, {
  required String label,
  required IconData icon,
}) {
  final messenger = ScaffoldMessenger.of(context);
  return FileMenuEntry(
    id: target.name,
    label: label,
    icon: icon,
    disabledReason: handoff.blocked(target) ?? fileTargetReason(target, paths),
    onSelected: () async {
      String? message;
      try {
        message = await handoff.send(target, paths);
      } catch (e) {
        message = '파일을 넘기지 못했습니다: $e';
      }
      if (message != null && messenger.mounted) {
        messenger.showSnackBar(SnackBar(content: Text(message)));
      }
    },
  );
}

/// 가상 폴더·업무자료함 가운데 어디에 넣을지 고른 결과.
class BoxChoice {
  const BoxChoice.existing(this.index) : newName = null;
  const BoxChoice.create(this.newName) : index = null;

  final int? index;
  final String? newName;
}

/// 넣을 곳을 고르거나 새 이름을 받는다. 취소하면 null.
Future<BoxChoice?> showBoxPickDialog(
  BuildContext context, {
  required String kind,
  required int fileCount,
  required List<String> names,
}) => showDialog<BoxChoice>(
  context: context,
  builder: (_) => _BoxPickDialog(kind: kind, fileCount: fileCount, names: names),
);

class _BoxPickDialog extends StatefulWidget {
  const _BoxPickDialog({
    required this.kind,
    required this.fileCount,
    required this.names,
  });

  final String kind;
  final int fileCount;
  final List<String> names;

  @override
  State<_BoxPickDialog> createState() => _BoxPickDialogState();
}

class _BoxPickDialogState extends State<_BoxPickDialog> {
  late final _name = TextEditingController(text: '새 ${widget.kind}');

  @override
  void dispose() {
    _name.dispose();
    super.dispose();
  }

  void _create() {
    final name = _name.text.trim();
    if (name.isNotEmpty) Navigator.pop(context, BoxChoice.create(name));
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    key: const ValueKey('boxPickDialog'),
    title: Text('파일 ${widget.fileCount}개를 넣을 ${widget.kind}'),
    content: SizedBox(
      width: 420,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Flexible(
            child: ListView(
              shrinkWrap: true,
              children: [
                for (var i = 0; i < widget.names.length; i++)
                  ListTile(
                    key: ValueKey('boxPick-$i'),
                    dense: true,
                    title: Text(widget.names[i]),
                    onTap: () => Navigator.pop(context, BoxChoice.existing(i)),
                  ),
              ],
            ),
          ),
          Row(
            children: [
              Expanded(
                child: TextField(
                  key: const ValueKey('boxPickNewName'),
                  controller: _name,
                  decoration: InputDecoration(labelText: '새 ${widget.kind} 이름'),
                  onSubmitted: (_) => _create(),
                ),
              ),
              TextButton(
                key: const ValueKey('boxPickCreate'),
                onPressed: _create,
                child: const Text('만들어 넣기'),
              ),
            ],
          ),
        ],
      ),
    ),
    actions: [
      TextButton(
        key: const ValueKey('boxPickCancel'),
        onPressed: () => Navigator.pop(context),
        child: const Text('취소'),
      ),
    ],
  );
}
