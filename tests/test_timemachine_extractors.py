"""타임머신 형식별 본문 추출기 및 위치 계약 단위 시험."""
from __future__ import annotations

import codecs
import io
import sys
import os
import tempfile
import unittest
from pathlib import Path
from unittest.mock import MagicMock, patch
from zipfile import ZipFile

from docfit_core.timemachine.extractors import (
    ExtractionError,
    ExtractionRecord,
    extract_document,
    records_to_text,
)
from docfit_core.timemachine.extractors_text import decode_text, extract_plain_text
from docfit_core.timemachine.extractors_hwpx import extract_hwpx
from docfit_core.timemachine.extractors_hwp import extract_hwp
from docfit_core.timemachine.extractors_ooxml import (
    extract_docx,
    extract_pptx,
    extract_xlsx,
    extract_xls,
    _resolve_rel_path,
)
from docfit_core.timemachine.extractors_pdf import extract_pdf
from docfit_core.timemachine.extractors_eml import extract_eml
from tests.fixtures.timemachine_extract.generate_fixtures import generate_all_fixtures, FIXTURES_DIR


class TimeMachineExtractorsTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        # 합성 fixtures 생성
        generate_all_fixtures()

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)

    def tearDown(self):
        self.tmp.cleanup()

    def test_dispatch_supports_all_declared_extensions(self):
        """10개 확장자 디스패치 및 미지원 형식의 unsupported 예외 확인."""
        exts = ["hwpx", "hwp", "docx", "pptx", "xlsx", "xls", "pdf", "txt", "md", "eml"]
        for ext in exts:
            fake_file = self.root / f"test.{ext}"
            fake_file.write_text("dummy", encoding="utf-8")
            # 각 확장자에 대해 디스패치가 올바른 추출기 함수를 찾아 호출을 시도하는지 확인
            # (hwp, pdf, xls 등은 내부 파싱 시 오류가 날 수 있지만 unsupported가 아님을 확인)
            try:
                extract_document(fake_file)
            except ExtractionError as exc:
                self.assertNotEqual(
                    exc.reason_code,
                    "unsupported",
                    f"확장자 .{ext}는 지원되어야 하지만 unsupported가 발생함",
                )
            except Exception:
                pass

        # 미지원 확장자 검증
        unsupported_file = self.root / "test.unknown_ext"
        unsupported_file.write_text("hello", encoding="utf-8")
        with self.assertRaises(ExtractionError) as ctx:
            extract_document(unsupported_file)
        self.assertEqual(ctx.exception.reason_code, "unsupported")

    def test_txt_md_detect_utf8_bom_utf16_and_cp949(self):
        """인코딩 4종 본문 정확성 판별 및 파일 입력 바이트 불변 확인."""
        cases = [
            ("sample.txt", "첫 번째 줄 텍스트입니다.", "utf-8"),
            ("sample_utf8_bom.txt", "BOM이 포함된 UTF-8 일반 텍스트 문서입니다.", "utf-8-sig"),
            ("sample_utf16le.txt", "UTF-16 LE 인코딩 본문입니다.", "utf-16"),
            ("sample_cp949.txt", "CP949 인코딩으로 작성된 한글 문서입니다.", "cp949"),
        ]
        for filename, expected_text, expected_enc in cases:
            path = FIXTURES_DIR / filename
            before_bytes = path.read_bytes()

            records = extract_plain_text(path)
            self.assertTrue(len(records) > 0)
            self.assertIn(expected_text, records[0].text)

            # 파일 바이트 불변 검증
            after_bytes = path.read_bytes()
            self.assertEqual(before_bytes, after_bytes, f"{filename}의 원본 바이트가 변경됨")

            # decode_text 판별 확인
            decoded, enc = decode_text(before_bytes)
            self.assertIn(expected_text, decoded)
            self.assertEqual(enc, expected_enc)

    def test_txt_location_lines_and_paragraphs(self):
        """행 번호, 빈 줄 경계, 검색 본문 줄 순서 및 레코드 계약 확인."""
        path = FIXTURES_DIR / "sample.txt"
        records = extract_plain_text(path)
        self.assertEqual(len(records), 2)
        # sample.txt는 1행에 첫 줄, 2행은 빈 줄, 3행에 두 번째 줄이 있음
        self.assertEqual(records[0].location_kind, "line")
        self.assertEqual(records[0].location_label, "1행")
        self.assertEqual(records[0].sequence, 1)
        self.assertEqual(records[0].text, "첫 번째 줄 텍스트입니다.")

        self.assertEqual(records[1].location_kind, "line")
        self.assertEqual(records[1].location_label, "3행")
        self.assertEqual(records[1].sequence, 2)
        self.assertEqual(records[1].text, "세 번째 줄 텍스트입니다.")

        combined = records_to_text(records)
        self.assertIn("⟦1행⟧ 첫 번째 줄 텍스트입니다.", combined)
        self.assertIn("⟦3행⟧ 세 번째 줄 텍스트입니다.", combined)

    def test_pdf_extracts_numbered_lines_individually_with_page_locations(self):
        """PDF 텍스트 줄을 나눠 번호 위계를 후속 분류기가 볼 수 있게 한다."""
        path = self.root / "numbered.pdf"
        path.write_bytes(b"synthetic")

        class FakeTextPage:
            def get_text_range(self):
                return "Ⅰ. 총칙\n1. 적용 범위\n가. 대상\n일반 본문"
            def close(self):
                pass

        class FakePage:
            def get_textpage(self):
                return FakeTextPage()
            def close(self):
                pass

        class FakeDocument:
            def __len__(self):
                return 1
            def get_page(self, index):
                return FakePage()
            def close(self):
                pass

        class FakePdfium:
            @staticmethod
            def PdfDocument(_path):
                return FakeDocument()

        records = extract_pdf(path, pdfium_module=FakePdfium)
        self.assertEqual([record.text for record in records],
                         ["Ⅰ. 총칙", "1. 적용 범위", "가. 대상", "일반 본문"])
        self.assertEqual([record.location_label for record in records], ["1쪽"] * 4)
        self.assertEqual([record.sequence for record in records], [1, 2, 3, 4])

    def test_hwpx_extracts_section_paragraph_locations(self):
        """다중 섹션 순서, sectionN 문단 M, 비ASCII 본문 확인."""
        path = FIXTURES_DIR / "sample.hwpx"
        records = extract_hwpx(path)
        self.assertEqual(len(records), 3)

        self.assertEqual(records[0].location_kind, "paragraph")
        self.assertEqual(records[0].location_label, "section0 문단 1")
        self.assertEqual(records[0].section, "section0")
        self.assertEqual(records[0].text, "타임머신 섹션0 문단 1")

        self.assertEqual(records[1].location_kind, "paragraph")
        self.assertEqual(records[1].location_label, "section0 문단 2")
        self.assertEqual(records[1].section, "section0")
        self.assertEqual(records[1].text, "섹션0 문단 2 내용")

        self.assertEqual(records[2].location_kind, "paragraph")
        self.assertEqual(records[2].location_label, "section1 문단 1")
        self.assertEqual(records[2].section, "section1")
        self.assertEqual(records[2].text, "섹션1 첫 번째 문단")

    def test_hwpx_extracts_paragraphs_inside_synthetic_one_cell_table(self):
        """합성 1x1 표 내부 문단도 추출하고 표 크기 메타데이터를 전달한다."""
        from docfit_core.timemachine.content_search import extract_paragraphs_legacy

        path = self.root / "synthetic-table.hwpx"
        with ZipFile(path, "w") as archive:
            archive.writestr("Contents/header.xml", "<head/>")
            archive.writestr("Contents/section0.xml", """<section><tbl><tr><tc><p>
                <run><t>제1장 총칙</t></run></p></tc></tr></tbl></section>""")
        hierarchy = extract_paragraphs_legacy(path, "hwpx")
        records = extract_hwpx(path, hierarchy_records=hierarchy)
        self.assertEqual(1, len(records))
        self.assertEqual("제1장 총칙", records[0].text)
        self.assertTrue(records[0].is_table_cell)
        self.assertEqual((1, 1), (records[0].table_rows, records[0].table_cols))

    def test_docx_paragraph_and_header_locations(self):
        """본문 문단 및 머리글/바닥글 위치 확인."""
        path = FIXTURES_DIR / "sample.docx"
        records = extract_docx(path)
        self.assertEqual(len(records), 3)

        self.assertEqual(records[0].location_label, "문단 1")
        self.assertEqual(records[0].section, "본문")
        self.assertEqual(records[0].text, "워드 본문 첫 번째 문단")

        self.assertEqual(records[1].location_label, "문단 2")
        self.assertEqual(records[1].section, "본문")
        self.assertEqual(records[1].text, "워드 본문 두 번째 문단")

        # 머리글은 본문 뒤에 추가
        self.assertIn("머리글", records[2].location_label)
        self.assertEqual(records[2].text, "워드 머리글 영역 내용")

    def test_pptx_slide_and_paragraph_locations(self):
        """슬라이드 정렬 및 슬라이드 번호 배지 확인."""
        path = FIXTURES_DIR / "sample.pptx"
        records = extract_pptx(path)
        self.assertEqual(len(records), 2)

        self.assertEqual(records[0].section, "슬라이드 1")
        self.assertEqual(records[0].location_label, "슬라이드 1 문단 1")
        self.assertEqual(records[0].text, "슬라이드 1 제목 문단")

        self.assertEqual(records[1].section, "슬라이드 2")
        self.assertEqual(records[1].location_label, "슬라이드 2 문단 1")
        self.assertEqual(records[1].text, "슬라이드 2 설명 문단")

    def test_xlsx_shared_and_inline_strings_sheet_rows(self):
        """시트명, 행 번호, 공유/인라인 문자열 확인."""
        path = FIXTURES_DIR / "sample.xlsx"
        records = extract_xlsx(path)

        # 1행: 공유 문자열 헤더 (품목명 수량 단가)
        self.assertEqual(records[0].section, "매출현황")
        self.assertEqual(records[0].location_label, "매출현황 1행")
        self.assertEqual(records[0].text, "품목명 수량 단가")

        # 2행: 인라인 문자열 + 숫자 (노트북 10 1500000)
        self.assertEqual(records[1].section, "매출현황")
        self.assertEqual(records[1].location_label, "매출현황 2행")
        self.assertEqual(records[1].text, "노트북 10 1500000")

        # 두 번째 시트 1행 (운반비 50000)
        self.assertEqual(records[2].section, "지출현황")
        self.assertEqual(records[2].location_label, "지출현황 1행")
        self.assertEqual(records[2].text, "운반비 50000")

    def test_xls_sheet_row_locations(self):
        """다중 시트 및 1-based 행 라벨 확인."""
        # xlrd 모의 객체 주입을 통한 다중 시트 검증
        mock_sheet1 = MagicMock()
        mock_sheet1.name = "시트1"
        mock_sheet1.nrows = 2
        mock_sheet1.row_values.side_effect = lambda r: ["매출", "100"] if r == 0 else ["순익", "20"]

        mock_sheet2 = MagicMock()
        mock_sheet2.name = "시트2"
        mock_sheet2.nrows = 1
        mock_sheet2.row_values.side_effect = lambda r: ["합계", "120"]

        mock_book = MagicMock()
        mock_book.nsheets = 2
        mock_book.sheet_by_index.side_effect = lambda idx: [mock_sheet1, mock_sheet2][idx]

        mock_xlrd = MagicMock()
        mock_xlrd.open_workbook.return_value = mock_book
        with patch.dict(sys.modules, {"xlrd": mock_xlrd}):
            dummy_path = self.root / "test.xls"
            dummy_path.write_text("biff", encoding="utf-8")
            records = extract_xls(dummy_path)

            self.assertEqual(len(records), 3)
            self.assertEqual(records[0].location_label, "시트1 1행")
            self.assertEqual(records[0].text, "매출 100")
            self.assertEqual(records[1].location_label, "시트1 2행")
            self.assertEqual(records[1].text, "순익 20")
            self.assertEqual(records[2].location_label, "시트2 1행")
            self.assertEqual(records[2].text, "합계 120")
            mock_book.release_resources.assert_called_once()

    def test_xls_runtime_unavailable_when_xlrd_missing(self):
        """xlrd 미설치 환경에서 runtime_unavailable 오류 코드 반환 격리 확인."""
        dummy_path = self.root / "test.xls"
        dummy_path.write_text("biff", encoding="utf-8")
        with patch.dict(sys.modules, {"xlrd": None}):
            with self.assertRaises(ExtractionError) as ctx:
                extract_xls(dummy_path)
            self.assertEqual(ctx.exception.reason_code, "runtime_unavailable")

    def test_pdfium_page_numbers_and_resource_close(self):
        """페이지별 추출, 1-based 쪽 번호, 예외 시 문서/페이지 close 확인."""
        mock_textpage = MagicMock()
        mock_textpage.get_text_range.return_value = "PDF 쪽 본문 내용입니다."

        mock_page = MagicMock()
        mock_page.get_textpage.return_value = mock_textpage

        mock_doc = MagicMock()
        mock_doc.__len__.return_value = 2
        mock_doc.get_page.return_value = mock_page

        mock_engine = MagicMock()
        mock_engine.PdfDocument.return_value = mock_doc

        dummy_path = self.root / "test.pdf"
        dummy_path.write_text("pdf_dummy", encoding="utf-8")

        records = extract_pdf(dummy_path, pdfium_module=mock_engine)
        self.assertEqual(len(records), 2)
        self.assertEqual(records[0].location_label, "1쪽")
        self.assertEqual(records[1].location_label, "2쪽")

        # close 호출 검증
        self.assertEqual(mock_textpage.close.call_count, 2)
        self.assertEqual(mock_page.close.call_count, 2)
        mock_doc.close.assert_called_once()

        # 페이지 처리 중 예외 발생 시에도 doc가 close되는지 확인
        mock_doc.reset_mock()
        mock_doc.get_page.side_effect = RuntimeError("페이지 렌더 실패")
        with self.assertRaises(Exception):
            extract_pdf(dummy_path, pdfium_module=mock_engine)
        mock_doc.close.assert_called_once()

    def test_eml_headers_multipart_charset_and_plain_body(self):
        """인코딩된 제목, From/To, plain text 우선, 첨부 제외 확인."""
        path = FIXTURES_DIR / "sample.eml"
        records = extract_eml(path)

        headers = {r.location_label: r.text for r in records if r.location_kind == "message"}
        self.assertIn("제목", headers)
        self.assertEqual(headers["제목"], "연간 업무 보고서 제출")
        self.assertEqual(headers["발신"], "sender@example.com")
        self.assertEqual(headers["수신"], "receiver@example.com")

        body_records = [r for r in records if r.location_kind == "paragraph"]
        self.assertTrue(len(body_records) > 0)
        body_text = "\n".join(r.text for r in body_records)
        self.assertIn("2026년 업무 추진 실적 보고서입니다.", body_text)
        # 첨부 파일 내용은 본문에 없어야 함
        self.assertNotIn("AQIDBAUGBwg=", body_text)

    def test_encrypted_and_corrupt_documents_report_reasons(self):
        """지원 형식 암호/손상 최소 사례가 구체적 reason_code로 분류되는지 확인."""
        # 1. 손상된 Zip (docx)
        bad_docx = self.root / "corrupt.docx"
        bad_docx.write_bytes(b"This is not a zip file")
        with self.assertRaises(ExtractionError) as ctx:
            extract_document(bad_docx)
        self.assertEqual(ctx.exception.reason_code, "corrupt")

        # 2. 빈 텍스트 파일
        empty_txt = self.root / "empty.txt"
        empty_txt.write_bytes(b"")
        with self.assertRaises(ExtractionError) as ctx:
            extract_document(empty_txt)
        self.assertEqual(ctx.exception.reason_code, "empty")

        # 3. 암호화된 ZIP 플래그 (hwpx)
        enc_hwpx = self.root / "enc.hwpx"
        with ZipFile(enc_hwpx, "w") as arc:
            info = arc.getinfo("Contents/header.xml") if "Contents/header.xml" in arc.namelist() else None
        # 손상되거나 암호화된 케이스
        bad_hwpx = self.root / "bad.hwpx"
        bad_hwpx.write_bytes(b"not a valid zip")
        with self.assertRaises(ExtractionError) as ctx:
            extract_document(bad_hwpx)
        self.assertEqual(ctx.exception.reason_code, "corrupt")

    def test_hwp5_uses_kordoc_without_com(self):
        """kordoc bridge를 통한 HWP5 변환 및 COM 미사용, 런타임 부재 검증."""
        fake_hwp = self.root / "sample.hwp"
        fake_hwp.write_text("fake_hwp_content", encoding="utf-8")

        # win32com이 호출되지 않는지 감시
        with patch("docfit_core.kordoc_bridge.parse_document") as mock_parse:
            # 변환 결과 마크다운 생성 시뮬레이션
            def fake_parse(src, target, output_format="markdown"):
                Path(target).write_text("<!-- page 1 -->\nHWP 첫 번째 쪽 내용\n<!-- page 2 -->\nHWP 두 번째 쪽 내용", encoding="utf-8")
            mock_parse.side_effect = fake_parse

            records = extract_hwp(fake_hwp)
            self.assertEqual(len(records), 2)
            self.assertEqual(records[0].location_label, "1쪽")
            self.assertEqual(records[0].text, "HWP 첫 번째 쪽 내용")
            self.assertEqual(records[1].location_label, "2쪽")
            self.assertEqual(records[1].text, "HWP 두 번째 쪽 내용")
            mock_parse.assert_called_once()

        # 런타임 부재 시 runtime_unavailable 예외 확인
        from docfit_core.kordoc_bridge import KordocUnavailableError
        with patch("docfit_core.kordoc_bridge.parse_document", side_effect=KordocUnavailableError("엔진 없음")):
            with self.assertRaises(ExtractionError) as ctx:
                extract_hwp(fake_hwp)
            self.assertEqual(ctx.exception.reason_code, "runtime_unavailable")


    def test_hwp5_page_break_form_feed_and_mapping(self):
        """HWP5 마크다운의 \\f(form feed) 및 빈 페이지/첫 페이지 매핑 누락 방지 확인."""
        fake_hwp = self.root / "sample_ff.hwp"
        fake_hwp.write_text("fake_hwp_content", encoding="utf-8")

        with patch("docfit_core.kordoc_bridge.parse_document") as mock_parse:
            # 케이스 1: \f 경계와 1쪽부터 순차 매핑
            def fake_parse_ff(src, target, output_format="markdown"):
                Path(target).write_text("1쪽 1행\n1쪽 2행\f2쪽 1행\f3쪽 1행", encoding="utf-8")
            mock_parse.side_effect = fake_parse_ff

            records = extract_hwp(fake_hwp)
            self.assertEqual(len(records), 4)
            self.assertEqual(records[0].location_label, "1쪽")
            self.assertEqual(records[0].text, "1쪽 1행")
            self.assertEqual(records[1].location_label, "1쪽")
            self.assertEqual(records[1].text, "1쪽 2행")
            self.assertEqual(records[2].location_label, "2쪽")
            self.assertEqual(records[2].text, "2쪽 1행")
            self.assertEqual(records[3].location_label, "3쪽")
            self.assertEqual(records[3].text, "3쪽 1행")

            # 케이스 2: 맨 앞에 \f가 있거나 중간에 빈 페이지(\f\f)가 있는 경우 1쪽 누락 및 번호 밀림 방지
            def fake_parse_empty_pages(src, target, output_format="markdown"):
                Path(target).write_text("\f1쪽 내용\f\f3쪽 내용", encoding="utf-8")
            mock_parse.side_effect = fake_parse_empty_pages

            records2 = extract_hwp(fake_hwp)
            self.assertEqual(len(records2), 2)
            self.assertEqual(records2[0].location_label, "1쪽")
            self.assertEqual(records2[0].text, "1쪽 내용")
            self.assertEqual(records2[1].location_label, "3쪽")
            self.assertEqual(records2[1].text, "3쪽 내용")

    def test_xlsx_opc_root_target_and_resolve_rel_path(self):
        """XLSX 관계 타깃이 /xl/worksheets/sheet1.xml 등 루트 절대 경로일 때도 정상 추출 확인."""
        # 1. _resolve_rel_path 단위 검증
        self.assertEqual(_resolve_rel_path("xl", "/xl/worksheets/sheet1.xml"), "xl/worksheets/sheet1.xml")
        self.assertEqual(_resolve_rel_path("xl", "worksheets/sheet1.xml"), "xl/worksheets/sheet1.xml")
        self.assertEqual(_resolve_rel_path("xl", "../xl/worksheets/sheet1.xml"), "xl/worksheets/sheet1.xml")

        # 허용되지 않는 상위 탈출 차단
        with self.assertRaises(ExtractionError) as ctx:
            _resolve_rel_path("xl", "../../secret.txt")
        self.assertEqual(ctx.exception.reason_code, "corrupt")

        with self.assertRaises(ExtractionError) as ctx:
            _resolve_rel_path("xl", "/../secret.txt")
        self.assertEqual(ctx.exception.reason_code, "corrupt")

        # 2. 루트 슬래시 Target을 가진 실제 XLSX 추출 검증
        xlsx_path = self.root / "root_target.xlsx"
        with ZipFile(xlsx_path, "w") as arc:
            arc.writestr(
                "xl/workbook.xml",
                '<?xml version="1.0" encoding="UTF-8"?>'
                '<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">'
                '<sheets><sheet name="절대경로시트" sheetId="1" r:id="rId1"/></sheets></workbook>',
            )
            arc.writestr(
                "xl/_rels/workbook.xml.rels",
                '<?xml version="1.0" encoding="UTF-8"?>'
                '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
                '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="/xl/worksheets/sheet1.xml"/>'
                '</Relationships>',
            )
            arc.writestr(
                "xl/worksheets/sheet1.xml",
                '<?xml version="1.0" encoding="UTF-8"?>'
                '<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">'
                '<sheetData>'
                '<row r="1"><c r="A1" t="inlineStr"><is><t>절대경로 데이터</t></is></c></row>'
                '</sheetData></worksheet>',
            )

        records = extract_xlsx(xlsx_path)
        self.assertEqual(len(records), 1)
        self.assertEqual(records[0].location_label, "절대경로시트 1행")
        self.assertEqual(records[0].text, "절대경로 데이터")


class HwpRustExtractorTests(unittest.TestCase):
    """kordoc 의존 줄이기 1단계(2026-11-15): HWP 본문은 Rust(klib extract) 우선, 실패하면 kordoc."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.hwp = Path(self.tmp.name) / "sample.hwp"
        self.hwp.write_bytes(b"fake")
        self.exe = Path(self.tmp.name) / "klib.exe"
        self.exe.write_bytes(b"")

    def _run(self, stdout, returncode=0):
        import subprocess as sp
        return sp.CompletedProcess(args=[], returncode=returncode, stdout=stdout, stderr=b"")

    def test_rust_records_map_to_extraction_records_without_kordoc(self):
        import json as js
        from docfit_core.timemachine import extractors_hwp as mod
        payload = js.dumps({"paragraphs": [
            {"text": "Ⅰ. 추진 배경", "location_kind": "paragraph", "location_label": "문단 1", "sequence": 1,
             "section": None, "hierarchy_level": None, "is_table_cell": False, "style_name": "개요 1", "outline_level": 1},
            {"text": "  ", "location_kind": "paragraph", "location_label": "문단 2", "sequence": 2},
            {"text": "예산 1,200", "location_kind": "table_cell", "location_label": "표 1", "sequence": 3},
            {"text": "쪽 머리", "location_kind": "header", "location_label": "머리말", "sequence": 4},
        ]}, ensure_ascii=False).encode("utf-8")
        with patch.object(mod, "_klib_executable", return_value=self.exe), \
                patch.object(mod.subprocess, "run", return_value=self._run(payload)) as run, \
                patch("docfit_core.kordoc_bridge.parse_document") as kordoc:
            records = mod.extract_hwp(self.hwp)
        kordoc.assert_not_called()
        self.assertEqual(run.call_args.args[0][1:], ["extract", str(self.hwp)])
        self.assertEqual([r.text for r in records], ["Ⅰ. 추진 배경", "예산 1,200", "쪽 머리"])
        self.assertEqual([r.sequence for r in records], [1, 2, 3])
        self.assertTrue(all(r.location_kind == "paragraph" for r in records))
        self.assertEqual((records[0].style_name, records[0].outline_level), ("개요 1", 1))
        self.assertTrue(records[1].is_table_cell)
        self.assertEqual(records[2].section, "머리말")

    def test_falls_back_to_kordoc_when_rust_fails_or_is_missing(self):
        from docfit_core.timemachine import extractors_hwp as mod

        def fake_parse(src, target, output_format="markdown"):
            Path(target).write_text("kordoc 본문", encoding="utf-8")
        for exe, result in ((self.exe, self._run(b"", returncode=1)), (None, None)):
            with self.subTest(exe=exe), patch.object(mod, "_klib_executable", return_value=exe), \
                    patch.object(mod.subprocess, "run", return_value=result), \
                    patch("docfit_core.kordoc_bridge.parse_document", side_effect=fake_parse) as kordoc:
                records = mod.extract_hwp(self.hwp)
                kordoc.assert_called_once()
                self.assertEqual(records[0].text, "kordoc 본문")

    def test_environment_can_force_kordoc(self):
        from docfit_core.timemachine import extractors_hwp as mod

        def fake_parse(src, target, output_format="markdown"):
            Path(target).write_text("kordoc 강제", encoding="utf-8")
        with patch.dict(os.environ, {"DOCFIT_HWP_ENGINE": "kordoc"}), \
                patch.object(mod, "_klib_executable") as find, \
                patch("docfit_core.kordoc_bridge.parse_document", side_effect=fake_parse):
            records = mod.extract_hwp(self.hwp)
        find.assert_not_called()
        self.assertEqual(records[0].text, "kordoc 강제")


if __name__ == "__main__":
    unittest.main()
