"""표 서식모음 한컴 PUA 문자(U+F0000~) 미리보기 치환 및 원본 보존 회귀 테스트."""

import re
import tempfile
import unittest
import zipfile
from pathlib import Path

from docfit_core.hwp_pua import (
    HANCOM_PUA_MAP,
    has_pua_chars,
    replace_pua_chars,
)
from docfit_core.table_catalog import (
    TABLE_PREVIEW_PUA_MAP,
    default_catalog_path,
    export_template,
    list_catalog,
)

_PLANE15_PUA_RE = re.compile(r"[\U000F0000-\U000FFFFD]")


class TestTablePuaPreview(unittest.TestCase):
    def test_pua_mapping_contains_investigated_codepoints(self):
        """U+F02B1(사각형 1) 및 U+F02FA(불릿)을 포함한 매핑표가 명시되어 있어야 한다."""
        self.assertIn("\U000F02B1", TABLE_PREVIEW_PUA_MAP)
        self.assertEqual(TABLE_PREVIEW_PUA_MAP["\U000F02B1"], "[1]")

        self.assertIn("\U000F02FA", TABLE_PREVIEW_PUA_MAP)
        self.assertEqual(TABLE_PREVIEW_PUA_MAP["\U000F02FA"], "·")

        # 1~20 사각형 번호
        for i in range(1, 21):
            ch = chr(0xF02B1 + i - 1)
            self.assertEqual(TABLE_PREVIEW_PUA_MAP.get(ch), f"[{i}]")

        # 불릿 및 기호
        self.assertEqual(TABLE_PREVIEW_PUA_MAP.get("\U000F02F8"), "●")
        self.assertEqual(TABLE_PREVIEW_PUA_MAP.get("\U000F02F9"), "○")
        self.assertEqual(TABLE_PREVIEW_PUA_MAP.get("\U000F02FB"), "▪")
        self.assertEqual(TABLE_PREVIEW_PUA_MAP.get("\U000F02FC"), "▫")

    def test_catalog_preview_cells_have_zero_pua(self):
        """resources/서식예시/표 서식모음.hwpx 미리보기 추출 결과 모든 셀 텍스트에 raw U+F0000~U+FFFFF 글자가 0건이어야 한다."""
        cat = list_catalog()
        leaked_pua_cells = []

        for category in cat.get("categories", []):
            cat_name = category.get("name", "")
            if _PLANE15_PUA_RE.search(cat_name):
                leaked_pua_cells.append(f"Category: {cat_name}")

            for tmpl in category.get("templates", []):
                tmpl_name = tmpl.get("name", "")
                if _PLANE15_PUA_RE.search(tmpl_name):
                    leaked_pua_cells.append(f"Template name: {tmpl_name}")

                for r_idx, row in enumerate(tmpl.get("cells", [])):
                    for c_idx, cell_text in enumerate(row):
                        if _PLANE15_PUA_RE.search(cell_text):
                            leaked_pua_cells.append(
                                f"Cell[{tmpl_name} r{r_idx} c{c_idx}]: {cell_text!r}"
                            )

                for r_idx, row in enumerate(tmpl.get("cell_meta", [])):
                    for c_idx, meta in enumerate(row):
                        meta_text = meta.get("text", "")
                        if _PLANE15_PUA_RE.search(meta_text):
                            leaked_pua_cells.append(
                                f"MetaText[{tmpl_name} r{r_idx} c{c_idx}]: {meta_text!r}"
                            )
                        for s_idx, shape in enumerate(meta.get("shapes", [])):
                            shape_text = shape.get("text", "")
                            if _PLANE15_PUA_RE.search(shape_text):
                                leaked_pua_cells.append(
                                    f"ShapeText[{tmpl_name} r{r_idx} c{c_idx} s{s_idx}]: {shape_text!r}"
                                )

        self.assertEqual(
            leaked_pua_cells,
            [],
            f"카탈로그 미리보기 텍스트에 한컴 PUA 문자가 남아있습니다: {leaked_pua_cells}",
        )

    def test_label_shape_3_preview_and_original_hwpx_unchanged(self):
        """라벨 서식 · 모양 3 미리보기 글자가 사각형 번호 [1]로 변환되고 원본 HWPX 파일은 변경되지 않는지 검증."""
        # 1) 원본 HWPX 파일 자체의 SHA 또는 바이트/내용 확인 (수정되지 않음)
        catalog_path = default_catalog_path()
        self.assertTrue(catalog_path.is_file(), f"카탈로그 파일이 존재해야 합니다: {catalog_path}")

        with zipfile.ZipFile(catalog_path, "r") as z:
            sec0_orig = z.read("Contents/section0.xml").decode("utf-8")
            # 원본에는 U+F02B1이 그대로 보존되어 있어야 함
            self.assertIn("\U000F02B1", sec0_orig, "카탈로그 원본 파일의 U+F02B1이 보존되어야 합니다.")

        # 2) 카탈로그 미리보기 추출 결과 확인
        cat = list_catalog()
        label_3 = None
        for category in cat.get("categories", []):
            if "라벨" in category.get("name", ""):
                for tmpl in category.get("templates", []):
                    if "모양 3" in tmpl.get("name", ""):
                        label_3 = tmpl
                        break

        self.assertIsNotNone(label_3, "라벨 서식 · 모양 3을 찾을 수 없습니다.")
        self.assertNotIn("\U000F02B1", label_3["cells"][0][0])
        self.assertEqual(label_3["cells"][0][0], "[1] 푸른하늘 은하수")
        self.assertEqual(label_3["cell_meta"][0][0]["text"], "[1] 푸른하늘 은하수")

        # 3) 한/글로 복사/내보내기한 HWPX 파일에는 원본 글자(U+F02B1)가 보존되는지 검증
        with tempfile.TemporaryDirectory() as tmp_dir:
            out_file = Path(tmp_dir) / "exported_label3.hwpx"
            export_template(label_3["id"], out_file)
            self.assertTrue(out_file.is_file())

            with zipfile.ZipFile(out_file, "r") as z_out:
                sec0_out = z_out.read("Contents/section0.xml").decode("utf-8")
                self.assertIn(
                    "\U000F02B1",
                    sec0_out,
                    "내보낸 HWPX XML에 원본 한컴 PUA 코드포인트(U+F02B1)가 그대로 보존되어야 합니다.",
                )


if __name__ == "__main__":
    unittest.main()
