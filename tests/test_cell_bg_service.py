"""표 칸 그림 → 칸 배경 변환 서비스 단위 시험 (Python 서비스 시험).

- 원본 불변 보장 검증
- 사본 생성 및 이름 규칙 '(표배경변환)' 검증
- 중복 파일명 처리 ('(표배경변환) (2)') 검증
- 변환된 칸 수 보고 검증
- XML 파싱은 defusedxml 사용
- 한/글 COM 미사용 (순수 HWPX ZIP/XML 처리)
"""
from __future__ import annotations

import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest.mock import MagicMock

from defusedxml import ElementTree as ET

from docfit_app.api_contract import ServiceError
from docfit_app.cell_bg_service import (
    CELL_BG_SUFFIX,
    convert_cell_bg_service,
    make_cell_bg_output_path,
)
from desktop_web_ui import DesktopWebBridge

HP = "{http://www.hancom.co.kr/hwpml/2011/paragraph}"
HH = "{http://www.hancom.co.kr/hwpml/2011/head}"
HC = "{http://www.hancom.co.kr/hwpml/2011/core}"

NS = (
    f'xmlns:hp="{HP[1:-1]}" xmlns:hh="{HH[1:-1]}" xmlns:hc="{HC[1:-1]}" '
    'xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section"'
)
PNG = b"\x89PNG\r\n\x1a\n" + bytes(range(64))


def _pic(name: str) -> str:
    return (
        f'<hp:pic id="{name}"><hp:sz width="100" height="100"/>'
        f'<hc:img binaryItemIDRef="{name}" bright="0" contrast="0" effect="REAL_PIC" alpha="0"/></hp:pic>'
    )


def _cell(inner: str, bf: str = "3", extra: str = "") -> str:
    span_attr = extra or 'colSpan="1" rowSpan="1"'
    return (
        f'<hp:tc name="" header="0" borderFillIDRef="{bf}"><hp:subList><hp:p id="0">'
        f'<hp:run charPrIDRef="0">{inner}</hp:run></hp:p></hp:subList>'
        f'<hp:cellAddr colAddr="0" rowAddr="0"/><hp:cellSpan {span_attr}/></hp:tc>'
    )


HEADER = (
    f'<?xml version="1.0" encoding="UTF-8"?><hh:head {NS}><hh:refList>'
    '<hh:borderFills itemCnt="3">'
    '<hh:borderFill id="1"><hh:leftBorder type="NONE"/></hh:borderFill>'
    '<hh:borderFill id="2"><hh:leftBorder type="NONE"/></hh:borderFill>'
    '<hh:borderFill id="3" threeD="0"><hh:leftBorder type="SOLID" width="0.4 mm" color="#FF0000"/>'
    '<hc:fillBrush><hc:winBrush faceColor="#FFFF00" hatchColor="#000000" alpha="0"/></hc:fillBrush>'
    '</hh:borderFill></hh:borderFills></hh:refList></hh:head>'
)

# 셀 1: 단독 그림 (변환 대상)
# 셀 2: 그림 + 텍스트 (건너뜀)
# 셀 3: 다중 그림 (건너뜀)
# 셀 4: 텍스트 (건너뜀)
# 셀 5: 병합 단독 그림 (변환 대상)
# 셀 6: 빈 셀 (건너뜀)
SECTION = (
    f'<?xml version="1.0" encoding="UTF-8"?><hs:sec {NS}><hp:p id="1"><hp:run><hp:tbl rowCnt="3" colCnt="2">'
    '<hp:tr>' + _cell(_pic("image1")) + _cell(_pic("image2") + "<hp:t>설명문</hp:t>") + '</hp:tr>'
    '<hp:tr>' + _cell(_pic("image3") + _pic("image4")) + _cell("<hp:t>문자</hp:t>") + '</hp:tr>'
    '<hp:tr>' + _cell(_pic("image5"), bf="1", extra='colSpan="2" rowSpan="1"') + _cell("") + '</hp:tr>'
    '</hp:tbl></hp:run></hp:p>'
    '<hp:p id="2"><hp:run>' + _pic("outside") + '</hp:run></hp:p></hs:sec>'
)


def _make_sample_hwpx(path: Path) -> None:
    with zipfile.ZipFile(path, "w") as z:
        z.writestr("mimetype", "application/hwp+zip", compress_type=zipfile.ZIP_STORED)
        z.writestr("Contents/header.xml", HEADER)
        z.writestr("Contents/section0.xml", SECTION)
        z.writestr("Contents/content.hpf", "<opf/>")
        for n in range(1, 6):
            z.writestr(f"BinData/image{n}.png", PNG + bytes([n]))


class TestCellBgService(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.tmp_dir = Path(self.tmp.name)
        self.src = self.tmp_dir / "보고서.hwpx"
        _make_sample_hwpx(self.src)

    def test_original_unmodified_and_copy_created(self):
        """원본 불변 보장 및 사본 생성 규칙, 변환된 칸 수 보고 검증."""
        before_bytes = self.src.read_bytes()

        result = convert_cell_bg_service(self.src)

        # 1. 원본 파일은 단 1바이트도 수정되지 않아야 함 (원본 불변)
        self.assertEqual(self.src.read_bytes(), before_bytes)

        # 2. 결과 응답 구조 검증
        self.assertTrue(result["ok"])
        self.assertEqual(result["source"], str(self.src.resolve()))
        self.assertEqual(result["converted_count"], 2)  # 단독 그림 셀 2개 변환됨
        self.assertIn("2개", result["message"])

        # 3. 사본 파일 이름 규칙 '(표배경변환)' 확인
        expected_output = self.tmp_dir / f"보고서 ({CELL_BG_SUFFIX}).hwpx"
        self.assertEqual(Path(result["output"]), expected_output)
        self.assertTrue(expected_output.is_file())

        # 4. 생성된 사본 내부의 XML 구조 검증 (defusedxml 사용)
        with zipfile.ZipFile(expected_output) as z:
            sec_root = ET.fromstring(z.read("Contents/section0.xml"))
            head_root = ET.fromstring(z.read("Contents/header.xml"))

        # 첫 번째 셀의 pic이 사라지고 새로운 borderFillIDRef가 적용되었는지 확인
        first_tc = sec_root.find(f".//{HP}tbl/{HP}tr[1]/{HP}tc[1]")
        self.assertIsNotNone(first_tc)
        self.assertEqual(len(first_tc.findall(f".//{HP}pic")), 0)
        new_bf_id = first_tc.get("borderFillIDRef")
        self.assertNotEqual(new_bf_id, "3")

        # header.xml에 해당 new_bf_id의 imgBrush가 mode="TOTAL"로 존재하는지 확인
        new_bf = None
        for bf in head_root.findall(f".//{HH}borderFill"):
            if bf.get("id") == new_bf_id:
                new_bf = bf
                break
        self.assertIsNotNone(new_bf)
        brush = new_bf.find(f".//{HC}imgBrush")
        self.assertIsNotNone(brush)
        self.assertEqual(brush.get("mode"), "TOTAL")

    def test_duplicate_output_naming(self):
        """이미 (표배경변환).hwpx 파일이 있을 때 (2), (3) 등으로 충돌 방지 검증."""
        first_output = self.tmp_dir / f"보고서 ({CELL_BG_SUFFIX}).hwpx"
        first_output.write_bytes(b"dummy")

        candidate = make_cell_bg_output_path(self.src)
        expected = self.tmp_dir / f"보고서 ({CELL_BG_SUFFIX}) (2).hwpx"
        self.assertEqual(candidate, expected)

        # 실제로 서비스 실행 시 충돌 없이 (2)로 저장되는지 확인
        res = convert_cell_bg_service(self.src)
        self.assertEqual(Path(res["output"]), expected)
        self.assertTrue(expected.is_file())
        # 먼저 있던 dummy 파일도 훼손되지 않아야 함
        self.assertEqual(first_output.read_bytes(), b"dummy")

    def test_custom_output_path(self):
        """사용자가 명시한 별도 사본 경로로 정상 저장되고 원본은 불변인지 검증."""
        custom_out = self.tmp_dir / "custom_copy.hwpx"
        before_bytes = self.src.read_bytes()

        res = convert_cell_bg_service(self.src, custom_out)
        self.assertTrue(res["ok"])
        self.assertEqual(Path(res["output"]), custom_out.resolve())
        self.assertTrue(custom_out.is_file())
        self.assertEqual(self.src.read_bytes(), before_bytes)

    def test_error_when_output_is_same_as_source(self):
        """원본을 덮어쓰려 할 때 ServiceError 발생 검증 (원본 수정 절대 금지)."""
        with self.assertRaises(ServiceError) as ctx:
            convert_cell_bg_service(self.src, self.src)
        self.assertEqual(ctx.exception.code, "ORIGINAL_MODIFICATION_PROHIBITED")

    def test_error_non_existent_file(self):
        """존재하지 않는 파일 입력 시 에러 검증."""
        with self.assertRaises(ServiceError) as ctx:
            convert_cell_bg_service(self.tmp_dir / "non_existent.hwpx")
        self.assertEqual(ctx.exception.code, "FILE_NOT_FOUND")

    def test_error_invalid_extension(self):
        """hwpx가 아닌 확장자 파일 입력 시 에러 검증."""
        txt_file = self.tmp_dir / "test.txt"
        txt_file.write_text("sample")
        with self.assertRaises(ServiceError) as ctx:
            convert_cell_bg_service(txt_file)
        self.assertEqual(ctx.exception.code, "INVALID_FORMAT")


class TestBridgeConvertCellBg(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.tmp_dir = Path(self.tmp.name)
        self.src = self.tmp_dir / "문서.hwpx"
        _make_sample_hwpx(self.src)

    def test_bridge_convert_cell_bg_with_dict(self):
        gui = MagicMock()
        gui.files = [str(self.src)]
        bridge = DesktopWebBridge(gui)

        res = bridge.convert_cell_bg({"path": str(self.src)})
        self.assertTrue(res["ok"])
        self.assertEqual(res["converted_count"], 2)
        self.assertTrue(Path(res["output"]).is_file())

    def test_bridge_convert_cell_bg_from_gui_files_fallback(self):
        gui = MagicMock()
        gui.files = [str(self.src)]
        bridge = DesktopWebBridge(gui)

        # 인자 없이 호출 시 gui.files[0]으로 처리
        res = bridge.convert_cell_bg()
        self.assertTrue(res["ok"])
        self.assertEqual(res["converted_count"], 2)

    def test_bridge_convert_cell_bg_no_files(self):
        gui = MagicMock()
        gui.files = []
        bridge = DesktopWebBridge(gui)

        res = bridge.convert_cell_bg()
        self.assertFalse(res["ok"])
        self.assertEqual(res["error"]["code"], "FILE_SELECTION_REQUIRED")


if __name__ == "__main__":
    unittest.main()
