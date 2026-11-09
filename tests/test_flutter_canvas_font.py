"""캔버스(CustomPainter)에 그리는 글자는 앱 테마 글꼴을 물려받지 않는다.

내장 한글 글꼴(NotoSansKR)을 직접 지정하지 않으면 오프라인에서 한글이 □로 보인다
(표그림판 2.0.5, 표템플릿 미리보기 2.2.42). 이 시험은 해당 파일의 TextPainter마다 글꼴을
지정하는지 소스에서 확인한다(Flutter 없이 실행 가능).
"""
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PAINTER_FILES = [
    "flutter_ui/lib/features/table_units/table_grid_preview.dart",
    "flutter_ui/lib/features/table_sketch/table_sketch_painter.dart",
]


class CanvasFontTests(unittest.TestCase):
    def test_painter_text_painters_set_embedded_korean_font(self):
        for rel in PAINTER_FILES:
            source = (ROOT / rel).read_text(encoding="utf-8")
            painters = len(re.findall(r"TextPainter\(", source))
            fonts = len(re.findall(r"fontFamily: 'NotoSansKR'", source))
            self.assertGreater(painters, 0, rel)
            self.assertGreaterEqual(
                fonts, painters,
                f"{rel}: TextPainter {painters}곳에 NotoSansKR 지정이 {fonts}곳뿐입니다(한글이 □로 보일 수 있음)")

    def test_embedded_font_is_declared_in_pubspec(self):
        pubspec = (ROOT / "flutter_ui/pubspec.yaml").read_text(encoding="utf-8")
        self.assertIn("family: NotoSansKR", pubspec)


if __name__ == "__main__":
    unittest.main()
