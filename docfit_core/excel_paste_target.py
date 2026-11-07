"""엑셀 클립보드 HTML(CF_HTML)을 몽땅클립보드 '목표 결과' 형태로 바꾼다 (순수 Python, 한/글·Windows 불필요).

TODO.md '엑셀 표를 한/글에 붙여넣을 때 목표 결과' 분석을 규칙으로 옮긴 것이다.
 ① 숫자 칸 앞쪽 공백 제거(회계 서식 `_-* #,##0_-`가 칸 폭만큼 채운 공백·mso-spacerun·&nbsp;). 뒤 공백은 한 칸만 남긴다.
 ② 글자 없이 공백(스페이스·&nbsp;·전각 공백 U+3000)뿐인 칸은 완전히 빈 칸으로.
 ③ 열 너비를 한 번만 정해 모든 행의 칸에 같은 너비를 준다(<col>·<td width>·표 너비 일치). 엑셀 열 너비 비례.
 ④ 칸 여백을 정상 값(좌우 510·상하 141 한/글 단위 ≈ 5.1pt·1.4pt)으로 지정 — 엑셀의 `mso-ignore:padding`(1px)을 대체.
 ⑤ 맨 위 빈 행 등 선택 범위의 빈 행·열은 지우지 않고 보존(목표 결과는 보존).
 ⑥ 셀 음영은 기본 유지, `shading="strip"`이면 기존 xlshade 모듈(excel_html_shading)로 제거.
머리글 행 반복(HWPX 속성)은 HTML로 표현할 수 없어 이 변환 대상이 아니다(HWPX 경로 `excel_table_hwpx`의 몫).
구조(행·칸 수, 병합)나 글자(공백 제외)가 바뀌었거나 예외가 나면 원본을 그대로 돌려준다.
"""
from __future__ import annotations

import re
from html.parser import HTMLParser

from .excel_html_shading import _rebuild_header, strip_cell_shading_report
from .excel_table import _NUMERIC

PADDING_LR_PT = 5.1   # 510 한/글 단위
PADDING_TB_PT = 1.4   # 141 한/글 단위
_BLANKS = "\\s\u00a0\u3000\u2000-\u200a\u202f\u205f"
_BLANK_ONLY = re.compile(rf"[{_BLANKS}]*\Z")
_SPACERUN = r"<span\b[^>]*mso-spacerun[^>]*>(?:[%s]|&nbsp;|&#160;|&#12288;)*</span\s*>" % _BLANKS
_LEAD = re.compile(rf"\A(?:[{_BLANKS}]|&nbsp;|&#160;|&#12288;|{_SPACERUN})+", re.I)
_TRAIL = re.compile(rf"(?:[{_BLANKS}]|&nbsp;|&#160;|&#12288;|{_SPACERUN})+\Z", re.I)
_TAG = re.compile(r"<[^>]*>")
_ENTITY_BLANK = re.compile(r"&nbsp;|&#160;|&#12288;", re.I)
_CELL = re.compile(r"(<(td|th)\b)((?:\"[^\"]*\"|'[^']*'|[^>\"'])*)>(.*?)</\2\s*>", re.I | re.S)
_ROW = re.compile(r"<tr\b[^>]*>.*?</tr\s*>", re.I | re.S)
_COL = re.compile(r"<col\b((?:\"[^\"]*\"|'[^']*'|[^>\"'])*)>", re.I)
_TABLE_TAG = re.compile(r"<table\b((?:\"[^\"]*\"|'[^']*'|[^>\"'])*)>", re.I)
_TD_RULE = re.compile(r"(\btd\s*\{)([^{}]*)(\})", re.I)
_ATTR = re.compile(r"""([^\s=/>"']+)(?:\s*=\s*("[^"]*"|'[^']*'|[^\s>]+))?""")


def _attrs(raw):
    return {m.group(1).lower(): (m.group(2) or "").strip("\"'") for m in _ATTR.finditer(raw)}


def _set_attr(raw, name, value):
    """태그 속성 문자열에서 name을 value로 바꾸거나 더한다(따옴표는 작은따옴표로 통일)."""
    pattern = re.compile(rf"(?<![\w-]){name}\s*=\s*(?:\"[^\"]*\"|'[^']*'|[^\s>]+)", re.I)
    new = f"{name}={value}"
    return pattern.sub(lambda _m: new, raw, count=1) if pattern.search(raw) else raw.rstrip() + " " + new


def _set_style(raw, **decls):
    """style 속성의 선언을 바꾸거나 더한다. 없으면 style을 만든다."""
    pattern = re.compile(r"(?<![\w-])style\s*=\s*(\"[^\"]*\"|'[^']*'|[^\s>]+)", re.I)
    match = pattern.search(raw)
    body = match.group(1).strip("\"'") if match else ""
    for key, value in decls.items():
        name = key.replace("_", "-")
        body, n = re.subn(rf"(?<![\w-]){name}\s*:\s*[^;]*", lambda _m: f"{name}:{value}", body, count=1, flags=re.I)
        if not n:
            body = (body.rstrip().rstrip(";") + ";" if body.strip() else "") + f"{name}:{value}"
    new = f"style='{body}'"
    return pattern.sub(lambda _m: new, raw, count=1) if match else raw.rstrip() + " " + new


def _plain(inner):
    return re.sub(rf"[{_BLANKS}]+", " ", _ENTITY_BLANK.sub(" ", _TAG.sub("", inner))).strip()


def _fix_inner(inner):
    """(새 안쪽 HTML, 바꿈 종류) — 종류: 'blank'(빈 칸 만들기), 'numeric'(앞 공백 제거), ''."""
    if "<img" not in inner.lower() and "<br" not in inner.lower() and _BLANK_ONLY.match(_ENTITY_BLANK.sub(" ", _TAG.sub("", inner))):
        return ("", "blank") if inner else (inner, "")
    text = _plain(inner)
    if not _NUMERIC.fullmatch(text):
        return inner, ""
    new = _LEAD.sub("", inner)
    trailing = _TRAIL.search(new)
    if trailing:  # 뒤쪽은 한 칸만(목표 결과 `88,180 `)
        new = new[:trailing.start()] + " "
    return (new, "numeric") if new != inner else (inner, "")


def _col_widths(html):
    """<col>에서 열 너비(px)를 펼쳐 모은다. width 속성(px)을 우선, 없으면 style의 pt를 px로."""
    widths = []
    for m in _COL.finditer(html):
        a = _attrs(m.group(1))
        span = max(1, int(a["span"])) if a.get("span", "").isdigit() else 1
        value = 0.0
        try:
            value = float(a.get("width", "") or 0)
        except ValueError:
            pass
        if value <= 0:
            pt = re.search(r"(?:^|[;\s])width\s*:\s*([\d.]+)\s*pt", a.get("style", ""), re.I)
            value = float(pt.group(1)) / 0.75 if pt else 0.0
        widths.extend([value] * span)
    return widths


def _fmt(value):
    return ("%.2f" % value).rstrip("0").rstrip(".")


def _normalize_widths(html, report):
    """열 너비를 한 번 정해 <col>·모든 행의 칸·표 너비에 같은 값을 준다. 격자가 맞지 않으면 건드리지 않는다."""
    widths = _col_widths(html)
    if not widths or any(w <= 0 for w in widths):
        report["width"] = "열 너비 정보 없음"
        return html
    count = len(widths)
    taken, placed = set(), []
    rows = list(_ROW.finditer(html))
    for y, row in enumerate(rows):
        x = 0
        for cell in _CELL.finditer(row.group(0)):
            a = _attrs(cell.group(3))
            while (y, x) in taken:
                x += 1
            cs = max(1, int(a["colspan"])) if a.get("colspan", "").isdigit() else 1
            rs = max(1, int(a["rowspan"])) if a.get("rowspan", "").isdigit() else 1
            if x + cs > count:
                report["width"] = "칸 수가 열 정보와 다름"
                return html
            for dy in range(rs):
                for dx in range(cs):
                    taken.add((y + dy, x + dx))
            placed.append((y, cell.start(), x, cs))
            x += cs
    # 1) <col>을 열마다 하나씩, 같은 너비로 다시 쓴다.
    first = _COL.search(html)
    last = list(_COL.finditer(html))[-1]
    cols = "".join(f"<col width={_fmt(w)} style='width:{_fmt(w * 0.75)}pt'>\n" for w in widths)
    html = html[:first.start()] + cols.rstrip("\n") + html[last.end():]
    # 2) 모든 칸에 자기 열 범위의 너비 합을 준다(행을 뒤에서부터 고쳐 위치가 어긋나지 않게).
    rows = list(_ROW.finditer(html))
    by_row = {}
    for y, start, x, cs in placed:
        by_row.setdefault(y, []).append((x, cs))
    for y in reversed(range(len(rows))):
        row = rows[y].group(0)
        spans = iter(by_row.get(y, []))

        def fix(m, spans=spans):
            x, cs = next(spans)
            w = sum(widths[x:x + cs])
            raw = _set_style(_set_attr(m.group(3), "width", _fmt(w)), width=f"{_fmt(w * 0.75)}pt")
            return f"{m.group(1)}{raw}>{m.group(4)}</{m.group(2)}>"
        html = html[:rows[y].start()] + _CELL.sub(fix, row) + html[rows[y].end():]
    # 3) 표 전체 너비.
    total = sum(widths)
    html = _TABLE_TAG.sub(lambda m: "<table" + _set_style(_set_attr(m.group(1), "width", _fmt(total)), width=f"{_fmt(total * 0.75)}pt") + ">", html, count=1)
    report["width"] = f"{count}열, 표 너비 {_fmt(total)}px"
    return html


def _normalize_padding(html):
    """엑셀의 td 기본 규칙(padding 1px, mso-ignore:padding)을 정상 칸 여백으로 바꾼다."""
    def rule(m):
        body = re.sub(r"(?:padding(?:-(?:top|right|bottom|left))?|mso-ignore)\s*:[^;]*;?\s*", "", m.group(2), flags=re.I)
        pad = (f"padding-top:{PADDING_TB_PT}pt;padding-right:{PADDING_LR_PT}pt;"
               f"padding-bottom:{PADDING_TB_PT}pt;padding-left:{PADDING_LR_PT}pt;")
        return m.group(1) + body.rstrip() + ("" if body.rstrip().endswith(";") or not body.strip() else ";") + pad + m.group(3)
    new, n = _TD_RULE.subn(rule, html)
    return new, bool(n)


class _Shape(HTMLParser):
    """바뀌면 안 되는 것: 행 수·칸 구성(병합)과 공백을 뺀 글자."""

    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.shape, self.text, self.skip = [], [], False

    def handle_starttag(self, tag, attrs):
        a = dict(attrs)
        if tag == "style":
            self.skip = True
        if tag in ("tr", "td", "th"):
            self.shape.append((tag, a.get("rowspan"), a.get("colspan")))

    def handle_endtag(self, tag):
        if tag == "style":
            self.skip = False

    def handle_data(self, data):
        if not self.skip:
            self.text.append(data)


def _shape(html):
    p = _Shape()
    p.feed(html)
    p.close()
    return p.shape, re.sub(rf"[{_BLANKS}]+", "", "".join(p.text))


def _transform(data, shading):
    is_text = isinstance(data, str)
    raw = data.encode("utf-8") if is_text else bytes(data)
    match = re.search(rb"StartHTML:\s*(\d+)", raw)
    start = int(match.group(1)) if match else 0
    header, body = (raw[:start], raw[start:]) if match and 0 < start < len(raw) else (b"", raw)
    html = body.decode("utf-8")
    if not re.search(r"<t[dh]\b", html, re.I):
        return data, dict(changed=False, reason="표 셀이 없는 HTML")
    report = dict(changed=False, reason="", numeric=0, blank=0, width="", padding=False, shading_removed=0)

    def cell(m):
        inner, kind = _fix_inner(m.group(4))
        if kind:
            report["blank" if kind == "blank" else "numeric"] += 1
        return f"{m.group(1)}{m.group(3)}>{inner}</{m.group(2)}>"
    new = _CELL.sub(cell, html)
    new = _normalize_widths(new, report)
    new, report["padding"] = _normalize_padding(new)
    if shading == "strip":
        new, sh = strip_cell_shading_report(new)
        report["shading_removed"] = sh["removed"]
    if _shape(html) != _shape(new):
        return data, dict(report, changed=False, reason="구조 검증 실패")
    if new == html:
        return data, dict(report, changed=False, reason="바꿀 것 없음")
    out = new.encode("utf-8")
    if header:
        out = _rebuild_header(header, out) + out
    report["changed"] = True
    return (out.decode("utf-8") if is_text else out), report


def to_target_html_report(data, *, shading="keep"):
    """(결과, 보고). shading은 "keep"(기본)·"strip". 실패·비정상이면 원본과 reason."""
    if not data or not isinstance(data, (str, bytes, bytearray)):
        return data, dict(changed=False, reason="비어 있거나 지원하지 않는 형식")
    try:
        return _transform(data, shading)
    except Exception as exc:  # 원본 보존이 우선이다.
        return data, dict(changed=False, reason=f"변환 실패: {type(exc).__name__}")


def to_target_html(data, *, shading="keep"):
    return to_target_html_report(data, shading=shading)[0]


def clean_tsv(tsv):
    """글자 클립보드(탭 구분)에도 같은 공백 규칙: 숫자 앞 공백 제거, 공백뿐인 칸은 빈 칸."""
    if not tsv or not isinstance(tsv, str):
        return tsv

    def one(value):
        if _BLANK_ONLY.match(value):
            return ""
        stripped = value.lstrip(" \u00a0\u3000")
        return stripped if _NUMERIC.fullmatch(stripped.strip(" \u00a0\u3000")) else value
    parts = re.split(r"(\r?\n)", tsv)  # 줄바꿈 종류를 그대로 보존
    return "".join(p if i % 2 else "\t".join(one(v) for v in p.split("\t")) for i, p in enumerate(parts))
