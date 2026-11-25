"""Reject corrupt spreadsheet packages independently of any Office application."""
from pathlib import Path
import tempfile
import unittest
from zipfile import ZipFile

from docfit_core.annual_xlsx import inspect_annual_xlsx


class AnnualXlsxValidationTests(unittest.TestCase):
    def test_missing_package_parts(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "broken.xlsx"
            with ZipFile(path, "w") as archive:
                archive.writestr("xl/workbook.xml", "<workbook/>")
            with self.assertRaisesRegex(ValueError, "Missing required"):
                inspect_annual_xlsx(path)

    def test_unsafe_member(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "unsafe.xlsx"
            with ZipFile(path, "w") as archive:
                archive.writestr("../escape.xml", "<unsafe/>")
            with self.assertRaisesRegex(ValueError, "Unsafe"):
                inspect_annual_xlsx(path)

    def test_formulas_entities_and_broken_relationships(self):
        import os
        import subprocess
        if not os.environ.get("ANNUAL_EXE"):
            self.skipTest("ANNUAL_EXE is required for package mutation tests")
        with tempfile.TemporaryDirectory() as folder:
            folder = Path(folder)
            subprocess.run([os.environ["ANNUAL_EXE"], "synthetic", str(folder)], check=True, capture_output=True)
            with ZipFile(folder / "통계.xlsx") as archive:
                parts = {name: archive.read(name) for name in archive.namelist()}
            mutations = [
                ("xl/worksheets/sheet1.xml", lambda b: b.replace(b"<v>", b"<f>1+1</f><v>", 1), "formulas"),
                ("xl/_rels/workbook.xml.rels", lambda b: b.replace(b"worksheets/sheet1.xml", b"worksheets/missing.xml"), "target missing"),
                ("xl/styles.xml", lambda b: b"<!DOCTYPE x [<!ENTITY a 'bad'>]><x>&a;</x>", None),
            ]
            for index, (name, mutate, message) in enumerate(mutations):
                path = folder / f"bad{index}.xlsx"
                with ZipFile(path, "w") as archive:
                    for member, data in parts.items():
                        archive.writestr(member, mutate(data) if member == name else data)
                if message:
                    with self.assertRaisesRegex(ValueError, message):
                        inspect_annual_xlsx(path)
                else:
                    from defusedxml.common import DefusedXmlException
                    with self.assertRaises(DefusedXmlException):
                        inspect_annual_xlsx(path)
