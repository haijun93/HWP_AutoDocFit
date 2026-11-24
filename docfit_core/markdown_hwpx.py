"""내부 Markdown → HWPX 생성기(공공 보고서 1단계, 앱 미연결).

annual/src/hwpx.rs의 ZIP·header/section·네이티브 표 골격을 Python으로
이식했다. 입력 해석 → XML 조립 → 검증 → 새 파일 게시 순으로 실행하며,
COM·kordoc·네트워크를 호출하지 않는다. 내어쓰기 실측은 후속 단계 몫이다.
"""
from __future__ import annotations

from dataclasses import dataclass, field
import html
import io
import re
import tempfile
from pathlib import Path
from zipfile import ZipFile, ZipInfo, ZIP_DEFLATED, ZIP_STORED

from defusedxml import ElementTree as ET

from .hwpx import validate_hwpx

BODY_WIDTH = 48_189  # annual과 같은 170mm(반올림)
HP = "http://www.hancom.co.kr/hwpml/2011/paragraph"
HH = "http://www.hancom.co.kr/hwpml/2011/head"
HC = "http://www.hancom.co.kr/hwpml/2011/core"
HS = "http://www.hancom.co.kr/hwpml/2011/section"
XML = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
MAX_INPUT_BYTES = 16 * 1024 * 1024
_HEADING = re.compile(r"^ {0,3}(#{1,6})\s+(.*?)(?:\s+#+)?\s*$")
_ITEM = re.compile(r"^(\s*)([□ㅇ※]|[-*+]|\d+[.)])[ \t]+(.*)$")
_SEP = re.compile(r"^:?-{3,}:?$")
_BR = re.compile(r"<br\s*/?>", re.I)


@dataclass(frozen=True)
class Run:
    text: str
    bold: bool = False
    italic: bool = False


@dataclass
class Paragraph:
    runs: list[Run]
    level: int = 0
    marker: str = ""
    depth: int = 0


@dataclass
class Table:
    rows: list[list[list[Run]]]


@dataclass
class Document:
    blocks: list[Paragraph | Table] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)


@dataclass(frozen=True)
class GenerationResult:
    path: Path
    warnings: tuple[str, ...]
    paragraphs: int
    tables: int


def _warn(warnings: list[str], message: str) -> None:
    if message not in warnings:
        warnings.append(message)


def inline_runs(text: str, warnings: list[str], bold=False, italic=False,
                _depth=0) -> list[Run]:
    """강조·이스케이프·코드·링크 레이블과 줄바꿈을 읽는다. 외부 자원은 읽지 않는다."""
    if _depth > 32:
        _warn(warnings, "인라인 중첩 32단 초과: 나머지를 일반 글자로 보존했습니다.")
        return [Run(text, bold, italic)]
    result: list[Run] = []
    buf: list[str] = []

    def flush():
        if buf:
            result.append(Run(html.unescape("".join(buf)), bold, italic))
            buf.clear()

    i = 0
    while i < len(text):
        if text[i] == "\\" and i + 1 < len(text) and text[i + 1] in r"\`*_{}[]()#+-.!|":
            buf.append(text[i + 1]); i += 2; continue
        br = _BR.match(text, i)
        if br:
            buf.append("\n"); i = br.end(); continue
        image = re.match(r"!\[([^\]]*)\]\([^)]*\)", text[i:])
        if image:
            _warn(warnings, "그림 삽입 미지원: Markdown 참조를 일반 글자로 보존했습니다.")
            buf.append(image.group()); i += image.end(); continue
        link = re.match(r"\[([^\]]*)\]\([^)]*\)", text[i:])
        if link:
            _warn(warnings, "하이퍼링크 대상 미지원: 표시 글자만 보존했습니다.")
            flush()
            result.extend(inline_runs(link[1], warnings, bold, italic, _depth + 1))
            i += link.end(); continue
        if text[i] == "`":
            ticks = re.match(r"`+", text[i:]).group()
            end = text.find(ticks, i + len(ticks))
            if end >= 0:
                flush(); result.append(Run(text[i + len(ticks):end], bold, italic))
                i = end + len(ticks); continue
        matched = False
        for delim in ("***", "___", "**", "__", "*", "_"):
            if not text.startswith(delim, i):
                continue
            # 낱말 중간 밑줄은 강조 시작으로 해석하지 않는다.
            if delim[0] == "_" and i and text[i - 1].isalnum():
                continue
            start = i + len(delim)
            end = text.find(delim, start)
            while end >= 0:
                escaped = len(text[:end]) - len(text[:end].rstrip("\\"))
                count = len(re.match(re.escape(delim[0]) + "+", text[end:]).group())
                if escaped % 2 or (len(delim) == 1 and count == 2):
                    end = text.find(delim, end + count)
                    continue
                # ***가 안쪽 기울임 + 바깥 굵게의 닫힘일 수도 있다.
                end += count - len(delim)
                break
            if end <= start or text[start].isspace():
                continue
            flush()
            result.extend(inline_runs(text[start:end], warnings,
                                     bold or len(delim) >= 2, italic or len(delim) != 2, _depth + 1))
            i = end + len(delim); matched = True; break
        if matched:
            continue
        buf.append(text[i]); i += 1
    flush()
    return result or [Run("", bold, italic)]


def _row(line: str) -> list[str]:
    line = line.strip()
    if line.startswith("|"):
        line = line[1:]
    if line.endswith("|") and not line.endswith("\\|"):
        line = line[:-1]
    return [cell.strip() for cell in re.split(r"(?<!\\)\|", line)]


def parse_markdown(source: str) -> Document:
    if not isinstance(source, str):
        raise TypeError("Markdown 입력은 문자열이어야 합니다.")
    if len(source.encode("utf-8")) > MAX_INPUT_BYTES:
        raise ValueError("Markdown 입력은 16MB 이하여야 합니다.")
    if any(not (c in "\t\n\r" or 0x20 <= ord(c) <= 0xD7FF or
                0xE000 <= ord(c) <= 0xFFFD or 0x10000 <= ord(c) <= 0x10FFFF) for c in source):
        raise ValueError("XML에 쓸 수 없는 제어 문자가 있습니다.")
    doc = Document()
    lines = source.lstrip("\ufeff").replace("\r\n", "\n").replace("\r", "\n").split("\n")
    pending: list[str] = []
    indents: list[int] = []

    def flush():
        if pending:
            # 원문의 줄바꿈(soft/hard)을 모두 보존한다. 문단 경계는 빈 줄이다.
            text = "\n".join(re.sub(r"(?: {2,}|\\)$", "", s) for s in pending)
            doc.blocks.append(Paragraph(inline_runs(text, doc.warnings)))
            pending.clear()

    i = 0
    while i < len(lines):
        line = lines[i]
        if not line.strip():
            flush(); indents.clear(); i += 1; continue
        fence = re.match(r"^\s*(`{3,}|~{3,})", line)
        if fence:
            flush(); indents.clear()
            _warn(doc.warnings, "코드 블록 전용 서식 미지원: 내용을 일반 문단으로 보존했습니다.")
            token = fence[1]; content = []; i += 1
            while i < len(lines) and not re.match(r"^\s*" + re.escape(token) + r"\s*$", lines[i]):
                content.append(lines[i]); i += 1
            doc.blocks.append(Paragraph([Run("\n".join(content))])); i += 1; continue
        if "|" in line and i + 1 < len(lines):
            sep = _row(lines[i + 1])
            if sep and all(_SEP.fullmatch(c) for c in sep):
                flush(); indents.clear()
                rows = [_row(line)]; cols = len(rows[0])
                if len(sep) != cols:
                    raise ValueError("Markdown 표 머리와 구분선의 열 수가 다릅니다.")
                i += 2
                while i < len(lines) and lines[i].strip() and "|" in lines[i]:
                    cells = _row(lines[i])
                    if len(cells) > cols:
                        raise ValueError("Markdown 표 데이터의 열 수가 머리보다 많습니다.")
                    rows.append(cells + [""] * (cols - len(cells))); i += 1
                if cols > 100 or len(rows) > 10_000:
                    raise ValueError("표는 100열·10,000행 이하여야 합니다.")
                doc.blocks.append(Table([[inline_runs(c, doc.warnings) for c in row] for row in rows]))
                continue
        head = _HEADING.match(line)
        if head:
            flush(); indents.clear()
            level = len(head[1])
            if level > 3:
                _warn(doc.warnings, "4~6단 제목 서식 미지원: 원문 표식을 일반 문단으로 보존했습니다.")
                doc.blocks.append(Paragraph(inline_runs(line, doc.warnings)))
            else:
                doc.blocks.append(Paragraph(inline_runs(head[2], doc.warnings), level=level))
            i += 1; continue
        item = _ITEM.match(line)
        if item:
            flush()
            width = len(item[1].expandtabs(4))
            while indents and indents[-1] >= width:
                indents.pop()
            indents.append(width)
            marker = "-" if item[2] in ("*", "+") else item[2]
            doc.blocks.append(Paragraph(inline_runs(item[3].lstrip(), doc.warnings),
                                        marker=marker, depth=len(indents) - 1))
            i += 1; continue
        # 목록의 들여쓴 후속 줄은 같은 항목의 줄바꿈으로 보존한다.
        if line.startswith((" ", "\t")) and doc.blocks and isinstance(doc.blocks[-1], Paragraph) and doc.blocks[-1].marker:
            doc.blocks[-1].runs.extend([Run("\n"), *inline_runs(line.strip(), doc.warnings)])
        else:
            indents.clear(); pending.append(line.replace("\t", " "))
        i += 1
    flush()
    if not doc.blocks:
        doc.blocks.append(Paragraph([Run("")]))
    return doc


def _attrs(**values) -> str:
    return " ".join(f'{k}="{html.escape(str(v), quote=True)}"' for k, v in values.items())


def _header() -> str:
    langs = ("HANGUL", "LATIN", "HANJA", "JAPANESE", "OTHER", "SYMBOL", "USER")
    scripts = ("hangul", "latin", "hanja", "japanese", "other", "symbol", "user")
    fonts = ''.join(f'<hh:fontface lang="{lang}" fontCnt="2">'
                    '<hh:font id="0" face="휴먼명조" type="TTF" isEmbedded="0"/>'
                    '<hh:font id="1" face="한컴돋움" type="TTF" isEmbedded="0"/></hh:fontface>' for lang in langs)
    borders = []
    for bid, color in ((1, "#E7EEF8"), (2, "#FFFFFF"), (3, None)):
        edges = ''.join(f'<hh:{edge}Border type="{"SOLID" if color else "NONE"}" width="0.12 mm" color="#AAB7C6"/>'
                        for edge in ("left", "right", "top", "bottom"))
        fill = f'<hc:fillBrush><hc:winBrush faceColor="{color}" hatchColor="#000000" alpha="0"/></hc:fillBrush>' if color else ''
        borders.append(f'<hh:borderFill id="{bid}" threeD="0" shadow="0" centerLine="NONE" breakCellSeparateLine="0">'
                       f'<hh:slash type="NONE" Crooked="0" isCounter="0"/><hh:backSlash type="NONE" Crooked="0" isCounter="0"/>{edges}{fill}</hh:borderFill>')
    chars = []
    for level in range(4):
        for flags in range(4):
            props = ''.join(f'<hh:{tag} {_attrs(**dict.fromkeys(scripts, value))}/>' for tag, value in
                            (("fontRef", int(level > 0)), ("ratio", 100), ("spacing", 0), ("relSz", 100), ("offset", 0)))
            emphasis = ('<hh:bold/>' if flags & 1 or level else '') + ('<hh:italic/>' if flags & 2 else '')
            chars.append(f'<hh:charPr id="{level * 4 + flags}" height="{1600 if level else 1100}" textColor="#202020" shadeColor="none" useFontSpace="0" useKerning="0" symMark="NONE" borderFillIDRef="3">'
                         f'{props}{emphasis}<hh:underline type="NONE" shape="SOLID" color="#000000"/>'
                         '<hh:strikeout shape="NONE" color="#000000"/><hh:outline type="NONE"/><hh:shadow type="NONE" color="#B2B2B2" offsetX="10" offsetY="10"/></hh:charPr>')
    paras = []
    for level in range(4):
        margins = ''.join(f'<hc:{key} value="{value}" unit="HWPUNIT"/>' for key, value in
                          (("intent", 0), ("left", 0), ("right", 0), ("prev", 300 if level else 0), ("next", 300 if level else 0)))
        paras.append(f'<hh:paraPr id="{level}" tabPrIDRef="0" condense="0" fontLineHeight="0" snapToGrid="0" suppressLineNumbers="0" checked="0" textDir="AUTO">'
                     f'<hh:align horizontal="{"LEFT" if level else "JUSTIFY"}" vertical="BASELINE"/>'
                     '<hh:heading type="NONE" idRef="0" level="0"/>'
                     f'<hh:breakSetting breakLatinWord="KEEP_WORD" breakNonLatinWord="BREAK_WORD" widowOrphan="1" keepWithNext="{int(level > 0)}" keepLines="0" pageBreakBefore="0" lineWrap="BREAK"/>'
                     f'<hh:autoSpacing eAsianEng="0" eAsianNum="0"/><hh:margin>{margins}</hh:margin><hh:lineSpacing type="PERCENT" value="160"/>'
                     '<hh:border borderFillIDRef="3" offsetLeft="0" offsetRight="0" offsetTop="0" offsetBottom="0" connect="0" ignoreMargin="0"/></hh:paraPr>')
    styles = ''.join(f'<hh:style id="{level}" type="PARA" name="{("제목 " + str(level)) if level else "바탕글"}" engName="{("Heading " + str(level)) if level else "Normal"}" paraPrIDRef="{level}" charPrIDRef="{level * 4}" nextStyleIDRef="0" langID="1042" lockForm="0"/>' for level in range(4))
    return (XML + f'<hh:head xmlns:hh="{HH}" xmlns:hc="{HC}" version="1.5" secCnt="1">'
            '<hh:beginNum page="1" footnote="1" endnote="1" pic="1" tbl="1" equation="1"/><hh:refList>'
            f'<hh:fontfaces itemCnt="7">{fonts}</hh:fontfaces><hh:borderFills itemCnt="3">{"".join(borders)}</hh:borderFills>'
            f'<hh:charProperties itemCnt="16">{"".join(chars)}</hh:charProperties>'
            '<hh:tabProperties itemCnt="1"><hh:tabPr id="0" autoTabLeft="0" autoTabRight="0"/></hh:tabProperties>'
            f'<hh:paraProperties itemCnt="4">{"".join(paras)}</hh:paraProperties><hh:styles itemCnt="4">{styles}</hh:styles>'
            '</hh:refList><hh:compatibleDocument targetProgram="HWP201X"><hh:layoutCompatibility/></hh:compatibleDocument></hh:head>')


def _runs(runs: list[Run], level=0) -> str:
    out = []
    for run in runs:
        cid = level * 4 + int(run.bold) + 2 * int(run.italic)
        # hp:lineBreak의 tail에도 글자가 올 수 있으므로 XML 텍스트 순서를 보존한다.
        text = '<hp:lineBreak/>'.join(html.escape(t.replace("\t", " "), quote=False) for t in run.text.split("\n"))
        out.append(f'<hp:run charPrIDRef="{cid}"><hp:t>{text}</hp:t></hp:run>')
    return ''.join(out)


class _Section:
    def __init__(self):
        self.serial = 0

    def next_id(self):
        self.serial += 1
        return self.serial

    def paragraph(self, p: Paragraph, prefix="") -> str:
        runs = p.runs
        if p.marker:
            runs = [Run("  " * p.depth + p.marker + " "), *runs]
        return (f'<hp:p id="{self.next_id()}" paraPrIDRef="{p.level}" styleIDRef="{p.level}" pageBreak="0" columnBreak="0" merged="0">'
                f'{prefix}{_runs(runs, p.level)}</hp:p>')

    def table(self, table: Table) -> str:
        rows, cols = len(table.rows), len(table.rows[0])
        widths = [BODY_WIDTH // cols] * cols
        widths[-1] += BODY_WIDTH - sum(widths)
        body = []
        for r, row in enumerate(table.rows):
            cells = []
            for c, runs in enumerate(row):
                content = self.paragraph(Paragraph([Run(x.text, x.bold or r == 0, x.italic) for x in runs]))
                cells.append(f'<hp:tc name="" header="{int(r == 0)}" hasMargin="1" protect="0" editable="1" dirty="0" borderFillIDRef="{1 if r == 0 else 2}">'
                             f'<hp:subList id="" textDirection="HORIZONTAL" lineWrap="BREAK" vertAlign="CENTER" linkListIDRef="0" linkListNextIDRef="0" textWidth="0" textHeight="0" hasTextRef="0" hasNumRef="0">{content}</hp:subList>'
                             f'<hp:cellAddr rowAddr="{r}" colAddr="{c}"/><hp:cellSpan rowSpan="1" colSpan="1"/><hp:cellSz width="{widths[c]}" height="1800"/>'
                             '<hp:cellMargin left="140" right="140" top="90" bottom="90"/></hp:tc>')
            body.append('<hp:tr>' + ''.join(cells) + '</hp:tr>')
        tid = self.next_id()
        control = (f'<hp:tbl id="{tid}" zOrder="0" numberingType="TABLE" textWrap="TOP_AND_BOTTOM" textFlow="BOTH_SIDES" lock="0" dropcapstyle="None" rowCnt="{rows}" colCnt="{cols}" borderFillIDRef="2" repeatHeader="1" pageBreak="CELL" cellSpacing="0" noAdjust="0">'
                   f'<hp:sz width="{BODY_WIDTH}" height="{1800 * rows}" widthRelTo="ABSOLUTE" heightRelTo="ABSOLUTE" protect="0"/>'
                   '<hp:pos treatAsChar="1" affectLSpacing="0" flowWithText="1" allowOverlap="0" holdAnchorAndSO="0" vertRelTo="PARA" horzRelTo="COLUMN" vertAlign="TOP" horzAlign="LEFT" vertOffset="0" horzOffset="0"/>'
                   '<hp:outMargin left="0" right="0" top="0" bottom="0"/><hp:inMargin left="0" right="0" top="0" bottom="0"/>'
                   + ''.join(body) + '</hp:tbl>')
        return self.paragraph(Paragraph([Run("")]), f'<hp:run charPrIDRef="0">{control}</hp:run>')


def _section(doc: Document) -> str:
    builder = _Section()
    sec = ('<hp:run charPrIDRef="0"><hp:secPr id="" textDirection="HORIZONTAL" spaceColumns="1134" tabStop="8000" tabStopVal="4000" tabStopUnit="HWPUNIT" outlineShapeIDRef="0" memoShapeIDRef="0" textVerticalWidthHead="0" masterPageCnt="0">'
           '<hp:grid lineGrid="0" charGrid="0" wonggojiFormat="0"/><hp:startNum pageStartsOn="BOTH" page="0" pic="0" tbl="0" equation="0"/>'
           '<hp:visibility hideFirstHeader="0" hideFirstFooter="0" hideFirstMasterPage="0" border="SHOW_ALL" fill="SHOW_ALL" hideFirstPageNum="0" hideFirstEmptyLine="0" showLineNumber="0"/>'
           '<hp:pagePr landscape="WIDELY" width="59528" height="84188" gutterType="LEFT_ONLY"><hp:margin header="3600" footer="3600" gutter="0" left="5669" right="5670" top="3600" bottom="3600"/></hp:pagePr>'
           '</hp:secPr><hp:ctrl><hp:colPr id="" type="NEWSPAPER" layout="LEFT" colCount="1" sameSz="1" sameGap="0"/></hp:ctrl></hp:run>')
    out = [builder.paragraph(Paragraph([Run("")]), sec)]
    out.extend(builder.table(b) if isinstance(b, Table) else builder.paragraph(b) for b in doc.blocks)
    return XML + f'<hs:sec xmlns:hs="{HS}" xmlns:hp="{HP}" xmlns:hc="{HC}">' + ''.join(out) + '</hs:sec>'


def render_hwpx(source: str) -> tuple[bytes, Document]:
    doc = parse_markdown(source)
    parts = {
        "mimetype": "application/hwp+zip",
        "version.xml": XML + '<hv:HCFVersion xmlns:hv="http://www.hancom.co.kr/hwpml/2011/version" tagetApplication="WORDPROCESSOR" major="5" minor="0" micro="5" buildNumber="0" os="1" xmlVersion="1.4" application="DocFit Markdown" appVersion="1.0"/>',
        "META-INF/container.xml": XML + '<ocf:container xmlns:ocf="urn:oasis:names:tc:opendocument:xmlns:container"><ocf:rootfiles><ocf:rootfile full-path="Contents/content.hpf" media-type="application/hwpml-package+xml"/></ocf:rootfiles></ocf:container>',
        "META-INF/manifest.xml": XML + '<opf:manifest xmlns:opf="http://www.idpf.org/2007/opf/"><opf:item id="header" href="Contents/header.xml" media-type="application/xml"/><opf:item id="section0" href="Contents/section0.xml" media-type="application/xml"/></opf:manifest>',
        # 한/글의 OPF namespace는 끝의 /까지 필요하고 href는 ZIP 루트 기준이다.
        "Contents/content.hpf": XML + '<opf:package xmlns:opf="http://www.idpf.org/2007/opf/" version="3.0"><opf:metadata/><opf:manifest><opf:item id="header" href="Contents/header.xml" media-type="application/xml"/><opf:item id="section0" href="Contents/section0.xml" media-type="application/xml"/></opf:manifest><opf:spine><opf:itemref idref="header" linear="no"/><opf:itemref idref="section0" linear="yes"/></opf:spine></opf:package>',
        "Contents/header.xml": _header(),
        "Contents/section0.xml": _section(doc),
    }
    stream = io.BytesIO()
    with ZipFile(stream, "w") as archive:
        for name, content in parts.items():
            if name.endswith((".xml", ".hpf")):
                ET.fromstring(content)  # XML 파싱은 defusedxml만 사용한다.
            info = ZipInfo(name, (2026, 1, 1, 0, 0, 0))
            info.compress_type = ZIP_STORED if name == "mimetype" else ZIP_DEFLATED
            archive.writestr(info, content.encode("utf-8"))
    return stream.getvalue(), doc


def generate_hwpx(source: str, target: str | Path) -> GenerationResult:
    """문자열을 새 HWPX로 쓴다. 기존 파일·심볼릭 링크는 절대 덮어쓰지 않는다."""
    target = Path(target)
    if target.suffix.lower() != ".hwpx":
        raise ValueError("결과 확장자는 .hwpx여야 합니다.")
    data, doc = render_hwpx(source)
    # 기존 검사기는 경로 API다. 임시 패키지에서 검사한 뒤 최종 이름을 확보한다.
    with tempfile.TemporaryDirectory(prefix="docfit-markdown-") as folder:
        staged = Path(folder) / "generated.hwpx"
        staged.write_bytes(data)
        validate_hwpx(staged, structure_only=True, parse_xml=True)
    target.parent.mkdir(parents=True, exist_ok=True)
    created = False
    try:
        with target.open("xb") as stream:
            created = True
            stream.write(data)
    except BaseException:
        if created:
            target.unlink(missing_ok=True)
        raise
    return GenerationResult(target, tuple(doc.warnings),
                            sum(isinstance(b, Paragraph) for b in doc.blocks),
                            sum(isinstance(b, Table) for b in doc.blocks))
