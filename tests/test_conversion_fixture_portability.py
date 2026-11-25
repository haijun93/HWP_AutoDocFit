"""호스트 OS와 무관한 합성 fixture 바이트 재현성 회귀 시험."""
import json
import tempfile
import unittest
import zipfile
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

from docfit_core import conversion_fixtures as fx


class ConversionFixturePortabilityTests(unittest.TestCase):
    def test_zip_host_platforms_match_committed_manifest_and_office_bytes(self):
        committed_dir = Path(__file__).resolve().parent / "fixtures" / "conversion_fidelity"
        committed = json.loads((committed_dir / "manifest.json").read_text(encoding="utf-8"))
        for platform in ("win32", "linux", "darwin"):
            with self.subTest(platform=platform), tempfile.TemporaryDirectory() as temp:
                # sys 자체를 바꾸지 않고 ZIP의 생성 OS 선택만 재현한다.
                with patch("zipfile.sys", SimpleNamespace(platform=platform)):
                    manifest = fx.generate_fixtures(temp)
                self.assertEqual(manifest, committed)
                for entry in manifest["fixtures"]:
                    if entry["format"] not in ("docx", "xlsx", "pptx"):
                        continue
                    generated = Path(temp) / entry["file"]
                    self.assertEqual(generated.read_bytes(), (committed_dir / entry["file"]).read_bytes())
                    with zipfile.ZipFile(generated) as archive:
                        self.assertTrue(all(info.create_system == 0 for info in archive.infolist()))
