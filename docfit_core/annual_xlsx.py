"""Read-only structural inspection of Rust-generated annual XLSX packages."""
from __future__ import annotations

import math
from pathlib import Path, PurePosixPath
import posixpath
import re
from zipfile import ZipFile

from defusedxml import ElementTree as ET

MAIN = "http://schemas.openxmlformats.org/spreadsheetml/2006/main"
RELS = "http://schemas.openxmlformats.org/package/2006/relationships"
OFFICE = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
TYPES = "http://schemas.openxmlformats.org/package/2006/content-types"


def inspect_annual_xlsx(xlsx_path: str | Path) -> dict:
    """Check CRC, XML, content types, relationships, cells and absence of formulas."""
    with ZipFile(xlsx_path) as archive:
        entries = archive.infolist()
        names = {entry.filename for entry in entries}
        if len(names) != len(entries):
            raise ValueError("Duplicate ZIP entries")
        if sum(e.file_size for e in entries) > 64 * 1024 * 1024:
            raise ValueError("XLSX exceeds inspection size limit")
        for name in names:
            if "\\" in name or PurePosixPath(name).is_absolute() or ".." in PurePosixPath(name).parts:
                raise ValueError("Unsafe ZIP member")
        required = {"[Content_Types].xml", "_rels/.rels", "xl/_rels/workbook.xml.rels", "xl/workbook.xml", "xl/styles.xml"}
        if required - names:
            raise ValueError(f"Missing required OpenXML entries: {required - names}")
        if archive.testzip() is not None:
            raise ValueError("ZIP CRC failure")
        roots = {name: ET.fromstring(archive.read(name)) for name in names if name.endswith((".xml", ".rels"))}
        if any(root.find(f".//{{{MAIN}}}f") is not None for root in roots.values()):
            raise ValueError("Calculation formulas are prohibited")
        if any("externalLink" in name or "vbaProject" in name for name in names):
            raise ValueError("External links/macros are prohibited")

        def relationships(name, base):
            root = roots[name]
            if root.tag != f"{{{RELS}}}Relationships":
                raise ValueError("Invalid relationships namespace")
            result = {}
            for item in root:
                ident, target = item.get("Id"), item.get("Target", "")
                if not ident or ident in result or item.get("TargetMode") == "External":
                    raise ValueError("Invalid/external relationship")
                target = posixpath.normpath(posixpath.join(base, target))
                if target not in names:
                    raise ValueError(f"Relationship target missing: {target}")
                result[ident] = (target, item.get("Type"))
            return result

        root_rels = relationships("_rels/.rels", "")
        if ("xl/workbook.xml", OFFICE + "/officeDocument") not in root_rels.values():
            raise ValueError("Missing workbook relationship")
        rels = relationships("xl/_rels/workbook.xml.rels", "xl")
        if ("xl/styles.xml", OFFICE + "/styles") not in rels.values():
            raise ValueError("Missing styles relationship")
        types = roots["[Content_Types].xml"]
        if types.tag != f"{{{TYPES}}}Types":
            raise ValueError("Invalid content types namespace")
        overrides = {node.get("PartName", "").lstrip("/"): node.get("ContentType", "") for node in types if node.tag == f"{{{TYPES}}}Override"}
        for name, suffix in [("xl/workbook.xml", "sheet.main+xml"), ("xl/styles.xml", "styles+xml")]:
            if not overrides.get(name, "").endswith(suffix):
                raise ValueError(f"Invalid content type: {name}")
        styles = roots["xl/styles.xml"].find(f"{{{MAIN}}}cellXfs")
        if styles is None or not len(styles):
            raise ValueError("Missing cell styles")
        workbook = roots["xl/workbook.xml"]
        if workbook.tag != f"{{{MAIN}}}workbook":
            raise ValueError("Invalid workbook namespace")
        sheets_info, sheet_names, sheet_ids, targets = [], set(), set(), set()
        for sheet in workbook.findall(f"{{{MAIN}}}sheets/{{{MAIN}}}sheet"):
            name, ident = sheet.get("name", ""), sheet.get("sheetId")
            if not name or len(name) > 31 or re.search(r"[\\/*?:\[\]]", name) or name.casefold() in sheet_names or ident in sheet_ids:
                raise ValueError("Invalid/duplicate sheet name or ID")
            sheet_names.add(name.casefold())
            sheet_ids.add(ident)
            target, kind = rels.get(sheet.get(f"{{{OFFICE}}}id"), (None, None))
            if kind != OFFICE + "/worksheet" or target in targets:
                raise ValueError("Missing/duplicate worksheet relationship")
            targets.add(target)
            if not overrides.get(target, "").endswith("worksheet+xml"):
                raise ValueError("Missing worksheet content type")
            worksheet = roots[target]
            if worksheet.tag != f"{{{MAIN}}}worksheet":
                raise ValueError("Invalid worksheet namespace")
            rows, last_row = [], 0
            for row in worksheet.findall(f"{{{MAIN}}}sheetData/{{{MAIN}}}row"):
                number = int(row.get("r", 0))
                if number <= last_row:
                    raise ValueError("Invalid row order")
                last_row, cells, last_col = number, [], 0
                for cell in row:
                    match = re.fullmatch(r"([A-Z]+)([1-9][0-9]*)", cell.get("r", ""))
                    if not match or int(match[2]) != number or int(cell.get("s", 0)) >= len(styles):
                        raise ValueError("Invalid cell address/style")
                    col = 0
                    for char in match[1]:
                        col = col * 26 + ord(char) - 64
                    if col <= last_col or col > 16384:
                        raise ValueError("Invalid column order")
                    last_col = col
                    if cell.get("t") == "inlineStr":
                        value = "".join(t.text or "" for t in cell.findall(f".//{{{MAIN}}}t"))
                    else:
                        raw = cell.findtext(f"{{{MAIN}}}v")
                        value = float(raw)
                        if not math.isfinite(value):
                            raise ValueError("Nonfinite numeric cell")
                        if value.is_integer():
                            value = int(value)
                    cells.append(value)
                rows.append(cells)
            if not rows or not rows[0] or any(len(row) != len(rows[0]) for row in rows):
                raise ValueError("Missing headers or inconsistent row width")
            sheets_info.append({"name": name, "row_count": len(rows), "rows": rows})
        if not sheets_info:
            raise ValueError("No worksheets")
    return {"valid": True, "sheet_count": len(sheets_info), "sheet_names": [s["name"] for s in sheets_info], "sheets": sheets_info, "has_formulas": False}
