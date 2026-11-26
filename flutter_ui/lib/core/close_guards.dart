import 'dart:async';
import 'package:flutter/material.dart';

/// A host may exit only after every editor has saved or explicitly discarded
/// changes AND its route has been disposed. Pending RPCs veto the close.
class CloseLease {
  CloseLease(this.prepare);
  final Future<bool> Function() prepare;
  final disposed = Completer<void>();
  void release() {
    CloseGuards._leases.remove(this);
    if (!disposed.isCompleted) disposed.complete();
  }
}

/// 화면에 머무는(닫을 라우트가 없는) 편집기의 '잃을 수 있는 작업'.
/// 상태: 없음([unsaved] null) · 저장 안 함([unsaved] 설명) · 저장·처리 중([saving] true, 지금은 닫을 수 없음).
class WorkWatch {
  WorkWatch(this.unsaved, this.saving);
  final String? Function() unsaved;
  final bool Function() saving;
  void release() => CloseGuards._watches.remove(this);
}

class CloseGuards {
  static final _leases = <CloseLease>[];
  static final _watches = <WorkWatch>[];

  static WorkWatch watch({required String? Function() unsaved, bool Function()? saving}) {
    final watch = WorkWatch(unsaved, saving ?? () => false);
    _watches.add(watch);
    return watch;
  }

  /// 앱(창)을 닫아도 되는지. 저장·처리 중이면 거절하고, 저장 안 한 작업이 있으면 한 번 묻는다.
  static Future<bool> confirmWork(BuildContext context) async {
    if (_watches.any((w) => w.saving())) return false;
    final names = [for (final w in _watches) ?w.unsaved()];
    if (names.isEmpty) return true;
    return await showDialog<bool>(
          context: context,
          builder: (context) => AlertDialog(
            key: const ValueKey('closeUnsavedDialog'),
            title: const Text('저장하지 않은 작업이 있습니다'),
            content: Text('${names.map((n) => '· $n').join('\n')}\n\n버리고 닫을까요?'),
            actions: [
              TextButton(
                onPressed: () => Navigator.pop(context, false),
                child: const Text('계속 작업'),
              ),
              FilledButton(
                onPressed: () => Navigator.pop(context, true),
                child: const Text('버리고 닫기'),
              ),
            ],
          ),
        ) ??
        false;
  }

  static CloseLease register(Future<bool> Function() prepare) {
    final lease = CloseLease(prepare);
    _leases.add(lease);
    return lease;
  }

  static Future<bool> prepareAll() async {
    for (final lease in List<CloseLease>.of(_leases).reversed) {
      if (!_leases.contains(lease)) continue;
      if (!await lease.prepare()) return false;
      try {
        await lease.disposed.future.timeout(const Duration(seconds: 3));
      } on TimeoutException {
        return false;
      }
    }
    return _leases.isEmpty;
  }
}

Future<bool> confirmHostDiscard(BuildContext context) async =>
    await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('저장하지 않은 변경사항'),
        content: const Text('변경사항을 버리고 앱을 종료할까요? 저장하려면 계속 편집을 선택하세요.'),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: const Text('계속 편집'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, true),
            child: const Text('변경사항 버리기'),
          ),
        ],
      ),
    ) ??
    false;

bool canCloseEditor(BuildContext context) =>
    ModalRoute.of(context)?.isCurrent == true && Navigator.of(context).canPop();
