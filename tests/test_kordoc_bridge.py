import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from docfit_core import kordoc_bridge


class KordocBridgeTest(unittest.TestCase):
    def test_generate_uses_requested_government_preset(self):
        with tempfile.TemporaryDirectory() as folder:
            markdown = Path(folder) / "report.md"
            markdown.write_text("# 제목", encoding="utf-8")
            with patch.object(kordoc_bridge, "run_kordoc") as run:
                target = kordoc_bridge.generate_hwpx(markdown, "보고서")
        arguments = run.call_args.args[0]
        self.assertIn("generate", arguments)
        self.assertEqual(arguments[arguments.index("--preset") + 1], "보고서")
        self.assertEqual(target.suffix, ".hwpx")

    def test_generate_passes_image_dir_when_given(self):
        with tempfile.TemporaryDirectory() as folder:
            markdown = Path(folder) / "report.md"
            markdown.write_text("# 제목", encoding="utf-8")
            with patch.object(kordoc_bridge, "run_kordoc") as run:
                kordoc_bridge.generate_hwpx(markdown, "보고서", image_dir=folder)
        arguments = run.call_args.args[0]
        self.assertIn("--image-dir", arguments)
        self.assertEqual(arguments[arguments.index("--image-dir") + 1], str(folder))

    def test_generate_omits_image_dir_when_not_given(self):
        with tempfile.TemporaryDirectory() as folder:
            markdown = Path(folder) / "report.md"
            markdown.write_text("# 제목", encoding="utf-8")
            with patch.object(kordoc_bridge, "run_kordoc") as run:
                kordoc_bridge.generate_hwpx(markdown, "보고서")
        arguments = run.call_args.args[0]
        self.assertNotIn("--image-dir", arguments)

    def test_parse_document_passes_ocr_flag_when_requested(self):
        with tempfile.TemporaryDirectory() as folder:
            source = Path(folder) / "scan.pdf"
            source.write_bytes(b"%PDF-1.4")
            target = Path(folder) / "scan.md"
            with patch.object(kordoc_bridge, "run_kordoc") as run:
                kordoc_bridge.parse_document(source, target, ocr=True)
        arguments = run.call_args.args[0]
        self.assertIn("--ocr", arguments)

    def test_parse_document_omits_ocr_flag_by_default(self):
        with tempfile.TemporaryDirectory() as folder:
            source = Path(folder) / "doc.docx"
            source.write_bytes(b"PK")
            target = Path(folder) / "doc.md"
            with patch.object(kordoc_bridge, "run_kordoc") as run:
                kordoc_bridge.parse_document(source, target)
        arguments = run.call_args.args[0]
        self.assertNotIn("--ocr", arguments)

    def test_pdfium_text_matches_legacy_text_on_generated_digital_pdf(self):
        """A small digital PDF fixture has exact parity with the prior extractor."""
        from docfit_core.pdf_extract import extract_text
        with tempfile.TemporaryDirectory() as folder:
            # Minimal PDF bytes with one text object; no external fixture or PDF authoring package.
            source = Path(folder) / "digital.pdf"
            content = b"BT /F1 12 Tf 72 720 Td (Digital PDF parity 2026) Tj ET"
            objects = [
                b"1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n",
                b"2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj\n",
                b"3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 612 792]/Resources<</Font<</F1 4 0 R>>>>/Contents 5 0 R>>endobj\n",
                b"4 0 obj<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>endobj\n",
                b"5 0 obj<</Length " + str(len(content)).encode() + b">>stream\n" + content + b"\nendstream endobj\n",
            ]
            source.write_bytes(b"%PDF-1.4\n" + b"".join(objects) + b"trailer<</Root 1 0 R>>\n%%EOF")
            legacy_text = "Digital PDF parity 2026"
            with patch("docfit_core.pdf_extract.extract_text", wraps=extract_text):
                target = Path(folder) / "out.txt"
                from docfit_core.pdf_extract import extract_to_file
                self.assertTrue(extract_to_file(source, target, legacy_text))
                self.assertEqual(target.read_text(encoding="utf-8").rstrip(), legacy_text)

    def test_pdf_markdown_and_ocr_keep_kordoc_path(self):
        with tempfile.TemporaryDirectory() as folder:
            source = Path(folder) / "digital.pdf"
            source.write_bytes(b"%PDF-1.4")
            target = Path(folder) / "digital.md"
            with patch("docfit_core.pdf_extract.extract_to_file") as lightweight, \
                    patch.object(kordoc_bridge, "run_kordoc") as kordoc:
                kordoc_bridge.parse_document(source, target, output_format="markdown")
                kordoc_bridge.parse_document(source, target, output_format="text", ocr=True)
            lightweight.assert_not_called()
            self.assertEqual(kordoc.call_count, 2)

    def test_compare_documents_reports_block_changes(self):
        documents = [
            {"blocks": [{"type": "paragraph", "text": "이전 내용"}]},
            {"blocks": [{"type": "paragraph", "text": "새로운 내용"}]},
        ]
        with tempfile.TemporaryDirectory() as folder:
            folder = Path(folder)
            source_a, source_b = folder / "a.hwpx", folder / "b.hwpx"
            source_a.write_bytes(b"a")
            source_b.write_bytes(b"b")
            calls = iter(documents)
            def fake_parse(_source, target, **_kwargs):
                Path(target).write_text(json.dumps(next(calls), ensure_ascii=False), encoding="utf-8")
                return Path(target)
            with patch.object(kordoc_bridge, "parse_document", side_effect=fake_parse):
                report_path = kordoc_bridge.compare_documents(source_a, source_b)
            report = json.loads(report_path.read_text(encoding="utf-8"))
        self.assertEqual(report["blocks_before"], 1)
        self.assertEqual(report["blocks_after"], 1)
        self.assertEqual(report["changes"][0]["operation"], "replace")

    def test_invalid_preset_is_rejected(self):
        with self.assertRaises(ValueError):
            kordoc_bridge.generate_hwpx("input.md", "알 수 없음")

    def test_hwp_render_preview_rust_first(self):
        """kordoc 의존 줄이기 2단계: HWP 미리보기는 Rust 우선."""
        with tempfile.TemporaryDirectory() as folder:
            hwp = Path(folder) / "sample.hwp"
            hwp.write_bytes(b"hwp")
            with patch.object(kordoc_bridge, "_render_preview_rust", return_value=True) as mock_rust, \
                    patch.object(kordoc_bridge, "run_kordoc") as mock_kordoc:
                out_dir = kordoc_bridge.render_preview(hwp)
            mock_rust.assert_called_once()
            mock_kordoc.assert_not_called()
            self.assertEqual(out_dir, hwp.with_name("sample_preview"))

    def test_hwp_render_preview_fallback_to_kordoc(self):
        """Rust 미리보기 실패 시 kordoc render로 대체."""
        with tempfile.TemporaryDirectory() as folder:
            hwp = Path(folder) / "sample.hwp"
            hwp.write_bytes(b"hwp")
            with patch.object(kordoc_bridge, "_render_preview_rust", return_value=False) as mock_rust, \
                    patch.object(kordoc_bridge, "run_kordoc") as mock_kordoc:
                out_dir = kordoc_bridge.render_preview(hwp)
            mock_rust.assert_called_once()
            mock_kordoc.assert_called_once()
            args = mock_kordoc.call_args.args[0]
            self.assertIn("render", args)
            self.assertEqual(out_dir, hwp.with_name("sample_preview"))

    def test_hwp_render_preview_forced_kordoc_env(self):
        """DOCFIT_HWP_ENGINE=kordoc이면 Rust를 거치지 않고 바로 kordoc render."""
        import os
        with tempfile.TemporaryDirectory() as folder:
            hwp = Path(folder) / "sample.hwp"
            hwp.write_bytes(b"hwp")
            with patch.dict(os.environ, {"DOCFIT_HWP_ENGINE": "kordoc"}), \
                    patch.object(kordoc_bridge, "_render_preview_rust") as mock_rust, \
                    patch.object(kordoc_bridge, "run_kordoc") as mock_kordoc:
                kordoc_bridge.render_preview(hwp)
            mock_rust.assert_not_called()
            mock_kordoc.assert_called_once()

    def test_hwp_parse_document_rust_first(self):
        """kordoc 의존 줄이기 2단계: HWP 파싱은 Rust 우선."""
        with tempfile.TemporaryDirectory() as folder:
            hwp = Path(folder) / "doc.hwp"
            hwp.write_bytes(b"hwp")
            target = Path(folder) / "doc.md"
            with patch.object(kordoc_bridge, "_parse_hwp_rust", return_value=True) as mock_rust, \
                    patch.object(kordoc_bridge, "run_kordoc") as mock_kordoc:
                res = kordoc_bridge.parse_document(hwp, target, output_format="text")
            mock_rust.assert_called_once()
            mock_kordoc.assert_not_called()
            self.assertEqual(res, target)

    def test_hwp_markdown_keeps_kordoc_for_table_structure(self):
        """마크다운(파일변환 중간 결과)은 표·제목 구조가 필요해 Rust를 쓰지 않고 kordoc으로 간다."""
        with tempfile.TemporaryDirectory() as folder:
            hwp = Path(folder) / "doc.hwp"
            hwp.write_bytes(b"hwp")
            target = Path(folder) / "doc.md"
            with patch.object(kordoc_bridge, "_parse_hwp_rust", return_value=True) as mock_rust,                     patch.object(kordoc_bridge, "run_kordoc") as mock_kordoc:
                kordoc_bridge.parse_document(hwp, target, output_format="markdown")
            mock_rust.assert_not_called()
            mock_kordoc.assert_called_once()

    def test_hwp_parse_document_fallback_to_kordoc(self):
        """Rust 파싱 실패 시 kordoc parse로 대체."""
        with tempfile.TemporaryDirectory() as folder:
            hwp = Path(folder) / "doc.hwp"
            hwp.write_bytes(b"hwp")
            target = Path(folder) / "doc.md"
            with patch.object(kordoc_bridge, "_parse_hwp_rust", return_value=False) as mock_rust, \
                    patch.object(kordoc_bridge, "run_kordoc") as mock_kordoc:
                res = kordoc_bridge.parse_document(hwp, target, output_format="text")
            mock_rust.assert_called_once()
            mock_kordoc.assert_called_once()
            args = mock_kordoc.call_args.args[0]
            self.assertEqual(args[0], str(hwp))
            self.assertEqual(res, target)

    def test_hwp_parse_document_identity_with_synthetic_hwp(self):
        """합성 HWP 문단 추출 결과 동일성 검증."""
        fake_paragraphs = [
            {"text": "1. 사업 개요", "location_kind": "paragraph"},
            {"text": "추진 기간: 2026년", "location_kind": "table_cell"},
            {"text": "예산: 50백만원", "location_kind": "table_cell"},
        ]
        payload = json.dumps({"paragraphs": fake_paragraphs}).encode("utf-8")
        import subprocess as sp
        mock_proc = sp.CompletedProcess(args=[], returncode=0, stdout=payload, stderr=b"")

        with tempfile.TemporaryDirectory() as folder:
            hwp = Path(folder) / "test.hwp"
            hwp.write_bytes(b"synthetic")
            target = Path(folder) / "test.md"
            with patch.object(kordoc_bridge, "_klib_executable", return_value=Path("/bin/klib")), \
                    patch("subprocess.run", return_value=mock_proc):
                res = kordoc_bridge.parse_document(hwp, target, output_format="text")
            self.assertTrue(res.is_file())
            content = res.read_text(encoding="utf-8")
            self.assertIn("1. 사업 개요", content)
            self.assertIn("추진 기간: 2026년", content)
            self.assertIn("예산: 50백만원", content)


if __name__ == "__main__":
    unittest.main()
