import 'package:web/web.dart' as web;

const _key = 'docfit.quickBarVisible';

/// 빠른 실행 막대 보임 여부를 웹 저장소에 기억한다(새 Python 설정 키 없이). 읽기·쓰기 실패는 무시한다.
bool readQuickBarVisible() {
  try {
    return web.window.localStorage.getItem(_key) != '0';
  } catch (_) {
    return true;
  }
}

void writeQuickBarVisible(bool value) {
  try {
    web.window.localStorage.setItem(_key, value ? '1' : '0');
  } catch (_) {}
}
