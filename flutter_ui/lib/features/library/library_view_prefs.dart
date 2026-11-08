import 'library_view_prefs_stub.dart'
    if (dart.library.js_interop) 'library_view_prefs_web.dart' as platform;

/// 자료함 보기 방식. 기본은 지금까지의 캔버스형(4개 영역에 카드 자유 배치).
enum LibraryViewMode { canvas, explorer }

/// 보기 선택 저장·복원(시험에서 대체). 기본 구현은 웹(앱 창)에서 localStorage, 그 밖에는 메모리.
abstract interface class LibraryViewPrefs {
  Future<String?> read(String key);
  Future<void> write(String key, String value);
}

class DefaultLibraryViewPrefs implements LibraryViewPrefs {
  const DefaultLibraryViewPrefs();

  @override
  Future<String?> read(String key) async {
    try {
      return platform.readPref(key);
    } catch (_) {
      return null; // 저장소를 못 쓰면 기본값(캔버스)으로 연다.
    }
  }

  @override
  Future<void> write(String key, String value) async {
    try {
      platform.writePref(key, value);
    } catch (_) {}
  }
}

/// 시험·임시용 메모리 저장소.
class MemoryLibraryViewPrefs implements LibraryViewPrefs {
  MemoryLibraryViewPrefs([Map<String, String>? values]) : values = values ?? {};
  final Map<String, String> values;

  @override
  Future<String?> read(String key) async => values[key];

  @override
  Future<void> write(String key, String value) async => values[key] = value;
}

const libraryViewModeKey = 'docfit.library.viewMode';
const libraryExplorerStyleKey = 'docfit.library.explorerStyle';
const libraryPaneCountKey = 'docfit.library.paneCount';
