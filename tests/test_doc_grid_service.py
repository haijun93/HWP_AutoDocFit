"""문서뷰(DocGridService) 시험: 목록·캐시·글자 카드·클라우드 제외·동시성 (DVF2)."""
import io
import os
import stat
import tempfile
import threading
import time
import unittest
import zipfile
from pathlib import Path
from unittest.mock import MagicMock, patch

from PIL import Image

from docfit_app.doc_grid_service import (
    DOC_EXTENSIONS,
    DocGridService,
    extract_first_text,
    find_klib_executable,
    render_text_card,
)
from docfit_core.timemachine import thumbs


def picture_bytes(fmt="PNG", size=(320, 200), color="blue"):
    output = io.BytesIO()
    Image.new("RGB", size, color).save(output, format=fmt)
    return output.getvalue()


def minimal_pdf(text: str = ""):
    objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 600] >>",
    ]
    output = bytearray(b"%PDF-1.4\n")
    offsets = []
    for number, body in enumerate(objects, 1):
        offsets.append(len(output))
        output += b"%d 0 obj\n" % number + body + b"\nendobj\n"
    start = len(output)
    output += b"xref\n0 4\n0000000000 65535 f \n"
    for offset in offsets:
        output += b"%010d 00000 n \n" % offset
    output += b"trailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % start
    return bytes(output)


def make_hwpx(path: Path, text: str = "첫 문단 텍스트", has_image: bool = False):
    members = {
        "Contents/header.xml": b"<head/>",
        "Contents/section0.xml": (
            f'<hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" '
            f'xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph">'
            f'<hp:p><hp:t>{text}</hp:t></hp:p></hs:sec>'
        ).encode("utf-8"),
    }
    if has_image:
        members["Preview/PrvImage.png"] = picture_bytes("PNG")
    with zipfile.ZipFile(path, "w") as z:
        for k, v in members.items():
            z.writestr(k, v)


def make_xlsx(path: Path, text: str = "엑셀 첫 행 텍스트", has_thumb: bool = False):
    members = {
        "[Content_Types].xml": (
            b'<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
            b'<Default Extension="xml" ContentType="application/xml"/>'
            b'<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
            b'<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>'
            b'<Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>'
            b'</Types>'
        ),
        "_rels/.rels": (
            b'<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
            b'<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>'
            b'</Relationships>'
        ),
        "xl/workbook.xml": (
            b'<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">'
            b'<sheets><sheet name="Sheet1" sheetId="1" id="rId1"/></sheets></workbook>'
        ),
        "xl/_rels/workbook.xml.rels": (
            b'<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
            b'<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>'
            b'</Relationships>'
        ),
        "xl/worksheets/sheet1.xml": (
            f'<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">'
            f'<sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>{text}</t></is></c></row></sheetData></worksheet>'
        ).encode("utf-8"),
    }
    if has_thumb:
        members["docProps/thumbnail.jpeg"] = picture_bytes("JPEG")
    with zipfile.ZipFile(path, "w") as z:
        for k, v in members.items():
            z.writestr(k, v)


class DocGridServiceTests(unittest.TestCase):
    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp_dir.cleanup)
        self.root = Path(self.temp_dir.name)
        self.docs_dir = self.root / "docs"
        self.docs_dir.mkdir()
        self.thumbs_dir = self.root / "thumbs"
        self.thumbs_dir.mkdir()

        self.service = DocGridService(thumb_dir=self.thumbs_dir)

    def assertValidThumbnailJpeg(self, data: bytes):
        self.assertIsNotNone(data)
        self.assertGreater(len(data), 0)
        with Image.open(io.BytesIO(data)) as img:
            self.assertEqual(img.format, "JPEG")
            self.assertEqual(max(img.size), thumbs.SIZE)

    # 1. 목록 시험 (List Documents)
    def test_list_documents_filtering_and_fields(self):
        """지원 대상 7개 형식만 포함되고 임시/숨김/비문서 파일은 제외되는지 확인."""
        valid_files = [
            "report.hwp", "doc.hwpx", "data.xls", "sheet.xlsx",
            "slides.ppt", "presentation.pptx", "manual.pdf",
            "UPPER.HWP", "Mixed.Pdf",
        ]
        for name in valid_files:
            (self.docs_dir / name).write_bytes(b"dummy")

        excluded_files = [
            "notes.txt", "archive.zip", "image.png", "music.mp3",
            ".hidden.hwp", "Thumbs.db", "~$temp.docx", "test.tmp",
            "#ieeee_legacy.hwp",
        ]
        for name in excluded_files:
            (self.docs_dir / name).write_bytes(b"dummy")

        # 하위 폴더 생성
        sub = self.docs_dir / "sub"
        sub.mkdir()
        (sub / "nested.hwp").write_bytes(b"dummy")

        # 1-1. non-recursive
        items = self.service.list_documents(self.docs_dir, recursive=False)
        names = [item["name"] for item in items]

        self.assertEqual(len(items), len(valid_files))
        for name in valid_files:
            self.assertIn(name, names)
        for name in excluded_files:
            self.assertNotIn(name, names)
        self.assertNotIn("nested.hwp", names)

        # 항목 필드 구조 검증
        first = items[0]
        self.assertIn("path", first)
        self.assertIn("name", first)
        self.assertIn("stem", first)
        self.assertIn("ext", first)
        self.assertIn("size", first)
        self.assertIn("mtime", first)
        self.assertIn("cloud", first)
        self.assertIn("thumb_key", first)
        self.assertIn("has_thumb", first)
        self.assertFalse(first["cloud"])
        self.assertFalse(first["has_thumb"])

        # 1-2. recursive
        items_rec = self.service.list_documents(self.docs_dir, recursive=True)
        rec_names = [item["name"] for item in items_rec]
        self.assertEqual(len(items_rec), len(valid_files) + 1)
        self.assertIn("nested.hwp", rec_names)

    def test_list_documents_nonexistent_folder(self):
        non_existent = self.root / "does_not_exist"
        with self.assertRaises(FileNotFoundError):
            self.service.list_documents(non_existent)

    # 2. 캐시 시험 (Cache)
    def test_cache_hit_and_invalidation(self):
        """첫 요청 시 생성 및 캐시 저장, 두 번째 요청 시 캐시 반환, 수정 시 재계산 확인."""
        hwpx_path = self.docs_dir / "test.hwpx"
        make_hwpx(hwpx_path, text="첫 줄 내용", has_image=True)

        # 첫 번째 호출: 생성 후 캐시 저장
        res1 = self.service.get_thumbnail(hwpx_path)
        self.assertTrue(res1["ok"])
        self.assertEqual(res1["type"], "image")
        self.assertValidThumbnailJpeg(res1["data"])
        key1 = res1["key"]
        self.assertIsNotNone(key1)

        # 디스크 캐시에 파일 저장 확인
        cached_file = self.thumbs_dir / f"{key1}.jpg"
        self.assertTrue(cached_file.is_file())

        # 목록 조회 시 has_thumb=True 반영 확인
        docs = self.service.list_documents(self.docs_dir)
        self.assertEqual(len(docs), 1)
        self.assertTrue(docs[0]["has_thumb"])

        # 두 번째 호출: 디스크 캐시에서 바로 반환
        res2 = self.service.get_thumbnail(hwpx_path)
        self.assertTrue(res2["ok"])
        self.assertEqual(res2["type"], "cached")
        self.assertEqual(res2["data"], res1["data"])
        self.assertEqual(res2["key"], key1)

        # 파일 수정(mtime 또는 내용 변경) 시 캐시 무효화
        time.sleep(0.05)
        hwpx_path.write_bytes(hwpx_path.read_bytes() + b" ")
        res3 = self.service.get_thumbnail(hwpx_path)
        self.assertTrue(res3["ok"])
        self.assertNotEqual(res3["key"], key1)

    # 3. 글자 카드 시험 (Text Card)
    def test_text_card_hwpx_without_preview_image(self):
        """미리보기 이미지가 없는 HWPX 문서는 첫 문단으로 글자 카드 생성."""
        hwpx_path = self.docs_dir / "sample_no_img.hwpx"
        first_line = "대한민국 공공서식 표준 규격 안내문"
        make_hwpx(hwpx_path, text=first_line, has_image=False)

        res = self.service.get_thumbnail(hwpx_path)
        self.assertTrue(res["ok"])
        self.assertEqual(res["type"], "text_card")
        self.assertValidThumbnailJpeg(res["data"])
        self.assertEqual(res["text"], first_line)

        # 캐시에도 저장되는지 확인
        res_cached = self.service.get_thumbnail(hwpx_path)
        self.assertEqual(res_cached["type"], "cached")
        self.assertEqual(res_cached["data"], res["data"])

    def test_text_card_xlsx_without_thumbnail(self):
        """썸네일 없는 XLSX 문서는 시트 첫 행으로 글자 카드 생성."""
        xlsx_path = self.docs_dir / "budget.xlsx"
        sheet_text = "2026년도 사업예산 집행현황"
        make_xlsx(xlsx_path, text=sheet_text, has_thumb=False)

        res = self.service.get_thumbnail(xlsx_path)
        self.assertTrue(res["ok"])
        self.assertEqual(res["type"], "text_card")
        self.assertValidThumbnailJpeg(res["data"])
        self.assertEqual(res["text"], sheet_text)

    def test_text_card_hwp_with_klib_preview_prvtext(self):
        """klib preview 결과 이미지가 없고 PrvText만 있는 HWP 문서는 글자 카드 생성."""
        hwp_path = self.docs_dir / "memo.hwp"
        hwp_path.write_bytes(b"dummy hwp bytes")

        prv_text_sample = "공문서 기안 요약: 업무 협조 요청의 건"

        with patch.object(
            self.service,
            "_run_klib_preview",
            return_value={"has_image": False, "format": None, "text": prv_text_sample},
        ) as mock_klib:
            res = self.service.get_thumbnail(hwp_path)
            mock_klib.assert_called_once()
            self.assertTrue(res["ok"])
            self.assertEqual(res["type"], "text_card")
            self.assertValidThumbnailJpeg(res["data"])
            self.assertEqual(res["text"], prv_text_sample)

    def test_image_thumbnail_hwp_with_klib_preview_image(self):
        """klib preview 결과 PNG 이미지가 있는 HWP 문서는 160px JPEG 썸네일 생성."""
        hwp_path = self.docs_dir / "cover.hwp"
        hwp_path.write_bytes(b"dummy hwp bytes")

        def fake_klib_preview(source, target_png, timeout=10.0):
            target_png.write_bytes(picture_bytes("PNG"))
            return {"has_image": True, "format": "png", "text": "본문 글"}

        with patch.object(self.service, "_run_klib_preview", side_effect=fake_klib_preview):
            res = self.service.get_thumbnail(hwp_path)
            self.assertTrue(res["ok"])
            self.assertEqual(res["type"], "image")
            self.assertValidThumbnailJpeg(res["data"])

    def test_real_klib_preview_with_fixture_if_available(self):
        """실제 klib 바이너리와 fixture HWP 파일이 있을 때 실제 preview 연동 검증."""
        klib_exe = find_klib_executable()
        fixture_hwp = Path(__file__).resolve().parent / "fixtures" / "KIDX13_reference.hwp"
        if not klib_exe or not fixture_hwp.is_file():
            self.skipTest("klib 바이너리 또는 fixture 파일이 없어 건너뜁니다.")

        target_hwp = self.docs_dir / "real_ref.hwp"
        target_hwp.write_bytes(fixture_hwp.read_bytes())

        res = self.service.get_thumbnail(target_hwp)
        self.assertTrue(res["ok"])
        self.assertEqual(res["type"], "image")
        self.assertValidThumbnailJpeg(res["data"])
        self.assertIsNotNone(res["key"])

        # 캐시 확인
        res_cached = self.service.get_thumbnail(target_hwp)
        self.assertEqual(res_cached["type"], "cached")
        self.assertEqual(res_cached["data"], res["data"])

    def test_as_base64_and_clear_cache(self):
        """as_base64=True 옵션 시 문자열 반환 및 clear_cache 동작 확인."""
        hwpx_path = self.docs_dir / "b64_test.hwpx"
        make_hwpx(hwpx_path, text="base64 테스트", has_image=True)

        res = self.service.get_thumbnail(hwpx_path, as_base64=True)
        self.assertTrue(res["ok"])
        self.assertIsInstance(res["data"], str)

        # base64 디코딩 후 JPEG 검증
        import base64
        raw = base64.b64decode(res["data"])
        self.assertValidThumbnailJpeg(raw)

        # clear_cache 검증
        cleared = self.service.clear_cache()
        self.assertGreaterEqual(cleared, 1)

    def test_render_text_card_empty_text(self):
        """내용이 전혀 없는 빈 문서도 안전하게 기본 글자 카드가 생성되는지 확인."""
        data = render_text_card("", ext="HWP", title="빈문서")
        self.assertValidThumbnailJpeg(data)

    # 4. 클라우드 전용 파일 제외 시험 (Cloud-Only)
    def test_cloud_only_file_never_reads_content(self):
        """클라우드 전용 파일(OneDrive RECALL 속성)은 내용을 전혀 읽지 않고 건너뛰는지 확인."""
        cloud_file = self.docs_dir / "cloud_doc.hwpx"
        # 내용이 손상된 파일이거나 읽으면 오류가 나는 파일
        cloud_file.write_bytes(b"NOT_A_VALID_ZIP")

        # 0x00400000 = FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS
        class FakeStat:
            st_mode = stat.S_IFREG | 0o644
            st_ino = 12345
            st_dev = 1
            st_nlink = 1
            st_uid = 1000
            st_gid = 1000
            st_size = 1024
            st_atime = 1700000000.0
            st_mtime = 1700000000.0
            st_ctime = 1700000000.0
            st_mtime_ns = 1700000000000000000
            st_file_attributes = 0x00400000

        fake_stat = FakeStat()

        with patch("os.lstat", return_value=fake_stat), \
             patch("os.scandir") as mock_scandir:
            # DirEntry mock
            entry_mock = MagicMock()
            entry_mock.name = "cloud_doc.hwpx"
            entry_mock.path = str(cloud_file)
            entry_mock.is_file.return_value = True
            entry_mock.is_dir.return_value = False
            entry_mock.stat.return_value = fake_stat

            mock_scandir.return_value.__enter__.return_value = [entry_mock]

            # 4-1. list_documents 검사
            items = self.service.list_documents(self.docs_dir)
            self.assertEqual(len(items), 1)
            self.assertTrue(items[0]["cloud"])
            self.assertFalse(items[0]["has_thumb"])

            # 4-2. get_thumbnail 검사: 파일 open이나 render를 절대 호출하지 않아야 함
            with patch("builtins.open") as mock_open, \
                 patch.object(thumbs, "render") as mock_render, \
                 patch.object(self.service, "_run_klib_preview") as mock_klib:

                res = self.service.get_thumbnail(cloud_file)
                self.assertTrue(res["ok"])
                self.assertEqual(res["type"], "cloud")
                self.assertIsNone(res["data"])

                mock_open.assert_not_called()
                mock_render.assert_not_called()
                mock_klib.assert_not_called()

    # 5. 동시 생성 제한 시험 (Concurrency Limit)
    def test_concurrency_slots_limit_to_two(self):
        """동시 생성 제한(2개)을 초과할 때 타임아웃 및 retry 반환 확인."""
        # 슬롯 2개 서비스
        service = DocGridService(thumb_dir=self.thumbs_dir, max_slots=2)

        file1 = self.docs_dir / "doc1.hwpx"
        file2 = self.docs_dir / "doc2.hwpx"
        file3 = self.docs_dir / "doc3.hwpx"
        make_hwpx(file1, "1")
        make_hwpx(file2, "2")
        make_hwpx(file3, "3")

        # 2개의 슬롯을 수동으로 점유
        self.assertTrue(service._slots.acquire(timeout=0.1))
        self.assertTrue(service._slots.acquire(timeout=0.1))

        # 세 번째 슬롯 요청은 타임아웃되고 retry=True 반환
        res = service.get_thumbnail(file3, timeout=0.1)
        self.assertFalse(res["ok"])
        self.assertEqual(res["type"], "busy")
        self.assertTrue(res["retry"])

        # 하나의 슬롯을 해제하면 다음 요청 성공
        service._slots.release()
        res_ok = service.get_thumbnail(file3, timeout=1.0)
        self.assertTrue(res_ok["ok"])

        # 나머지 슬롯 해제
        service._slots.release()


if __name__ == "__main__":
    unittest.main()
