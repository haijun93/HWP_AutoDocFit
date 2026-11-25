"""Exercise the real Rust CLI when ANNUAL_EXE points to the built internal tool."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from zipfile import ZipFile

from defusedxml import ElementTree as ET
from docfit_core.annual_xlsx import inspect_annual_xlsx
from docfit_core.hwpx import inspect_hwpx

ROOT = Path(__file__).resolve().parents[1]


@unittest.skipUnless(os.environ.get("ANNUAL_EXE"), "ANNUAL_EXE is required for the Rust bundle integration test")
class AnnualSyntheticTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory()
        cls.addClassCleanup(cls.temp.cleanup)
        cls.out = Path(cls.temp.name)
        subprocess.run([os.environ["ANNUAL_EXE"], "synthetic", str(cls.out), "20261125"], check=True, capture_output=True)

    def test_three_artifacts_and_reproducibility(self):
        second = self.out / "again"
        subprocess.run([os.environ["ANNUAL_EXE"], "synthetic", str(second), "20261125"], check=True, capture_output=True)
        for name in ("인수인계서.hwpx", "실적보고서.hwpx", "통계.xlsx", "원자료/합성_문서.json"):
            self.assertEqual((self.out / name).read_bytes(), (second / name).read_bytes())
        docs = json.loads((self.out / "원자료/합성_문서.json").read_text(encoding="utf-8"))
        self.assertEqual(len(docs), 124)
        self.assertEqual(len({d["path"] for d in docs}), 124)

    def test_xlsx_totals_and_literal_strings(self):
        info = inspect_annual_xlsx(self.out / "통계.xlsx")
        self.assertEqual(info["sheet_count"], 6)
        self.assertFalse(info["has_formulas"])
        sheets = {s["name"]: s["rows"] for s in info["sheets"]}
        self.assertEqual(sum(row[2] for row in sheets["월별통계"][1:]), 124)
        self.assertEqual(sum(row[3] for row in sheets["주차별통계"][1:]), 124)
        self.assertEqual(sum(row[-1] for row in sheets["요일열지도"][1:]), 124)
        self.assertEqual(sum(row[1] for row in sheets["폴더이력"][1:]), 124)
        self.assertEqual(len(sheets["요일열지도"]), 54)  # 2026 has 53 ISO weeks
        for row in sheets["요일열지도"][1:]:
            self.assertEqual(sum(row[2:9]), row[9])

    def test_native_tables_and_both_cli_validators(self):
        for name, count in [("인수인계서.hwpx", 2), ("실적보고서.hwpx", 3)]:
            self.assertEqual(inspect_hwpx(self.out / name).table_count, count)
        for script, args in [("validate_annual_hwpx.py", ["--charts", str(self.out / "인수인계서.hwpx"), str(self.out / "실적보고서.hwpx")]),
                             ("validate_annual_xlsx.py", [str(self.out / "통계.xlsx")])]:
            result = subprocess.run([sys.executable, str(ROOT / "scripts" / script), *args], capture_output=True)
            self.assertEqual(result.returncode, 0, result.stderr.decode("utf-8", errors="replace"))
        ns = {"hp": "http://www.hancom.co.kr/hwpml/2011/paragraph"}
        with ZipFile(self.out / "실적보고서.hwpx") as archive:
            section = ET.fromstring(archive.read("Contents/section0.xml"))
        heatmap = section.findall(".//hp:tbl", ns)[-1]
        self.assertEqual(int(heatmap.get("rowCnt")), 54)

    def test_invalid_seed_fails_without_creating_output(self):
        target = self.out / "invalid"
        result = subprocess.run([os.environ["ANNUAL_EXE"], "synthetic", str(target), "not-a-seed"], capture_output=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(target.exists())
