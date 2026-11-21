"""PDF 압축: 원본 보존, 글자·쪽 보존, 보안 PDF 권한 유지, 거부 사유."""

import hashlib
import tempfile
import unittest
import random
from pathlib import Path

from PIL import Image, ImageDraw


def _scan_pdf(path, pages=2):
    """사진처럼 무늬가 있는 큰 그림을 품질 95 JPEG로 담은 A4 300dpi PDF."""
    images = []
    for index in range(pages):
        image = Image.new("RGB", (2480, 3508), "white")
        draw = ImageDraw.Draw(image)
        for y in range(0, 3508, 6):
            draw.line([(0, y), (2480, (y * 7 + index * 131) % 3508)], fill=((y * 3) % 255, (y * 5) % 255, 120), width=3)
        images.append(image)
    images[0].save(path, save_all=True, append_images=images[1:], resolution=300, quality=95)


def _encrypt(source, target, user_password=""):
    from pypdf import PdfWriter
    from pypdf.constants import UserAccessPermissions as UAP

    writer = PdfWriter(clone_from=str(source))
    writer.encrypt(user_password=user_password, owner_password="owner",
                   permissions_flag=UAP.PRINT | UAP.PRINT_TO_REPRESENTATION, algorithm="AES-256")
    with open(target, "wb") as stream:
        writer.write(stream)


class PdfCompressTests(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory()
        self.root = Path(self.folder.name)
        self.scan = self.root / "scan.pdf"
        _scan_pdf(self.scan)

    def tearDown(self):
        self.folder.cleanup()

    def test_scan_pdf_shrinks_keeps_pages_and_source(self):
        from docfit_core.pdf_compress import compress_pdf

        digest = hashlib.sha256(self.scan.read_bytes()).hexdigest()
        result = compress_pdf(self.scan, self.root / "out.pdf")
        self.assertGreater(result.saving, 0.3)
        self.assertFalse(result.secured)
        self.assertEqual(hashlib.sha256(self.scan.read_bytes()).hexdigest(), digest)
        from pypdf import PdfReader
        self.assertEqual(len(PdfReader(str(result.target)).pages), 2)

    def test_secured_pdf_keeps_encryption_and_permissions(self):
        from pypdf import PdfReader
        from docfit_core.pdf_compress import compress_pdf

        secured = self.root / "secured.pdf"
        _encrypt(self.scan, secured)
        result = compress_pdf(secured, self.root / "out.pdf")
        self.assertTrue(result.secured)
        before, after = PdfReader(str(secured)), PdfReader(str(result.target))
        self.assertTrue(after.is_encrypted)
        before.decrypt("")
        after.decrypt("")
        self.assertEqual(int(after.trailer["/Encrypt"]["/P"]), int(before.trailer["/Encrypt"]["/P"]))
        self.assertIn("보안 유지", result.summary())

    def test_secured_pdf_with_negative_signed_permission_value(self):
        """/P가 음수(-3904)로 저장된 원본(Acrobat 등)도 같은 권한이면 압축해 저장한다(2026-11-20 사용자 오류)."""
        import io
        from pypdf import PdfReader, PdfWriter
        from docfit_core.pdf_compress import compress_pdf

        writer = PdfWriter(clone_from=str(self.scan))
        writer.encrypt(user_password="", owner_password="owner", permissions_flag=4294963392, algorithm="RC4-128")
        buffer = io.BytesIO()
        writer.write(buffer)
        raw = buffer.getvalue()
        self.assertIn(b"/P 4294963392", raw)
        negative = self.root / "negative_p.pdf"
        negative.write_bytes(raw.replace(b"/P 4294963392", b"/P -3904"))
        before = PdfReader(str(negative))
        before.decrypt("")
        self.assertEqual(int(before.trailer["/Encrypt"]["/P"]), -3904)

        result = compress_pdf(negative, self.root / "negative_out.pdf")
        self.assertTrue(result.secured)
        after = PdfReader(str(result.target))
        self.assertTrue(after.decrypt(""))
        self.assertEqual(int(after.trailer["/Encrypt"]["/P"]) & 0xFFFFFFFF, -3904 & 0xFFFFFFFF)

    def test_permission_bits_normalizes_signed_and_unsigned_values(self):
        from docfit_core.pdf_compress import _permission_bits

        self.assertEqual(_permission_bits(-3904), 4294963392)
        self.assertEqual(_permission_bits(4294963392), 4294963392)
        self.assertEqual(_permission_bits(-44), 4294967252)

    def test_refusals(self):
        from docfit_core.pdf_compress import NothingToCompress, PdfCompressError, compress_pdf

        opened = self.root / "open.pdf"
        _encrypt(self.scan, opened, user_password="1234")
        with self.assertRaisesRegex(PdfCompressError, "열기 암호"):
            compress_pdf(opened, self.root / "a.pdf")
        fake = self.root / "drm.pdf"
        fake.write_bytes(b"DRM-PROTECTED")
        with self.assertRaisesRegex(PdfCompressError, "올바른 PDF"):
            compress_pdf(fake, self.root / "b.pdf")
        existing = self.root / "exists.pdf"
        existing.write_bytes(b"x")
        with self.assertRaises(PdfCompressError):
            compress_pdf(self.scan, existing)
        self.assertEqual(existing.read_bytes(), b"x")
        small = self.root / "small.pdf"
        once = compress_pdf(self.scan, small)
        with self.assertRaises(NothingToCompress):
            compress_pdf(once.target, self.root / "again.pdf")
        self.assertFalse((self.root / "again.pdf").exists())

    def test_output_name_and_target_menu(self):
        from docfit_core.file_conversion import output_path_for

        self.assertEqual(output_path_for(self.scan, "pdfz").name, "scan(PDF압축).pdf")
        text = self.root / "memo.txt"
        text.write_text("가", encoding="utf-8")
        with self.assertRaises(ValueError):
            output_path_for(text, "pdfz")

    def test_target_size_reached(self):
        from docfit_core.pdf_compress import compress_pdf

        target_path = self.root / "reached.pdf"
        # 1.5MB 목표 크기 설정
        target_bytes = int(1.5 * 1024 * 1024)
        result = compress_pdf(self.scan, target_path, target_bytes=target_bytes)
        self.assertTrue(target_path.exists())
        self.assertTrue(result.reached_target)
        self.assertLessEqual(result.after, target_bytes)
        self.assertEqual(result.reason, "")
        self.assertTrue(result.step.startswith("1단계") or "단계" in result.step)

    def test_target_size_unreached_reports_reason_and_saves_smallest(self):
        from docfit_core.pdf_compress import compress_pdf

        target_path = self.root / "unreached.pdf"
        # 달성 불가능한 작은 목표 크기(50KB) 설정
        target_bytes = 50 * 1024
        result = compress_pdf(self.scan, target_path, target_bytes=target_bytes)
        self.assertTrue(target_path.exists())
        self.assertFalse(result.reached_target)
        self.assertGreater(result.after, target_bytes)
        self.assertIn("목표 50KB에 못 미침", result.reason)
        self.assertIn("못 미침", result.summary())
        self.assertIn("최대 압축", result.step)

    def test_secured_pdf_with_target_size(self):
        from pypdf import PdfReader
        from docfit_core.pdf_compress import compress_pdf

        secured = self.root / "secured_target.pdf"
        _encrypt(self.scan, secured)
        target_path = self.root / "secured_out.pdf"
        result = compress_pdf(secured, target_path, target_bytes=10 * 1024 * 1024)
        self.assertTrue(result.secured)
        self.assertTrue(result.reached_target)
        self.assertTrue(target_path.exists())
        reader = PdfReader(str(target_path))
        self.assertTrue(reader.is_encrypted)
        reader.decrypt("")
        self.assertIsNotNone(reader.trailer["/Encrypt"]["/P"])

    def test_invalid_target_bytes(self):
        from docfit_core.pdf_compress import PdfCompressError, compress_pdf

        with self.assertRaisesRegex(PdfCompressError, "목표 파일 크기는 0보다 커야 합니다"):
            compress_pdf(self.scan, self.root / "invalid.pdf", target_bytes=0)
        with self.assertRaisesRegex(PdfCompressError, "목표 파일 크기는 0보다 커야 합니다"):
            compress_pdf(self.scan, self.root / "invalid.pdf", target_bytes=-1024)

    def _vector_pdf(self, path):
        """그림이 없고 벡터 경로 데이터가 큰 임시 PDF."""
        from pypdf import PdfWriter
        from pypdf.generic import DecodedStreamObject, NameObject

        writer = PdfWriter()
        page = writer.add_blank_page(width=612, height=792)
        rng = random.Random(42)
        # 화면에는 거의 빈 쪽만 보이지만 클립 영역 밖에 복잡한 벡터 경로가 많다.
        # 이 경우 쪽 이미지화는 벡터 데이터를 크게 줄일 수 있다.
        paths = ["0 0 1 1 re W n", "BT /F1 12 Tf 40 750 Td (Vector text sample) Tj ET"]
        for _ in range(2500):
            x, y = rng.randrange(612), rng.randrange(792)
            x2, y2 = rng.randrange(612), rng.randrange(792)
            paths.append(f"{x} {y} m {x2} {y2} l S")
        stream = DecodedStreamObject()
        stream.set_data("\n".join(paths).encode("ascii"))
        page[NameObject("/Contents")] = writer._add_object(stream)
        with path.open("wb") as output:
            writer.write(output)

    def test_auto_switches_vector_pdf_to_raster_and_reports_state(self):
        from pypdf import PdfReader
        from docfit_core.pdf_compress import compress_pdf

        source, target = self.root / "vector.pdf", self.root / "vector_out.pdf"
        self._vector_pdf(source)
        original = source.read_bytes()
        result = compress_pdf(source, target, target_bytes=10 * 1024, method="auto")
        self.assertEqual(source.read_bytes(), original)
        self.assertEqual(result.method, "image_pdf")
        self.assertFalse(result.text_preserved)
        original_pages, converted_pages = PdfReader(str(source)).pages, PdfReader(str(target)).pages
        self.assertEqual(len(original_pages), len(converted_pages))
        self.assertEqual(
            [(round(float(p.mediabox.width), 1), round(float(p.mediabox.height), 1)) for p in original_pages],
            [(round(float(p.mediabox.width), 1), round(float(p.mediabox.height), 1)) for p in converted_pages],
        )
        from docfit_core.pdf_compress import _page_texts
        self.assertTrue(_page_texts(source)[0].strip())
        self.assertFalse(_page_texts(target)[0].strip())
        self.assertIn("글자 선택·검색 불가", result.summary())

    def test_user_selected_image_pdf_and_cancellation(self):
        from pypdf import PdfReader
        from docfit_core.pdf_compress import PdfCompressCancelled, compress_pdf

        source, target = self.root / "vector_choice.pdf", self.root / "chosen.pdf"
        self._vector_pdf(source)
        result = compress_pdf(source, target, target_bytes=10 * 1024, method="image_pdf")
        self.assertEqual(result.method, "image_pdf")
        self.assertFalse(result.text_preserved)
        self.assertEqual(len(PdfReader(str(source)).pages), len(PdfReader(str(target)).pages))
        self.assertTrue(result.reached_target)
        cancelled = self.root / "cancelled.pdf"
        with self.assertRaises(PdfCompressCancelled):
            compress_pdf(source, cancelled, method="image_pdf", cancel_check=lambda: True)
        self.assertFalse(cancelled.exists())

    def test_secure_pdf_rejects_image_pdf_selection(self):
        from docfit_core.pdf_compress import PdfCompressError, compress_pdf

        secured = self.root / "secure_image.pdf"
        _encrypt(self.scan, secured)
        with self.assertRaisesRegex(PdfCompressError, "권한·암호가 유지되지 않아"):
            compress_pdf(secured, self.root / "secure_image_out.pdf", method="image_pdf")


if __name__ == "__main__":
    unittest.main()
