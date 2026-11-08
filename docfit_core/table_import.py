"""HWPX 표(hp:tbl)를 표그림판 그림(grid·lines·fills·texts)으로 바꾼다. COM·클립보드를 쓰지 않는다.

입력 파일은 신뢰하지 않는다: open_hwpx_safely(압축 폭탄·경로 검사), defusedxml, 표 칸·격자 상한.
칸 폭·높이(HWPUNIT, 1mm≈283.465)는 표그림판 격자(cell_mm)로 반올림한다. 원본 문서는 읽기만 한다.
표현하지 못한 속성(대각선, 칸 그림 배경, 그라데이션, 칸 안 표)은 warnings로 알린다.
"""
from __future__ import annotations

from html.parser import HTMLParser
import math
import re

from defusedxml import ElementTree as ET

from .hwpx import open_hwpx_safely

PT_TO_MM = 25.4 / 72
UNIT_MM = 283.465
MAX_GRID = 200
MAX_CELLS = 2000
MAX_XML_BYTES = 30 * 1024 * 1024
_MARGIN = 1
_HP = "{http://www.hancom.co.kr/hwpml/2011/paragraph}"
_HH = "{http://www.hancom.co.kr/hwpml/2011/head}"
_HC = "{http://www.hancom.co.kr/hwpml/2011/core}"
_RANK = {"thin": 0, "thick": 1, "double": 2}


def _local(tag):
    return tag.rsplit("}", 1)[-1]


def _int(value, default=0):
    try:
        return int(value)
    except (TypeError, ValueError):
        return default


def _hex(value):
    if isinstance(value, str) and re.fullmatch(r"#[0-9A-Fa-f]{6}", value):
        return value.upper()
    return None


def _border(node):
    """hh:*Border → (굵기, 모양, 색) 또는 None(선 없음)."""
    if node is None:
        return None
    kind = (node.get("type") or "NONE").upper()
    if kind == "NONE":
        return None
    match = re.match(r"\s*([\d.]+)", node.get("width") or "")
    mm = float(match.group(1)) if match else 0.12
    color = _hex(node.get("color")) or "#000000"
    if "DOUBLE" in kind or kind == "THICK_THIN" or kind == "THIN_THICK":
        return ("double", "solid", color)
    shape = "dash" if "DASH" in kind else "dot" if "DOT" in kind else "solid"
    return ("thick" if mm >= 0.4 else "thin", shape, color)


def _read_header(root):
    borders, chars, paras = {}, {}, {}
    if root is None:
        return borders, chars, paras
    for node in root.iter():
        name = _local(node.tag)
        ident = node.get("id")
        if ident is None:
            continue
        if name == "borderFill":
            side = {}
            for child in node:
                cname = _local(child.tag)
                if cname in ("leftBorder", "rightBorder", "topBorder", "bottomBorder"):
                    side[cname[:-6]] = _border(child)
                elif cname == "diagonal" and _border(child) is not None:
                    side["_diagonal"] = True
            fill, extra = None, None
            for brush in node.iter():
                bname = _local(brush.tag)
                if bname == "winBrush":
                    face = _hex(brush.get("faceColor"))
                    fill = None if face in (None, "#FFFFFF") else face
                elif bname == "imgBrush":
                    extra = "image"
                elif bname == "gradation":
                    extra = "gradient"
            for slash in ("slash", "backSlash"):
                for child in node:
                    if _local(child.tag) == slash and (child.get("type") or "NONE").upper() != "NONE":
                        side["_diagonal"] = True
            borders[ident] = dict(sides=side, fill=fill, extra=extra)
        elif name == "charPr":
            chars[ident] = any(_local(c.tag) == "bold" for c in node)
        elif name == "paraPr":
            align = next((c for c in node if _local(c.tag) == "align"), None)
            value = (align.get("horizontal") if align is not None else "") or ""
            paras[ident] = {"LEFT": "left", "RIGHT": "right", "JUSTIFY": "left", "DISTRIBUTE": "left"}.get(value.upper(), "center")
    return borders, chars, paras


def _own_paragraphs(sublist, chars, paras):
    """칸 문단 → (글, 굵게, 정렬). 칸 안 표의 글은 제외한다."""
    lines, bold, align = [], False, None

    def text_of(node):
        parts = []

        def walk(n, top=False):
            name = _local(n.tag)
            if not top and name in ("tbl", "p"):
                return
            if name == "t" and n.text:
                parts.append(n.text)
            elif name in ("lineBreak", "br"):
                parts.append("\n")
            elif name == "tab":
                parts.append("\t")
            elif name in ("fwSpace", "nbSpace"):
                parts.append(" ")
            for child in n:
                walk(child)
                if name == "t" and child.tail:
                    parts.append(child.tail)
        walk(node, True)
        return "".join(parts)

    for para in sublist.findall(_HP + "p"):
        text = text_of(para)
        for run in para.findall(_HP + "run"):
            run_text = "".join(t.text or "" for t in run.findall(_HP + "t"))
            if run_text.strip():
                bold = bold or chars.get(run.get("charPrIDRef"), False)
        lines.append(text)
        if align is None and text.strip():
            align = paras.get(para.get("paraPrIDRef"))
    return "\n".join(lines).strip("\n"), bold, align or "center"


def _top_tables(sections):
    found = []

    def walk(element, inside=False):
        for child in element:
            is_table = child.tag == _HP + "tbl"
            if is_table and not inside:
                found.append(child)
            walk(child, inside or is_table)
    for root in sections:
        walk(root)
    return found


def _sizes(items, count, axis, cell_mm, warnings):
    """열 폭·행 높이를 격자 칸 수로. 병합하지 않은 칸 값을 먼저 쓰고 모자라면 병합 칸 값을 나눈다."""
    raw = [None] * count
    for cell in items:
        index, span, size = (cell["col"], cell["colspan"], cell["w"]) if axis == "col" else (cell["row"], cell["rowspan"], cell["h"])
        if span == 1 and size > 0 and index < count:
            raw[index] = max(raw[index] or 0, size)
    for cell in items:
        index, span, size = (cell["col"], cell["colspan"], cell["w"]) if axis == "col" else (cell["row"], cell["rowspan"], cell["h"])
        rng = range(index, min(count, index + span))
        missing = [i for i in rng if raw[i] is None]
        if size > 0 and missing:
            share = max(0.0, size - sum(raw[i] or 0 for i in rng)) / len(missing)
            for i in missing:
                raw[i] = share or None
    fallback = 30 * UNIT_MM
    out = []
    for value in raw:
        mm = (value or fallback) / UNIT_MM
        # 행은 올림(글이 선에 겹치지 않게), 열은 반올림.
        out.append(max(1, math.ceil(mm / cell_mm - 0.2)) if axis == "row" else max(1, round(mm / cell_mm)))
    return out


def _check_cell_mm(cell_mm):
    if isinstance(cell_mm, bool) or not isinstance(cell_mm, (int, float)) or not math.isfinite(cell_mm) or not 0.5 <= cell_mm <= 50:
        raise ValueError("칸 간격은 0.5~50mm로 지정해 주세요.")


def _read_document(path):
    """HWPX → (최상위 표 목록, header 뿌리). 표가 없으면 빈 목록."""
    sections, header = [], None
    with open_hwpx_safely(path) as archive:
        names = sorted((n for n in archive.namelist() if re.fullmatch(r"Contents/section\d+\.xml", n)),
                       key=lambda n: int(re.search(r"(\d+)", n.rsplit("/", 1)[-1]).group(1)))
        if not names:
            raise ValueError("HWPX에 본문 구역이 없습니다.")
        total = 0
        for name in names + (["Contents/header.xml"] if "Contents/header.xml" in archive.namelist() else []):
            info = archive.getinfo(name)
            total += info.file_size
            if info.file_size > MAX_XML_BYTES or total > 2 * MAX_XML_BYTES:
                raise ValueError("문서 XML이 너무 큽니다.")
            try:
                root = ET.fromstring(archive.read(name))
            except ET.ParseError as exc:
                raise ValueError("HWPX XML을 읽을 수 없습니다.") from exc
            if name == "Contents/header.xml":
                header = root
            else:
                sections.append(root)
    return _top_tables(sections), header


def _read_cells(table, borders, chars, paras, warnings):
    cells = []
    for tr in table.findall(_HP + "tr"):
        for tc in tr.findall(_HP + "tc"):
            if len(cells) >= MAX_CELLS:
                raise ValueError("표 칸이 너무 많습니다(2000칸 넘음).")
            addr, span, size = tc.find(_HP + "cellAddr"), tc.find(_HP + "cellSpan"), tc.find(_HP + "cellSz")
            if addr is None:
                continue
            sub = tc.find(_HP + "subList")
            text, bold, align = _own_paragraphs(sub, chars, paras) if sub is not None else ("", False, "center")
            if sub is not None and any(True for _ in sub.iter(_HP + "tbl")):
                warnings.append("칸 안의 표는 글만 가져왔습니다.")
            cells.append(dict(
                row=max(0, _int(addr.get("rowAddr"))), col=max(0, _int(addr.get("colAddr"))),
                rowspan=max(1, _int(span.get("rowSpan"), 1)) if span is not None else 1,
                colspan=max(1, _int(span.get("colSpan"), 1)) if span is not None else 1,
                w=_int(size.get("width")) if size is not None else 0, h=_int(size.get("height")) if size is not None else 0,
                bf=borders.get(tc.get("borderFillIDRef")), text=text, bold=bold, align=align))
    if not cells:
        raise ValueError("표에 칸이 없습니다.")
    return cells


def import_hwpx_table(path, index=0, cell_mm=5.0):
    """HWPX 파일의 index번째(0부터, 문서 순서) 최상위 표 → (sketch, info). 실패는 ValueError."""
    if isinstance(index, bool) or not isinstance(index, int) or index < 0:
        raise ValueError("표 번호는 0 이상의 정수여야 합니다.")
    _check_cell_mm(cell_mm)
    tables, header = _read_document(path)
    if not tables:
        raise ValueError("HWPX에 표가 없습니다.")
    if index >= len(tables):
        raise ValueError(f"표 번호가 범위를 벗어났습니다(표 {len(tables)}개).")
    sketch, info = _convert(tables[index], _read_header(header), cell_mm)
    return sketch, dict(info, tables=len(tables))


def list_hwpx_tables(path, cell_mm=5.0, limit=200):
    """HWPX 파일의 최상위 표 요약(화면의 표 고르기 목록). 표마다 번호·행×열·첫 칸 글,
    지금 칸 간격으로 가져올 수 있는지(ok)와 못 하면 이유(reason). 표가 없으면 빈 목록."""
    _check_cell_mm(cell_mm)
    tables, header = _read_document(path)
    head = _read_header(header)
    items = []
    for index, table in enumerate(tables[:limit]):
        item = dict(index=index, rows=0, cols=0, first_text="", ok=True, reason="")
        try:
            cells = _read_cells(table, *head, [])
            item["rows"] = max(c["row"] + c["rowspan"] for c in cells)
            item["cols"] = max(c["col"] + c["colspan"] for c in cells)
            first = min(cells, key=lambda c: (c["row"], c["col"]))
            text = " ".join(first["text"].split())
            item["first_text"] = text if len(text) <= 40 else text[:39] + "…"
            _convert(table, head, cell_mm)
        except ValueError as exc:
            item.update(ok=False, reason=str(exc))
        items.append(item)
    return dict(tables=items, total=len(tables))


def _convert(table, head, cell_mm):
    """최상위 표 하나 → (sketch, info). 너무 크면 ValueError."""
    borders, chars, paras = head
    warnings = []
    cells = _read_cells(table, borders, chars, paras, warnings)
    n_rows = max(c["row"] + c["rowspan"] for c in cells)
    n_cols = max(c["col"] + c["colspan"] for c in cells)
    if n_rows > MAX_GRID or n_cols > MAX_GRID:
        raise ValueError("표가 너무 커서 표그림판(200칸)에 들어가지 않습니다.")
    widths, heights = _sizes(cells, n_cols, "col", cell_mm, warnings), _sizes(cells, n_rows, "row", cell_mm, warnings)
    xs, ys = [_MARGIN], [_MARGIN]
    for w in widths:
        xs.append(xs[-1] + w)
    for h in heights:
        ys.append(ys[-1] + h)
    if xs[-1] + _MARGIN > MAX_GRID or ys[-1] + _MARGIN > MAX_GRID:
        raise ValueError(f"표가 너무 커서 표그림판(200칸)에 들어가지 않습니다(칸 간격 {cell_mm:g}mm). 칸 간격을 늘려 보세요.")
    segments, texts, fills = {}, [], []

    def mark(key, line):
        old = segments.get(key)
        if line is not None and (old is None or _RANK[line[0]] > _RANK[old[0]]):
            segments[key] = line

    lost = set()
    for c in cells:
        x0, x1, y0, y1 = xs[c["col"]], xs[c["col"] + c["colspan"]], ys[c["row"]], ys[c["row"] + c["rowspan"]]
        bf = c["bf"] or dict(sides={}, fill=None, extra=None)
        if bf["fill"]:
            fills.append(dict(x1=x0, y1=y0, x2=x1, y2=y1, color=bf["fill"]))
        if bf["extra"]:
            lost.add({"image": "칸 그림 배경", "gradient": "그라데이션 배경"}[bf["extra"]])
        if bf["sides"].get("_diagonal"):
            lost.add("대각선")
        side = bf["sides"]
        for x in range(x0, x1):
            mark(("h", y0, x), side.get("top"))
            mark(("h", y1, x), side.get("bottom"))
        for y in range(y0, y1):
            mark(("v", x0, y), side.get("left"))
            mark(("v", x1, y), side.get("right"))
        if c["text"].strip():
            texts.append(dict(x=x0, y=y0, text=c["text"][:10000], bold=c["bold"], align=c["align"]))
    warnings.extend(f"{name}은(는) 표그림판에서 표현하지 못해 제외했습니다." for name in sorted(lost))
    runs = []
    for (kind, at, start), (width, style, color) in sorted(segments.items()):
        last = runs[-1] if runs else None
        if last and last[:2] == [kind, at] and last[3:] == [width, style, color] and last[2][1] == start:
            last[2][1] = start + 1
        else:
            runs.append([kind, at, [start, start + 1], width, style, color])
    lines = []
    for kind, at, (start, end), width, style, color in runs:
        coords = dict(x1=start, y1=at, x2=end, y2=at) if kind == "h" else dict(x1=at, y1=start, x2=at, y2=end)
        lines.append({**coords, "width": width, "style": style, **({} if color == "#000000" else {"color": color})})
    sketch = dict(grid=dict(cols=max(36, xs[-1] + _MARGIN), rows=max(50, ys[-1] + _MARGIN), cell_mm=float(cell_mm)),
                  lines=lines, texts=texts, images=[], fills=fills)
    return sketch, dict(rows=n_rows, cols=n_cols, warnings=sorted(set(warnings)))


class _HTMLTableParser(HTMLParser):
    """첫 최상위 HTML 표의 행·칸(속성·글·굵게·정렬)을 모은다. 칸 안 표는 글만 이어 붙인다."""

    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.rows, self.depth, self.done = [], 0, False
        self.cell, self.bold_depth, self.paragraph = None, 0, []

    def handle_starttag(self, tag, attrs):
        if self.done:
            return
        attrs = dict(attrs)
        if tag == "table":
            self.depth += 1
            return
        if self.depth != 1 and not (self.depth > 1 and self.cell is not None):
            return
        if self.depth == 1 and tag == "tr":
            self.rows.append([])
        elif self.depth == 1 and tag in ("td", "th") and self.rows:
            if sum(len(r) for r in self.rows) >= MAX_CELLS:
                raise ValueError("표 칸이 너무 많습니다(2000칸 넘음).")
            self.cell = {
                "style": attrs.get("style") or "",
                "colspan": _int(attrs.get("colspan"), 1),
                "rowspan": _int(attrs.get("rowspan"), 1),
                "lines": [],
                "bold": False,
                "align": "",
            }
            self.paragraph = []
        elif self.cell is not None:
            style = (attrs.get("style") or "").lower()
            if tag in ("b", "strong") or re.search(r"font-weight\s*:\s*(bold|[6-9]00)", style):
                self.bold_depth += 1
                self.cell["bold"] = True
            if tag == "p":
                match = re.search(r"text-align\s*:\s*(left|center|right|justify)", style)
                if match and not self.cell["align"]:
                    self.cell["align"] = match.group(1)
            if tag == "br":
                self._end_paragraph()

    def handle_endtag(self, tag):
        if self.done:
            return
        if tag == "table":
            self.depth -= 1
            if self.depth == 0:
                self.done = True
            return
        if self.cell is None:
            return
        if tag == "p":
            self._end_paragraph()
        elif self.depth == 1 and tag in ("td", "th"):
            self._end_paragraph()
            self.rows[-1].append(self.cell)
            self.cell = None

    def handle_data(self, data):
        if self.cell is not None and not self.done:
            self.paragraph.append(data)

    def _end_paragraph(self):
        if self.cell is not None and self.paragraph:
            text = re.sub(r"\s+", " ", "".join(self.paragraph).replace("\xa0", " ")).strip()
            self.cell["lines"].append(text)
        self.paragraph = []


def _pt(style, name):
    match = re.search(name + r"\s*:\s*([\d.]+)\s*pt", style, re.I)
    return float(match.group(1)) if match else None


def _html_background(style):
    match = re.search(r"background(?:-color)?\s*:\s*([^;]+)", style, re.I)
    if not match or "#" not in match.group(1):
        return None
    match_color = re.search(r"#([0-9a-fA-F]{6})\b", match.group(1))
    if not match_color:
        return None
    color = ("#" + match_color.group(1)).upper()
    return None if color in (None, "#FFFFFF") else color


def _html_border(style, side):
    match = re.search(r"border-" + side + r"\s*:\s*([^;]+)", style, re.I)
    if not match:
        match = re.search(r"border\s*:\s*([^;]+)", style, re.I)
    if not match:
        return None
    value = match.group(1).lower()
    if "none" in value or "hidden" in value:
        return None
    size = re.search(r"([\d.]+)\s*pt", value)
    points = float(size.group(1)) if size else 0.5
    kind = "dash" if "dash" in value else "dot" if "dot" in value else "solid"
    color_m = re.search(r"#([0-9a-fA-F]{6})\b", value)
    color = ("#" + color_m.group(1)).upper() if color_m else "#000000"
    if "double" in value:
        return ("double", "solid", color)
    return ("thick" if points >= 0.9 else "thin", kind, color)


def _place_html_cells(rows):
    taken, cells = set(), []
    for r, row in enumerate(rows):
        c = 0
        for cell in row:
            while (r, c) in taken:
                c += 1
            rs = max(1, min(MAX_GRID, cell["rowspan"]))
            cs = max(1, min(MAX_GRID, cell["colspan"]))
            for dr in range(rs):
                for dc in range(cs):
                    taken.add((r + dr, c + dc))
            cells.append((r, c, rs, cs, cell))
            c += cs
    return cells


def import_html_table(html: str | bytes, cell_mm: float = 5.0) -> tuple[dict, dict]:
    """클립보드 HTML(문자열·바이트)에서 표그림판 그림 (sketch, info)을 만든다.

    순수 파이썬(표준 html.parser)으로 실행되며 COM이나 외부 엔진을 쓰지 않는다.
    """
    _check_cell_mm(cell_mm)
    if isinstance(html, bytes):
        html = html.decode("utf-8", errors="replace")
    if len(html) > 20 * 1024 * 1024:
        raise ValueError("복사한 내용이 너무 큽니다.")
    start = html.find("<!--StartFragment-->")
    parser = _HTMLTableParser()
    parser.feed(html[start:] if start >= 0 else html)
    rows = [row for row in parser.rows if row]
    if not rows:
        raise ValueError("클립보드에 한/글 표가 없습니다. 한/글에서 표를 선택해 복사(Ctrl+C)한 뒤 다시 눌러 주세요.")

    raw_cells = _place_html_cells(rows)
    n_rows = max(r + rs for r, _, rs, _, _ in raw_cells)
    n_cols = max(c + cs for _, c, _, cs, _ in raw_cells)
    if n_rows > MAX_GRID or n_cols > MAX_GRID:
        raise ValueError("표가 너무 커서 표그림판(200칸)에 들어가지 않습니다.")

    # 각 셀을 HWPUNIT으로 환산해 _sizes로 계산
    items = []
    warnings = []
    for r, c, rs, cs, cell in raw_cells:
        pw = _pt(cell["style"], "width")
        ph = _pt(cell["style"], "height")
        w_unit = int(pw * PT_TO_MM * UNIT_MM) if pw else 0
        h_unit = int(ph * PT_TO_MM * UNIT_MM) if ph else 0
        text = "\n".join(line for line in cell["lines"] if line).strip()
        align = {"justify": "left"}.get(cell["align"], cell["align"]) or "center"
        items.append({
            "row": r,
            "col": c,
            "rowspan": rs,
            "colspan": cs,
            "w": w_unit,
            "h": h_unit,
            "style": cell["style"],
            "text": text,
            "bold": cell["bold"],
            "align": align,
        })

    widths = _sizes(items, n_cols, "col", cell_mm, warnings)
    heights = _sizes(items, n_rows, "row", cell_mm, warnings)

    xs, ys = [_MARGIN], [_MARGIN]
    for w in widths:
        xs.append(xs[-1] + w)
    for h in heights:
        ys.append(ys[-1] + h)

    if xs[-1] + _MARGIN > MAX_GRID or ys[-1] + _MARGIN > MAX_GRID:
        raise ValueError(f"표가 너무 커서 표그림판(200칸)에 들어가지 않습니다(칸 간격 {cell_mm:g}mm). 칸 간격을 늘려 보세요.")

    segments, texts, fills = {}, [], []

    def mark(key, line):
        old = segments.get(key)
        if line is not None and (old is None or _RANK[line[0]] > _RANK[old[0]]):
            segments[key] = line

    for it in items:
        x0, x1 = xs[it["col"]], xs[it["col"] + it["colspan"]]
        y0, y1 = ys[it["row"]], ys[it["row"] + it["rowspan"]]
        bg = _html_background(it["style"])
        if bg:
            fills.append(dict(x1=x0, y1=y0, x2=x1, y2=y1, color=bg))
        for x in range(x0, x1):
            mark(("h", y0, x), _html_border(it["style"], "top"))
            mark(("h", y1, x), _html_border(it["style"], "bottom"))
        for y in range(y0, y1):
            mark(("v", x0, y), _html_border(it["style"], "left"))
            mark(("v", x1, y), _html_border(it["style"], "right"))
        if it["text"]:
            texts.append(dict(x=x0, y=y0, text=it["text"][:10000], bold=it["bold"], align=it["align"]))

    runs = []
    for (kind, at, start), (width, style, color) in sorted(segments.items()):
        last = runs[-1] if runs else None
        if last and last[:2] == [kind, at] and last[3:] == [width, style, color] and last[2][1] == start:
            last[2][1] = start + 1
        else:
            runs.append([kind, at, [start, start + 1], width, style, color])

    lines = []
    for kind, at, (start, end), width, style, color in runs:
        coords = dict(x1=start, y1=at, x2=end, y2=at) if kind == "h" else dict(x1=at, y1=start, x2=at, y2=end)
        lines.append({**coords, "width": width, "style": style, **({} if color == "#000000" else {"color": color})})

    sketch = dict(
        grid=dict(cols=max(36, xs[-1] + _MARGIN), rows=max(50, ys[-1] + _MARGIN), cell_mm=float(cell_mm)),
        lines=lines,
        texts=texts,
        images=[],
        fills=fills,
    )
    return sketch, dict(rows=n_rows, cols=n_cols, warnings=sorted(set(warnings)))
