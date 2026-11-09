import 'package:flutter/widgets.dart';

/// VS Code Codicon 아이콘 폰트 상수 모음 (공식 codicon.ttf v0.0.46-24 기반).
/// 코드포인트는 공식 mapping.json 및 codicon.csv에서 추출.
/// 라이선스: CC BY 4.0 (assets/fonts/codicon/LICENSE 참고).
class Codicons {
  Codicons._();

  static const String _fontFamily = 'Codicon';

  // 1. 공통 및 기본 편집 조작
  static const IconData add = IconData(0xEA60, fontFamily: _fontFamily);
  static const IconData plus = IconData(0xEA60, fontFamily: _fontFamily);
  static const IconData close = IconData(0xEA76, fontFamily: _fontFamily);
  static const IconData edit = IconData(0xEA73, fontFamily: _fontFamily);
  static const IconData replace = IconData(0xEB3D, fontFamily: _fontFamily);
  static const IconData save = IconData(0xEB4B, fontFamily: _fontFamily);
  static const IconData search = IconData(0xEA6D, fontFamily: _fontFamily);
  static const IconData trash = IconData(0xEA81, fontFamily: _fontFamily);
  static const IconData refresh = IconData(0xEB37, fontFamily: _fontFamily);
  static const IconData sync = IconData(0xEA77, fontFamily: _fontFamily);
  static const IconData check = IconData(0xEAB2, fontFamily: _fontFamily);
  static const IconData warning = IconData(0xEA6C, fontFamily: _fontFamily);
  static const IconData error = IconData(0xEA87, fontFamily: _fontFamily);
  static const IconData info = IconData(0xEA74, fontFamily: _fontFamily);
  static const IconData question = IconData(0xEB32, fontFamily: _fontFamily);
  static const IconData gear = IconData(0xEAF8, fontFamily: _fontFamily);
  static const IconData settings = IconData(0xEB52, fontFamily: _fontFamily);
  static const IconData menu = IconData(0xEB94, fontFamily: _fontFamily);
  static const IconData ellipsis = IconData(0xEA7C, fontFamily: _fontFamily);
  static const IconData linkExternal = IconData(0xEB14, fontFamily: _fontFamily);
  static const IconData copy = IconData(0xEBCC, fontFamily: _fontFamily);

  // 2. 파일 및 폴더
  static const IconData file = IconData(0xEA7B, fontFamily: _fontFamily);
  static const IconData symbolFile = IconData(0xEA7B, fontFamily: _fontFamily);
  static const IconData files = IconData(0xEAF0, fontFamily: _fontFamily);
  static const IconData fileText = IconData(0xEC5E, fontFamily: _fontFamily);
  static const IconData filePdf = IconData(0xEAEB, fontFamily: _fontFamily);
  static const IconData fileMedia = IconData(0xEAEA, fontFamily: _fontFamily);
  static const IconData newFile = IconData(0xEA7F, fontFamily: _fontFamily);
  static const IconData folder = IconData(0xEA83, fontFamily: _fontFamily);
  static const IconData folderOpened = IconData(0xEAF7, fontFamily: _fontFamily);
  static const IconData newFolder = IconData(0xEA80, fontFamily: _fontFamily);

  // 3. 네비게이션 및 화살표
  static const IconData arrowLeft = IconData(0xEA9B, fontFamily: _fontFamily);
  static const IconData arrowRight = IconData(0xEA9C, fontFamily: _fontFamily);
  static const IconData arrowUp = IconData(0xEAA1, fontFamily: _fontFamily);
  static const IconData arrowDown = IconData(0xEA9A, fontFamily: _fontFamily);
  static const IconData arrowSwap = IconData(0xEBCB, fontFamily: _fontFamily);

  // 4. 사이드메뉴 / 워크스페이스 탭 아이콘
  static const IconData table = IconData(0xEBB7, fontFamily: _fontFamily);
  static const IconData paintcan = IconData(0xEB2A, fontFamily: _fontFamily);
  static const IconData graph = IconData(0xEB03, fontFamily: _fontFamily);
  static const IconData typeHierarchy = IconData(0xEBB9, fontFamily: _fontFamily);
  static const IconData organization = IconData(0xEA7E, fontFamily: _fontFamily);
  static const IconData symbolColor = IconData(0xEB5C, fontFamily: _fontFamily);
  static const IconData symbolRuler = IconData(0xEA96, fontFamily: _fontFamily);
  static const IconData history = IconData(0xEA82, fontFamily: _fontFamily);
  static const IconData archive = IconData(0xEA98, fontFamily: _fontFamily);
  static const IconData library = IconData(0xEB9C, fontFamily: _fontFamily);
  static const IconData book = IconData(0xEAA4, fontFamily: _fontFamily);
  static const IconData notebook = IconData(0xEBAF, fontFamily: _fontFamily);
  static const IconData listTree = IconData(0xEB86, fontFamily: _fontFamily);
  static const IconData clippy = IconData(0xEAC0, fontFamily: _fontFamily);
  static const IconData checklist = IconData(0xEAB3, fontFamily: _fontFamily);
  static const IconData output = IconData(0xEB9D, fontFamily: _fontFamily);
}
