"""공문서 장·절·세부 띠/본문 계층과 표 부속 제목을 비파괴 분석한다."""
from collections import Counter
import copy
import re
import unicodedata

LEVELS = {"title": 0, "overview": 1, "chapter": 2, "section": 3,
          "detail": 4, "square": 5, "circle": 6, "dash": 7}
ROMAN = r"(?:[Ⅰ-Ⅿ]+|(?=[IVXLCDM]+(?:[\s.．:：)]|$))M{0,3}(?:CM|CD|D?C{0,3})(?:XC|XL|L?X{0,3})(?:IX|IV|V?I{0,3}))"
NUMBER = r"(?:[0-9]{1,3}(?:[.-][0-9]{1,3})+|[0-9]{1,3}|[\u2776-\u277f]|\U000f02b2)"
TOKEN = re.compile(rf"^({ROMAN}|{NUMBER}|[①-⑳]|[가나다라마바사아자차카타파하])(?:[.．:：)]|\s|$)")
CAPTION = re.compile(r"^\s*(?:<[^<>\r\n]+>|\[[^\[\]\r\n]+\]|\([^()\r\n]+\)|〈[^〈〉\r\n]+〉|＜[^＜＞\r\n]+＞|［[^［］\r\n]+］|（[^（）\r\n]+）)\s*$")
SQUARE_GLYPHS = frozenset({"\U000f02b2"})

def tag(e):
    return e.tag.rsplit("}", 1)[-1]

def text(e):
    from .marker_spacing import xml_text
    return xml_text(e)

def marker(text_value):
    match = TOKEN.match(text_value.strip())
    if not match:
        return None
    token = match[1]
    if re.fullmatch(ROMAN, token) and token:
        return "roman", token
    if re.fullmatch(r"[0-9]{1,3}(?:[.-][0-9]{1,3})+", token):
        return "detail", token
    if token.isascii() and token.isdigit() or token in SQUARE_GLYPHS:
        return "number", token
    if token in "❶❷❸❹❺❻❼❽❾❿":
        return "detail", token
    return "circled" if unicodedata.category(token) == "No" else "hangul", token

def band(table):
    """표 모양만으로 데이터를 제목으로 보지 않도록 번호·제목·빈 간격칸을 함께 확인."""
    if int(table.get("rowCnt", "0")) != 1:
        return None
    rows = [r for r in table if tag(r) == "tr"]
    if len(rows) != 1:
        return None
    cells = [c for c in rows[0] if tag(c) == "tc"]
    cols = int(table.get("colCnt", "0"))
    if cols not in (1, 2, 3) or len(cells) != cols:
        return None
    if any(tag(x) in {"tbl", "pic", "ole", "rect", "fieldBegin"} for c in cells for x in c.iter()):
        return None
    for cell in cells:
        span = cell.find("{*}cellSpan")
        if span is not None and (span.get("rowSpan", "1") != "1" or span.get("colSpan", "1") != "1"):
            return None
    values = [text(c).strip() for c in cells]
    if cols == 3 and values[1]:
        return None
    found = marker(values[0])
    if not found:
        return None
    family, token = found
    if cols == 1:
        title = values[0][len(token):].lstrip(" .．:：)")
    else:
        if values[0].rstrip(" .．:：)") != token:
            return None
        title = values[-1]
    if not title or len(title) > 160 or "\n" in title or re.match(r"^[□ㅇ○\-※*]", title):
        return None
    role = "chapter" if family == "roman" else "section" if family == "number" else "detail"
    return {"role": role, "level": LEVELS[role], "layout": f"1x{cols}",
            "variant": "B" if cols == 1 else "A" if cols == 3 else "exception",
            "marker": token, "marker_family": family, "text": title,
            "glyph_mapping": "font-dependent" if token in SQUARE_GLYPHS else "unicode"}

def primary(p, index):
    runs = [(len(text(r).strip()), r) for r in p if tag(r) == "run"]
    run = max(runs, key=lambda item: item[0])[1] if runs else None
    return dict(index.char(run.get("charPrIDRef")) or {}) if run is not None else {}

def analyze(roots, index, classify=None):
    nodes, captions, candidates = [], [], []
    source_order = 0
    for section, root in enumerate(roots):
        preceding = None
        previous_form = None
        for position, p in enumerate(x for x in root if tag(x) == "p"):
            tables = [t for r in p if tag(r) == "run" for t in r if tag(t) == "tbl"]
            if tables:
                for table in tables:
                    source_order += 1
                    info = band(table)
                    legacy = classify(table) if classify else None
                    if info:
                        # Keep source typography separate from the logical role.
                        paragraphs = table.findall(".//{*}p")
                        info["typography"] = primary(paragraphs[-1], index) if paragraphs else {}
                    elif legacy and legacy.startswith("title"):
                        info = {"role": "title", "level": LEVELS["title"], "text": text(table)}
                    elif legacy == "overview" or legacy == "box" and previous_form == "title":
                        info = {"role": "overview", "level": LEVELS["overview"], "text": text(table),
                                "required_alignment": "JUSTIFY"}
                    if info:
                        nodes.append({**info, "section": section, "paragraph": position, "table_order": source_order})
                        previous_form = info["role"]
                    else:
                        previous_form = None
                    if (preceding and int(table.get("rowCnt", "0")) >= 2
                            and int(table.get("colCnt", "0")) >= 2 and CAPTION.fullmatch(preceding["text"])
                            and len(preceding["text"]) <= 200):
                        captions.append({**preceding, "table_order": source_order,
                            "role": "table_title", "relation": "immediately_before",
                            "section": section, "table_paragraph": position})
                    preceding = None
                continue
            value = text(p).strip()
            if not value:
                preceding = None  # adjacent means immediately above, not an arbitrary prior sentence.
                continue
            typography = primary(p, index)
            base = {"text": value, "section": section, "paragraph": position,
                    "typography": typography, "paragraph_style": dict(index.para(p.get("paraPrIDRef")) or {})}
            preceding = base
            role = next((r for symbol, r in (("□", "square"), ("ㅁ", "square"), ("ㅇ", "circle"), ("○", "circle"), ("-", "dash"))
                         if re.match(re.escape(symbol) + r"(?=\s|$)", value)), None)
            if role:
                nodes.append({**base, "role": role, "level": LEVELS[role], "container": "text"})
            elif marker(value) and len(value) <= 160:
                candidates.append(base)
            previous_form = previous_form if previous_form == "title" and len(value) < 60 else None
    # Circled/Hangul/Roman prose is promoted only when typography and body context support a heading.
    body_sizes = [n["typography"].get("size", 0) for n in nodes if n["role"] == "square"]
    body_size = Counter(body_sizes).most_common(1)[0][0] if body_sizes else 0
    for candidate in candidates:
        following = next((n for n in nodes if n["section"] == candidate["section"]
                          and n["paragraph"] > candidate["paragraph"]), None)
        ty = candidate["typography"]
        if (following and following["role"] in {"square", "section", "detail"}
                and body_size and ty.get("size", 0) >= body_size
                and (ty.get("size", 0) > body_size or ty.get("bold")
                     or ty.get("font") != following.get("typography", {}).get("font"))):
            family, token = marker(candidate["text"])
            nodes.append({**candidate, "role": "section", "level": LEVELS["section"],
                          "marker": token, "marker_family": family, "container": "text"})
    nodes.sort(key=lambda n: (n["section"], n["paragraph"], n.get("table_order", 0)))
    stack = []
    for i, node in enumerate(nodes):
        node["id"] = i
        while stack and stack[-1]["level"] >= node["level"]:
            stack.pop()
        node["parent_id"] = stack[-1]["id"] if stack else None
        if node["role"] != "overview":
            stack.append(node)
    return {"version": 1, "levels": dict(LEVELS), "nodes": nodes, "table_titles": captions,
            "warnings": ["PUA 네모숫자의 숫자값은 글꼴 매핑 확인이 필요합니다."]
                        if any(n.get("glyph_mapping") == "font-dependent" for n in nodes) else []}

def justify_overview(header, table):
    """문단 모양을 복제해 JUSTIFY 적용; 공유 paraPr나 다른 표는 바꾸지 않는다."""
    group = header.find(".//{*}paraProperties")
    if group is None:
        return 0
    mapping = {}
    changed = 0
    by_id = {p.get("id"): p for p in group}
    next_id = max((int(i) for i in by_id if i and i.isdigit()), default=-1) + 1
    for p in table.findall("{*}tr/{*}tc/{*}subList/{*}p"):
        # Nested tables have their own formatting contract: do not descend into them.
        old = p.get("paraPrIDRef")
        original = by_id.get(old)
        if original is None:
            continue
        align = original.find("{*}align")
        if align is not None and align.get("horizontal") == "JUSTIFY":
            continue
        if old not in mapping:
            clone = copy.deepcopy(original)
            clone.set("id", str(next_id))
            next_id += 1
            align = clone.find("{*}align")
            if align is None:
                align = type(clone)(clone.tag.rsplit("}", 1)[0] + "}align", {})
                clone.insert(0, align)
            align.set("horizontal", "JUSTIFY")
            group.append(clone)
            mapping[old] = clone.get("id")
        p.set("paraPrIDRef", mapping[old])
        for cache in list(p):
            if tag(cache) == "linesegarray":
                p.remove(cache)
        changed += 1
    group.set("itemCnt", str(len(group)))
    return changed
