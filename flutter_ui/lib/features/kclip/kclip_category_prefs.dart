import 'kclip_category_prefs_stub.dart'
    if (dart.library.js_interop) 'kclip_category_prefs_web.dart'
    as platform;

/// 몽땅클립보드 관리창의 마지막 분류 선택을 저장한다.
abstract interface class KClipCategoryPrefs {
  Future<String?> read();
  Future<void> write(String category);
}

class DefaultKClipCategoryPrefs implements KClipCategoryPrefs {
  const DefaultKClipCategoryPrefs();

  @override
  Future<String?> read() async {
    try {
      return platform.readCategory();
    } catch (_) {
      return null;
    }
  }

  @override
  Future<void> write(String category) async {
    try {
      platform.writeCategory(category);
    } catch (_) {}
  }
}

/// 시험용 저장소. 같은 인스턴스를 다시 주입하면 화면을 닫았다 열어도 값이 남는다.
class MemoryKClipCategoryPrefs implements KClipCategoryPrefs {
  MemoryKClipCategoryPrefs([this.value]);

  String? value;

  @override
  Future<String?> read() async => value;

  @override
  Future<void> write(String category) async => value = category;
}

const kclipCategoryPrefKey = 'docfit.kclip.category';
