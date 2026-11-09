import 'package:web/web.dart' as web;

import 'kclip_category_prefs.dart';

String? readCategory() => web.window.localStorage.getItem(kclipCategoryPrefKey);

void writeCategory(String category) =>
    web.window.localStorage.setItem(kclipCategoryPrefKey, category);
