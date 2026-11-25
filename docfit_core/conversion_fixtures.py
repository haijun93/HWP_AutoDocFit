"""변환 품질 비교용 합성 문서 생성기 (KORDOC2D 카드 B).

사용자 문서는 쓰지 않는다. 블록 명세(제목·문단·목록·표·그림) 하나에서 마크다운/HTML/DOCX/XLSX/PPTX 를
표준 라이브러리(zipfile)만으로 조립하고, 기대 구조(expected)와 SHA-256 을 manifest.json 에 기록한다.
기대 구조의 스키마는 conversion_fidelity.extract_* 의 구조와 같다.
"""
from __future__ import annotations

import hashlib
import html
import json
import struct
import zipfile
import zlib
from pathlib import Path
from typing import Any

MANIFEST_VERSION = 1

# 블록: ("h", level, text) ("p", text) ("ul"|"ol", [(depth, text), ...]) ("table", [[cell,...],...]) ("img", alt)
DOC_SPECS: dict[str, list[tuple]] = {
    "doc_basic": [
        ("h", 1, "마포구 합성 보고서"),
        ("p", "이 문서는 변환 품질 시험용 합성 문서입니다. 숫자 1,234와 English words 포함."),
        ("h", 2, "추진 배경"),
        ("ul", [(0, "주민 편의 개선"), (1, "창구 대기시간 단축"), (1, "누리집 안내 정비"), (0, "예산 절감")]),
        ("h", 2, "추진 일정"),
        ("ol", [(0, "계획 수립"), (0, "시범 운영"), (1, "1차 점검"), (0, "전면 시행")]),
        ("h", 3, "세부 표"),
        ("table", [["구분", "건수", "비고"], ["접수", "120", "정상"], ["처리", "118", "2건 보류"]]),
        ("img", "현황 그림"),
        ("p", "끝."),
    ],
    "doc_tables": [
        ("h", 1, "표 중심 문서"),
        ("table", [["항목", "값"], ["가", "1"]]),
        ("p", "표 사이 문단."),
        ("table", [["부서", "담당", "연락처"], ["기획", "김하나", "02-0000-0001"],
                   ["총무", "이두리", "02-0000-0002"], ["복지", "박세나", "02-0000-0003"]]),
    ],
    "doc_text_only": [
        ("p", "제목도 목록도 표도 없는 본문입니다."),
        ("p", "두 번째 문단에는 한국어와 English, 그리고 12345가 섞여 있습니다."),
    ],
}

SHEET_SPECS: dict[str, list[tuple[str, list[list[str]]]]] = {
    "sheet_basic": [
        ("예산", [["항목", "금액", "비고"], ["인건비", "1200", "확정"], ["운영비", "300", "예정"]]),
        ("실적", [["월", "건수"], ["1월", "10"], ["2월", "12"]]),
    ],
}

SLIDE_SPECS: dict[str, list[dict[str, Any]]] = {
    "slides_basic": [
        {"title": "개요 슬라이드", "bullets": [(0, "첫째 항목"), (0, "둘째 항목"), (1, "하위 항목")]},
        {"title": "표 슬라이드", "table": [["구분", "값"], ["A", "1"], ["B", "2"]], "image": True},
    ],
}


class _FixedZip(zipfile.ZipFile):
    """ZIP 메타데이터를 고정한다. 압축 바이트는 zlib 구현·버전에 따라 다를 수 있다."""

    def writestr(self, item, data, compress_type=None, compresslevel=None):
        if isinstance(item, str):
            info = zipfile.ZipInfo(item, date_time=(2026, 1, 1, 0, 0, 0))
            # 커밋된 ZIP과 동일하게 고정한다. 기본값은 생성 OS에 따라 0/3으로 달라진다.
            info.create_system = 0
            info.external_attr = 0o600 << 16
            info.compress_type = compress_type if compress_type is not None else self.compression
            item = info
        super().writestr(item, data, compress_type=compress_type, compresslevel=compresslevel)


def png_bytes(width: int = 8, height: int = 8, color: tuple[int, int, int] = (200, 40, 40)) -> bytes:
    """외부 라이브러리 없이 만드는 단색 PNG."""
    raw = b"".join(b"\x00" + bytes(color) * width for _ in range(height))

    def chunk(kind: bytes, data: bytes) -> bytes:
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)

    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))


def esc(text: str) -> str:
    return html.escape(text, quote=False)


# ───────────────────── 기대 구조 ─────────────────────
def expected_from_blocks(blocks: list[tuple]) -> dict[str, Any]:
    headings, lists, tables, texts, images = [], [], [], [], 0
    for b in blocks:
        kind = b[0]
        if kind == "h":
            headings.append([b[1], b[2]]); texts.append(b[2])
        elif kind == "p":
            texts.append(b[1])
        elif kind in ("ul", "ol"):
            for depth, text in b[1]:
                lists.append([depth, kind == "ol", text]); texts.append(text)
        elif kind == "table":
            tables.append([list(r) for r in b[1]]); texts.extend(c for r in b[1] for c in r)
        elif kind == "img":
            images += 1
    return {"headings": headings, "lists": lists, "tables": tables, "images": images, "text": "\n".join(texts)}


def expected_from_sheets(sheets) -> dict[str, Any]:
    tables = [[list(r) for r in rows] for _, rows in sheets]
    return {"headings": [], "lists": [], "tables": tables, "images": 0,
            "text": "\n".join(c for t in tables for r in t for c in r)}


def expected_from_slides(slides) -> dict[str, Any]:
    headings, lists, tables, images, texts = [], [], [], 0, []
    for s in slides:
        headings.append([1, s["title"]]); texts.append(s["title"])
        for depth, text in s.get("bullets", []):
            lists.append([depth, False, text]); texts.append(text)
        if "table" in s:
            tables.append([list(r) for r in s["table"]]); texts.extend(c for r in s["table"] for c in r)
        images += 1 if s.get("image") else 0
    return {"headings": headings, "lists": lists, "tables": tables, "images": images, "text": "\n".join(texts)}


# ───────────────────── 마크다운 · HTML ─────────────────────
def render_markdown(blocks: list[tuple]) -> str:
    out: list[str] = []
    for b in blocks:
        kind = b[0]
        if kind == "h":
            out.append("#" * b[1] + " " + b[2])
        elif kind == "p":
            out.append(b[1])
        elif kind in ("ul", "ol"):
            counters: dict[int, int] = {}
            lines = []
            for depth, text in b[1]:
                counters[depth] = counters.get(depth, 0) + 1
                for deeper in [d for d in counters if d > depth]:
                    del counters[deeper]
                marker = f"{counters[depth]}." if kind == "ol" else "-"
                lines.append("    " * depth + f"{marker} {text}")
            out.append("\n".join(lines))
        elif kind == "table":
            rows = b[1]
            lines = ["| " + " | ".join(rows[0]) + " |", "|" + "---|" * len(rows[0])]
            lines += ["| " + " | ".join(r) + " |" for r in rows[1:]]
            out.append("\n".join(lines))
        elif kind == "img":
            out.append(f"![{b[1]}](image1.png)")
    return "\n\n".join(out) + "\n"


def _html_list(tag: str, items: list[tuple[int, str]]) -> str:
    """깊이(0부터)를 실제 중첩 목록으로 만든다."""
    parts: list[str] = []
    depth = -1
    for d, text in items:
        if d > depth:
            while depth < d:
                parts.append(f"<{tag}>")
                depth += 1
                if depth < d:
                    parts.append("<li>")
        else:
            parts.append("</li>")
            while depth > d:
                parts.append(f"</{tag}></li>")
                depth -= 1
        parts.append(f"<li>{esc(text)}")
    parts.append("</li>")
    while depth > 0:
        parts.append(f"</{tag}></li>")
        depth -= 1
    parts.append(f"</{tag}>")
    return "".join(parts)


def render_html(blocks: list[tuple]) -> str:
    body: list[str] = []
    for b in blocks:
        kind = b[0]
        if kind == "h":
            body.append(f"<h{b[1]}>{esc(b[2])}</h{b[1]}>")
        elif kind == "p":
            body.append(f"<p>{esc(b[1])}</p>")
        elif kind in ("ul", "ol"):
            body.append(_html_list(kind, b[1]))
        elif kind == "table":
            rows = "".join("<tr>" + "".join(f"<{'th' if i == 0 else 'td'}>{esc(c)}</{'th' if i == 0 else 'td'}>" for c in r)
                           + "</tr>" for i, r in enumerate(b[1]))
            body.append(f"<table>{rows}</table>")
        elif kind == "img":
            body.append(f'<img src="image1.png" alt="{html.escape(b[1])}">')
    return ('<!doctype html><html lang="ko"><head><meta charset="utf-8"><title>합성 문서</title></head><body>\n'
            + "\n".join(body) + "\n</body></html>\n")


# ───────────────────── DOCX ─────────────────────
W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"'
R = 'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"'
XML = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'


def build_docx(blocks: list[tuple], path: Path) -> None:
    body: list[str] = []
    has_image = False
    for b in blocks:
        kind = b[0]
        if kind == "h":
            body.append(f'<w:p><w:pPr><w:pStyle w:val="Heading{b[1]}"/></w:pPr><w:r><w:t>{esc(b[2])}</w:t></w:r></w:p>')
        elif kind == "p":
            body.append(f'<w:p><w:r><w:t xml:space="preserve">{esc(b[1])}</w:t></w:r></w:p>')
        elif kind in ("ul", "ol"):
            num_id = 1 if kind == "ul" else 2
            for depth, text in b[1]:
                body.append(f'<w:p><w:pPr><w:pStyle w:val="ListParagraph"/><w:numPr><w:ilvl w:val="{depth}"/>'
                            f'<w:numId w:val="{num_id}"/></w:numPr></w:pPr><w:r><w:t>{esc(text)}</w:t></w:r></w:p>')
        elif kind == "table":
            rows = "".join("<w:tr>" + "".join(f'<w:tc><w:tcPr><w:tcW w:w="2400" w:type="dxa"/></w:tcPr>'
                                              f'<w:p><w:r><w:t>{esc(c)}</w:t></w:r></w:p></w:tc>' for c in r) + "</w:tr>"
                           for r in b[1])
            body.append(f'<w:tbl><w:tblPr><w:tblBorders><w:top w:val="single" w:sz="4"/><w:left w:val="single" w:sz="4"/>'
                        f'<w:bottom w:val="single" w:sz="4"/><w:right w:val="single" w:sz="4"/>'
                        f'<w:insideH w:val="single" w:sz="4"/><w:insideV w:val="single" w:sz="4"/></w:tblBorders></w:tblPr>{rows}</w:tbl>')
        elif kind == "img":
            has_image = True
            body.append('<w:p><w:r><w:drawing><wp:inline xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" distT="0" distB="0" distL="0" distR="0">'
                        '<wp:extent cx="720000" cy="720000"/>'
                        f'<wp:docPr id="1" name="Picture 1" descr="{html.escape(b[1])}"/>'
                        '<a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">'
                        '<a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture">'
                        '<pic:pic xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture">'
                        '<pic:nvPicPr><pic:cNvPr id="0" name="image1.png"/><pic:cNvPicPr/></pic:nvPicPr>'
                        '<pic:blipFill><a:blip r:embed="rId10"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>'
                        '<pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="720000" cy="720000"/></a:xfrm>'
                        '<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic>'
                        '</wp:inline></w:drawing></w:r></w:p>')
    document = f'{XML}<w:document {W} {R}><w:body>{"".join(body)}<w:sectPr/></w:body></w:document>'
    styles = (f'{XML}<w:styles {W}>'
              + "".join(f'<w:style w:type="paragraph" w:styleId="Heading{n}"><w:name w:val="heading {n}"/>'
                        f'<w:pPr><w:outlineLvl w:val="{n - 1}"/></w:pPr></w:style>' for n in (1, 2, 3))
              + '<w:style w:type="paragraph" w:styleId="ListParagraph"><w:name w:val="List Paragraph"/></w:style></w:styles>')

    def level(i: int, fmt: str, text: str) -> str:
        return (f'<w:lvl w:ilvl="{i}"><w:start w:val="1"/><w:numFmt w:val="{fmt}"/><w:lvlText w:val="{text}"/>'
                f'<w:pPr><w:ind w:left="{720 * (i + 1)}" w:hanging="360"/></w:pPr></w:lvl>')

    numbering = (f'{XML}<w:numbering {W}>'
                 f'<w:abstractNum w:abstractNumId="0">{"".join(level(i, "bullet", "•") for i in range(3))}</w:abstractNum>'
                 f'<w:abstractNum w:abstractNumId="1">{"".join(level(i, "decimal", f"%{i + 1}.") for i in range(3))}</w:abstractNum>'
                 '<w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num><w:num w:numId="2"><w:abstractNumId w:val="1"/></w:num>'
                 '</w:numbering>')
    rels = (f'{XML}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
            '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>'
            '<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/>'
            + ('<Relationship Id="rId10" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/>' if has_image else "")
            + '</Relationships>')
    ctypes = (f'{XML}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
              '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
              '<Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/>'
              '<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
              '<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>'
              '<Override PartName="/word/numbering.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/>'
              '</Types>')
    root_rels = (f'{XML}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
                 '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>')
    with _FixedZip(path, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("[Content_Types].xml", ctypes)
        z.writestr("_rels/.rels", root_rels)
        z.writestr("word/document.xml", document)
        z.writestr("word/styles.xml", styles)
        z.writestr("word/numbering.xml", numbering)
        z.writestr("word/_rels/document.xml.rels", rels)
        if has_image:
            z.writestr("word/media/image1.png", png_bytes())


# ───────────────────── XLSX ─────────────────────
def _col(i: int) -> str:
    return chr(ord("A") + i)


def build_xlsx(sheets: list[tuple[str, list[list[str]]]], path: Path) -> None:
    ns = "http://schemas.openxmlformats.org/spreadsheetml/2006/main"
    parts = {}
    for n, (_, rows) in enumerate(sheets, 1):
        data = "".join(
            f'<row r="{r}">' + "".join(
                f'<c r="{_col(c)}{r}" t="inlineStr"><is><t>{esc(v)}</t></is></c>' for c, v in enumerate(row, 0)) + "</row>"
            for r, row in enumerate(rows, 1))
        parts[f"xl/worksheets/sheet{n}.xml"] = f'{XML}<worksheet xmlns="{ns}"><sheetData>{data}</sheetData></worksheet>'
    workbook = (f'{XML}<workbook xmlns="{ns}" {R}><sheets>'
                + "".join(f'<sheet name="{html.escape(name)}" sheetId="{n}" r:id="rId{n}"/>' for n, (name, _) in enumerate(sheets, 1))
                + "</sheets></workbook>")
    wb_rels = (f'{XML}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
               + "".join(f'<Relationship Id="rId{n}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet{n}.xml"/>'
                         for n in range(1, len(sheets) + 1)) + "</Relationships>")
    ctypes = (f'{XML}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
              '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
              '<Default Extension="xml" ContentType="application/xml"/>'
              '<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>'
              + "".join(f'<Override PartName="/xl/worksheets/sheet{n}.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>'
                        for n in range(1, len(sheets) + 1)) + "</Types>")
    root_rels = (f'{XML}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
                 '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>')
    with _FixedZip(path, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("[Content_Types].xml", ctypes)
        z.writestr("_rels/.rels", root_rels)
        z.writestr("xl/workbook.xml", workbook)
        z.writestr("xl/_rels/workbook.xml.rels", wb_rels)
        for name, content in parts.items():
            z.writestr(name, content)


# ───────────────────── PPTX ─────────────────────
A = 'xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"'
P = 'xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"'
REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
THEME = (f'{XML}<a:theme {A} name="Synthetic"><a:themeElements>'
         '<a:clrScheme name="c">' + "".join(f'<a:{n}><a:srgbClr val="{v}"/></a:{n}>' for n, v in (
             ("dk1", "000000"), ("lt1", "FFFFFF"), ("dk2", "222222"), ("lt2", "EEEEEE"), ("accent1", "4472C4"),
             ("accent2", "ED7D31"), ("accent3", "A5A5A5"), ("accent4", "FFC000"), ("accent5", "5B9BD5"),
             ("accent6", "70AD47"), ("hlink", "0563C1"), ("folHlink", "954F72"))) + "</a:clrScheme>"
         '<a:fontScheme name="f"><a:majorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont>'
         '<a:minorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont></a:fontScheme>'
         '<a:fmtScheme name="s"><a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:fillStyleLst>'
         '<a:lnStyleLst><a:ln w="6350"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="12700"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="19050"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln></a:lnStyleLst>'
         '<a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst>'
         '<a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:bgFillStyleLst></a:fmtScheme>'
         '</a:themeElements></a:theme>')


def _sp(shape_id: int, name: str, ph: str, paras: str, y: int) -> str:
    return (f'<p:sp><p:nvSpPr><p:cNvPr id="{shape_id}" name="{name}"/><p:cNvSpPr/><p:nvPr><p:ph type="{ph}"/></p:nvPr></p:nvSpPr>'
            f'<p:spPr><a:xfrm><a:off x="457200" y="{y}"/><a:ext cx="8229600" cy="900000"/></a:xfrm></p:spPr>'
            f'<p:txBody><a:bodyPr/><a:lstStyle/>{paras}</p:txBody></p:sp>')


def build_pptx(slides: list[dict[str, Any]], path: Path) -> None:
    files: dict[str, str | bytes] = {}
    slide_ids, pres_rels = [], []
    for n, s in enumerate(slides, 1):
        shapes = [_sp(2, "Title", "title", f'<a:p><a:r><a:t>{esc(s["title"])}</a:t></a:r></a:p>', 300000)]
        if s.get("bullets"):
            paras = "".join(f'<a:p><a:pPr lvl="{d}"/><a:r><a:t>{esc(t)}</a:t></a:r></a:p>' for d, t in s["bullets"])
            shapes.append(_sp(3, "Body", "body", paras, 1400000))
        slide_rels = ['<Relationship Id="rId1" Type="%s/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>' % REL]
        if "table" in s:
            rows = "".join("<a:tr h=\"370840\">" + "".join(f'<a:tc><a:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:t>{esc(c)}</a:t></a:r></a:p></a:txBody><a:tcPr/></a:tc>' for c in r) + "</a:tr>"
                           for r in s["table"])
            grid = "".join('<a:gridCol w="2000000"/>' for _ in s["table"][0])
            shapes.append('<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="4" name="Table"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr>'
                          '<p:xfrm><a:off x="457200" y="2600000"/><a:ext cx="4000000" cy="1100000"/></p:xfrm>'
                          '<a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/table">'
                          f'<a:tbl><a:tblPr/><a:tblGrid>{grid}</a:tblGrid>{rows}</a:tbl></a:graphicData></a:graphic></p:graphicFrame>')
        if s.get("image"):
            files["ppt/media/image1.png"] = png_bytes()
            slide_rels.append('<Relationship Id="rId2" Type="%s/image" Target="../media/image1.png"/>' % REL)
            shapes.append('<p:pic><p:nvPicPr><p:cNvPr id="5" name="Picture"/><p:cNvPicPr/><p:nvPr/></p:nvPicPr>'
                          '<p:blipFill><a:blip r:embed="rId2"/><a:stretch><a:fillRect/></a:stretch></p:blipFill>'
                          '<p:spPr><a:xfrm><a:off x="5000000" y="2600000"/><a:ext cx="720000" cy="720000"/></a:xfrm>'
                          '<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr></p:pic>')
        files[f"ppt/slides/slide{n}.xml"] = (f'{XML}<p:sld {P} {A} {R}><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>'
                                              f'<p:grpSpPr/>{"".join(shapes)}</p:spTree></p:cSld></p:sld>')
        files[f"ppt/slides/_rels/slide{n}.xml.rels"] = (f'{XML}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{"".join(slide_rels)}</Relationships>')
        slide_ids.append(f'<p:sldId id="{255 + n}" r:id="rId{n + 1}"/>')
        pres_rels.append(f'<Relationship Id="rId{n + 1}" Type="{REL}/slide" Target="slides/slide{n}.xml"/>')
    empty_tree = '<p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/></p:spTree></p:cSld>'
    files["ppt/slideMasters/slideMaster1.xml"] = (f'{XML}<p:sldMaster {P} {A} {R}>{empty_tree}<p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/>'
                                                  '<p:sldLayoutIdLst><p:sldLayoutId id="2147483649" r:id="rId1"/></p:sldLayoutIdLst></p:sldMaster>')
    files["ppt/slideMasters/_rels/slideMaster1.xml.rels"] = (f'{XML}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
                                                              f'<Relationship Id="rId1" Type="{REL}/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>'
                                                              f'<Relationship Id="rId2" Type="{REL}/theme" Target="../theme/theme1.xml"/></Relationships>')
    files["ppt/slideLayouts/slideLayout1.xml"] = f'{XML}<p:sldLayout {P} {A} {R} type="obj">{empty_tree}</p:sldLayout>'
    files["ppt/slideLayouts/_rels/slideLayout1.xml.rels"] = (f'{XML}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
                                                              f'<Relationship Id="rId1" Type="{REL}/slideMaster" Target="../slideMasters/slideMaster1.xml"/></Relationships>')
    files["ppt/theme/theme1.xml"] = THEME
    files["ppt/presentation.xml"] = (f'{XML}<p:presentation {P} {A} {R}><p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst>'
                                     f'<p:sldIdLst>{"".join(slide_ids)}</p:sldIdLst><p:sldSz cx="9144000" cy="6858000"/><p:notesSz cx="6858000" cy="9144000"/></p:presentation>')
    files["ppt/_rels/presentation.xml.rels"] = (f'{XML}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
                                                 f'<Relationship Id="rId1" Type="{REL}/slideMaster" Target="slideMasters/slideMaster1.xml"/>{"".join(pres_rels)}</Relationships>')
    pml = "application/vnd.openxmlformats-officedocument.presentationml"
    files["[Content_Types].xml"] = (f'{XML}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
                                     '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
                                     '<Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/>'
                                     f'<Override PartName="/ppt/presentation.xml" ContentType="{pml}.presentation.main+xml"/>'
                                     f'<Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="{pml}.slideMaster+xml"/>'
                                     f'<Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="{pml}.slideLayout+xml"/>'
                                     '<Override PartName="/ppt/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/>'
                                     + "".join(f'<Override PartName="/ppt/slides/slide{n}.xml" ContentType="{pml}.slide+xml"/>' for n in range(1, len(slides) + 1))
                                     + "</Types>")
    files["_rels/.rels"] = (f'{XML}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
                            f'<Relationship Id="rId1" Type="{REL}/officeDocument" Target="ppt/presentation.xml"/></Relationships>')
    with _FixedZip(path, "w", zipfile.ZIP_DEFLATED) as z:
        for name, content in files.items():
            z.writestr(name, content)


# ───────────────────── 기대 구조 → HWPX (시험용 후보/기준 출력 대역) ─────────────────────
HP = "http://www.hancom.co.kr/hwpml/2011/paragraph"
HH = "http://www.hancom.co.kr/hwpml/2011/head"


def build_hwpx_from_blocks(blocks: list[tuple], path: Path, *, list_style: str = "paraPr") -> None:
    """블록 명세(문서 순서 유지)로 최소 HWPX 를 만든다. list_style: 'paraPr'(문단 속성 BULLET/NUMBER) | 'marker'(본문 앞 기호).

    실제 kordoc 출력이 아니라 비교기·시험용 대역이다. 한/글에서 열어 본 적 없다.
    """
    styles = "".join(f'<hh:style id="{n}" name="개요 {n}" engName="Outline {n}"/>' for n in (1, 2, 3, 4))
    para_prs = "".join(f'<hh:paraPr id="{20 + d}"><hh:heading type="BULLET" level="{d}"/></hh:paraPr>'
                       f'<hh:paraPr id="{30 + d}"><hh:heading type="NUMBER" level="{d}"/></hh:paraPr>' for d in range(4))
    header = (f'{XML}<hh:head xmlns:hh="{HH}"><hh:refList><hh:styles>{styles}</hh:styles>'
              f'<hh:paraProperties>{para_prs}</hh:paraProperties></hh:refList></hh:head>')

    def p(text: str, style: str = "0", para_pr: str = "0") -> str:
        return f'<hp:p styleIDRef="{style}" paraPrIDRef="{para_pr}"><hp:run><hp:t>{esc(text)}</hp:t></hp:run></hp:p>'

    paras: list[str] = []
    images = 0
    for b in blocks:
        kind = b[0]
        if kind == "h":
            paras.append(p(b[2], style=str(b[1])))
        elif kind == "p":
            paras.append(p(b[1]))
        elif kind in ("ul", "ol"):
            for depth, text in b[1]:
                if list_style == "marker":
                    paras.append(p("  " * depth + ("1. " if kind == "ol" else "- ") + text))
                else:
                    paras.append(p(text, para_pr=str((30 if kind == "ol" else 20) + depth)))
        elif kind == "table":
            rows = "".join("<hp:tr>" + "".join(f'<hp:tc><hp:subList>{p(c)}</hp:subList></hp:tc>' for c in row) + "</hp:tr>" for row in b[1])
            paras.append(f'<hp:p><hp:run><hp:tbl rowCnt="{len(b[1])}">{rows}</hp:tbl></hp:run></hp:p>')
        elif kind == "img":
            images += 1
            paras.append('<hp:p><hp:run><hp:pic/></hp:run></hp:p>')
    section = f'{XML}<hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" xmlns:hp="{HP}">{"".join(paras)}</hs:sec>'
    with _FixedZip(path, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("mimetype", "application/hwp+zip", compress_type=zipfile.ZIP_STORED)
        z.writestr("Contents/header.xml", header)
        z.writestr("Contents/section0.xml", section)
        for n in range(images):
            z.writestr(f"BinData/image{n + 1}.png", png_bytes())


# ───────────────────── 생성 + manifest ─────────────────────
def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def generate_fixtures(out_dir: str | Path) -> dict[str, Any]:
    """합성 문서와 해당 실물의 해시·크기·기대 구조를 담은 manifest.json을 쓴다.

    같은 압축 환경에서는 바이트도 재현된다. 환경 간 비교는 압축을 푼 모든 항목의
    원시 바이트와 ZIP 메타데이터로 한다(압축 파일 해시·크기는 환경별일 수 있다).
    """
    out = Path(out_dir)
    out.mkdir(parents=True, exist_ok=True)
    entries: list[dict[str, Any]] = []

    def add(fid: str, fmt: str, file: Path, expected: dict[str, Any]) -> None:
        entries.append({"id": fid, "format": fmt, "file": file.name, "sha256": sha256(file),
                        "bytes": file.stat().st_size, "expected": expected})

    for name, blocks in DOC_SPECS.items():
        expected = expected_from_blocks(blocks)
        has_img = any(b[0] == "img" for b in blocks)
        if has_img:
            (out / "image1.png").write_bytes(png_bytes())
        md = out / f"{name}.md"
        md.write_text(render_markdown(blocks), encoding="utf-8", newline="\n")
        add(f"{name}__md", "md", md, expected)
        ht = out / f"{name}.html"
        ht.write_text(render_html(blocks), encoding="utf-8", newline="\n")
        add(f"{name}__html", "html", ht, expected)
        dx = out / f"{name}.docx"
        build_docx(blocks, dx)
        add(f"{name}__docx", "docx", dx, expected)
    for name, sheets in SHEET_SPECS.items():
        xl = out / f"{name}.xlsx"
        build_xlsx(sheets, xl)
        add(f"{name}__xlsx", "xlsx", xl, expected_from_sheets(sheets))
    for name, slides in SLIDE_SPECS.items():
        pp = out / f"{name}.pptx"
        build_pptx(slides, pp)
        add(f"{name}__pptx", "pptx", pp, expected_from_slides(slides))

    manifest = {"version": MANIFEST_VERSION, "generator": "docfit_core.conversion_fixtures",
                "note": "합성 문서 — 사용자 문서 아님. expected 는 conversion_fidelity 구조 스키마.",
                "fixtures": entries}
    (out / "manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=1) + "\n", encoding="utf-8", newline="\n")
    return manifest
