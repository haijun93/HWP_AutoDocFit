/// 웹이 아닌 환경(시험·데스크톱)에서는 앱이 떠 있는 동안만 기억한다.
final _values = <String, String>{};

String? readPref(String key) => _values[key];

void writePref(String key, String value) => _values[key] = value;
