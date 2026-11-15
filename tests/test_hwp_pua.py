"""한컴 PUA 문자 치환 및 표 템플릿 미리보기/내보내기 회귀 테스트."""

import tempfile
import unittest
import zipfile
from pathlib import Path

from docfit_core.hwp_pua import (
    HANCOM_PUA_MAP,
    has_pua_chars,
    replace_pua_chars,
)
from docfit_core.table_catalog import export_template, list_catalog


class TestHwpPua(unittest.TestCase):
    def test_pua_detection(self):
        self.assertTrue(has_pua_chars("\U000F02B1 푸른하늘"))
        self.assertTrue(has_pua_chars("불릿 \U000F02FA"))
        self.assertFalse(has_pua_chars("[1] 푸른하늘"))
        self.assertFalse(has_pua_chars("일반 텍스트 123 !@#"))

    def test_squared_number_mapping(self):
        # 1부터 20까지 사각형 안 숫자가 [1] ~ [20]으로 변환되는지 확인
        for i in range(1, 21):
            pua_char = chr(0xF02B1 + i - 1)
            expected = f"[{i}]"
            self.assertEqual(replace_pua_chars(f"{pua_char} 제목"), f"{expected} 제목")

    def test_bullet_mapping(self):
        self.assertEqual(replace_pua_chars("\U000F02FA\U000F02FA\U000F02FA"), "···")

    def test_catalog_preview_has_no_pua(self):
        """카탈로그 전체의 미리보기 텍스트에 U+F0000대 글자가 남아있지 않아야 한다."""
        cat = list_catalog()
        found_pua = []
        for category in cat.get("categories", []):
            for tmpl in category.get("templates", []):
                for r, row in enumerate(tmpl.get("cells", [])):
                    for c, text in enumerate(row):
                        if has_pua_chars(text):
                            found_pua.append((category.get("name"), tmpl.get("name"), r, c, text))
                for r, row in enumerate(tmpl.get("cell_meta", [])):
                    for c, meta in enumerate(row):
                        meta_text = meta.get("text", "")
                        if has_pua_chars(meta_text):
                            found_pua.append((category.get("name"), tmpl.get("name"), r, c, meta_text))

        self.assertEqual(found_pua, [], "카탈로그 미리보기에 PUA 문자가 남아있습니다.")

    def test_label_shape_3_preview(self):
        """라벨 서식 · 모양 3의 미리보기가 [1] 푸른하늘 은하수로 표시되는지 확인."""
        cat = list_catalog()
        label_shape_3 = None
        for category in cat.get("categories", []):
            if "라벨" in category.get("name", ""):
                for tmpl in category.get("templates", []):
                    if "모양 3" in tmpl.get("name", ""):
                        label_shape_3 = tmpl
                        break
        self.assertIsNotNone(label_shape_3, "라벨 서식 · 모양 3 템플릿을 찾을 수 없습니다.")
        self.assertEqual(label_shape_3["cells"][0][0], "[1] 푸른하늘 은하수")
        self.assertEqual(label_shape_3["cell_meta"][0][0]["text"], "[1] 푸른하늘 은하수")

    def test_export_preserves_original_pua(self):
        """한/글로 복사·내보내는 HWPX에서는 원본 글자(U+F02B1)가 변조 없이 보존되어야 한다."""
        cat = list_catalog()
        target_id = None
        for category in cat.get("categories", []):
            if "라벨" in category.get("name", ""):
                for tmpl in category.get("templates", []):
                    if "모양 3" in tmpl.get("name", ""):
                        target_id = tmpl.get("id")
                        break
        self.assertIsNotNone(target_id)

        with tempfile.TemporaryDirectory() as tmp_dir:
            out_path = Path(tmp_dir) / "label3.hwpx"
            export_template(target_id, out_path)
            self.assertTrue(out_path.is_file())

            with zipfile.ZipFile(out_path, "r") as z:
                sec0 = z.read("Contents/section0.xml").decode("utf-8")
                self.assertIn("\U000f02b1", sec0, "내보낸 HWPX에서 원본 한컴 PUA 코드가 보존되지 않았습니다.")


if __name__ == "__main__":
    unittest.main()
