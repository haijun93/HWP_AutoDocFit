import 'package:web/web.dart' as web;

/// 앱 창(웹뷰)의 localStorage에 보기 선택을 남긴다.
String? readPref(String key) => web.window.localStorage.getItem(key);

void writePref(String key, String value) => web.window.localStorage.setItem(key, value);
