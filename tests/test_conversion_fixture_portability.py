"""호스트 OS·deflate 구현과 무관한 합성 fixture 내용 재현성 회귀 시험."""
import copy
import tempfile
import unittest
import warnings
import zipfile
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

from docfit_core import conversion_fixtures as fx
from tests.conversion_fixture_checks import (
    COMMITTED, assert_generation_matches_committed, assert_zip_contents_equal,
)


class ConversionFixturePortabilityTests(unittest.TestCase):
    def test_zip_host_platforms_match_committed_manifest_and_office_contents(self):
        for platform in ("win32", "linux", "darwin"):
            with self.subTest(platform=platform), tempfile.TemporaryDirectory() as temp:
                # sys 자체를 바꾸지 않고 ZIP의 생성 OS 선택만 재현한다.
                with patch("zipfile.sys", SimpleNamespace(platform=platform)):
                    manifest = fx.generate_fixtures(temp)
                assert_generation_matches_committed(self, temp, manifest)
                for entry in manifest["fixtures"]:
                    if entry["format"] not in ("docx", "xlsx", "pptx"):
                        continue
                    generated = Path(temp) / entry["file"]
                    with zipfile.ZipFile(generated) as archive:
                        self.assertTrue(all(info.create_system == 0 for info in archive.infolist()))

    def test_different_deflate_output_preserves_every_fixture_part(self):
        original = fx._FixedZip.writestr

        def different_compression(archive, item, data, compress_type=None, compresslevel=None):
            return original(archive, item, data, compress_type, compresslevel=0)

        with tempfile.TemporaryDirectory() as temp:
            with patch.object(fx._FixedZip, "writestr", different_compression):
                manifest = fx.generate_fixtures(temp)
            self.assertNotEqual((Path(temp) / "doc_basic.docx").read_bytes(),
                                (COMMITTED / "doc_basic.docx").read_bytes())
            assert_generation_matches_committed(self, temp, manifest)

    def test_content_comparison_rejects_zip_damage_even_with_valid_crc(self):
        reference = COMMITTED / "doc_basic.docx"
        with zipfile.ZipFile(reference) as archive:
            parts = [(info, archive.read(info)) for info in archive.infolist()]
        for damage in ("xml", "image", "missing", "extra", "duplicate", "order", "host", "comment"):
            with self.subTest(damage=damage), tempfile.TemporaryDirectory() as temp:
                changed = copy.deepcopy(parts)
                comment = b""
                if damage == "xml":
                    i = next(i for i, (info, _) in enumerate(changed) if info.filename == "word/document.xml")
                    info, data = changed[i]
                    changed[i] = info, data.replace("마포구".encode(), "종로구".encode())
                elif damage == "image":
                    i = next(i for i, (info, _) in enumerate(changed) if info.filename.endswith(".png"))
                    changed[i] = changed[i][0], fx.png_bytes(color=(40, 200, 40))
                elif damage == "missing":
                    changed.pop()
                elif damage == "extra":
                    changed.append((zipfile.ZipInfo("extra.xml"), b"<extra/>"))
                elif damage == "duplicate":
                    changed.append(copy.deepcopy(changed[0]))
                elif damage == "order":
                    changed.reverse()
                elif damage == "host":
                    changed[0][0].create_system = 3
                elif damage == "comment":
                    comment = b"changed"
                target = Path(temp) / "changed.docx"
                with warnings.catch_warnings(), zipfile.ZipFile(target, "w") as archive:
                    warnings.simplefilter("ignore", UserWarning)  # 의도적으로 만든 중복 ZIP 항목
                    archive.comment = comment
                    for info, data in changed:
                        archive.writestr(info, data)
                # subTest를 쓰는 실제 unittest 객체 대신 독립 assertion 객체로 실패를 포착한다.
                with self.assertRaises(AssertionError):
                    assert_zip_contents_equal(unittest.TestCase(), target, reference)

    def test_manifest_hash_and_size_cannot_be_ignored(self):
        with tempfile.TemporaryDirectory() as temp:
            manifest = fx.generate_fixtures(temp)
            for field, value in (("sha256", "0" * 64), ("bytes", 0)):
                with self.subTest(field=field):
                    damaged = copy.deepcopy(manifest)
                    entry = next(e for e in damaged["fixtures"] if e["format"] == "docx")
                    entry[field] = value
                    with self.assertRaises(AssertionError):
                        assert_generation_matches_committed(unittest.TestCase(), temp, damaged)
