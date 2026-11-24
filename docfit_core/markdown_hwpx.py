"""내부 Markdown → HWPX 생성기(공공 보고서 2단계, 앱 미연결).

annual/src/hwpx.rs의 ZIP·header/section·네이티브 표 골격을 Python으로
이식했다. 입력 해석 → XML 조립 → 검증 → 새 파일 게시 순으로 실행하며,
COM·kordoc·네트워크를 호출하지 않는다. 내어쓰기 실측은 후속 단계 몫이다.
"""
from __future__ import annotations

from dataclasses import dataclass, field
import html
import io
import ntpath
import re
import tempfile
import warnings as pywarnings
from pathlib import Path
from zipfile import ZipFile, ZipInfo, ZIP_DEFLATED, ZIP_STORED

from defusedxml import ElementTree as ET

from .hwpx import validate_hwpx
from .path_utils import path_is_within

BODY_WIDTH = 48_189  # annual과 같은 170mm(반올림)
HP = "http://www.hancom.co.kr/hwpml/2011/paragraph"
HH = "http://www.hancom.co.kr/hwpml/2011/head"
HC = "http://www.hancom.co.kr/hwpml/2011/core"
HS = "http://www.hancom.co.kr/hwpml/2011/section"
XML = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
MAX_INPUT_BYTES = 16 * 1024 * 1024
MAX_IMAGE_BYTES = 16 * 1024 * 1024
MAX_TOTAL_IMAGE_BYTES = 64 * 1024 * 1024
MAX_IMAGE_PIXELS = 40_000_000
MAX_IMAGES = 256
_HEADING = re.compile(r"^ {0,3}(#{1,6})\s+(.*?)(?:\s+#+)?\s*$")
_ITEM = re.compile(r"^(\s*)([□ㅇ※]|[-*+]|\d+[.)])[ \t]+(.*)$")
_SEP = re.compile(r"^:?-{3,}:?$")
_BR = re.compile(r"<br\s*/?>", re.I)


@dataclass(frozen=True)
class Image:
    data: bytes
    width: int
    height: int
    extension: str
    alt: str


@dataclass(frozen=True)
class Run:
    text: str
    bold: bool = False
    italic: bool = False
    image: Image | None = None
    code: bool = False


@dataclass
class Paragraph:
    runs: list[Run]
    level: int = 0
    marker: str = ""
    depth: int = 0
    code: bool = False


@dataclass
class Table:
    rows: list[list[list[Run]]]


@dataclass
class Document:
    blocks: list[Paragraph | Table] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)


class _Images:
    """한 번의 입력에만 유효한 그림 예산. 실패한 참조는 파서가 글자로 보존한다."""
    def __init__(self, directory):
        self.root = Path(directory).resolve() if directory is not None else None
        self.total = 0
        self.count = 0

    def read(self, destination: str, alt: str) -> Image:
        from PIL import Image as PILImage

        if self.root is None:
            raise ValueError("이미지 디렉터리가 지정되지 않았습니다")
        name = html.unescape(destination.strip())
        if name.startswith('<') and name.endswith('>'):
            name = name[1:-1]
        # URL, drive-relative, UNC, ADS 및 명시적 상위 이동을 모두 거절한다.
        if (not name or ntpath.splitdrive(name)[0] or name.startswith(('/', '\\'))
                or ':' in name or '..' in name.replace('\\', '/').split('/')):
            raise ValueError("안전하지 않은 상대 경로입니다")
        candidate = (self.root / name.replace('\\', '/')).resolve()
        if not path_is_within(candidate, self.root):
            raise ValueError("이미지 디렉터리를 이탈한 경로입니다")
        if candidate.suffix.lower() not in ('.png', '.jpg', '.jpeg'):
            raise ValueError("PNG/JPG 파일만 지원합니다")
        if not candidate.is_file():
            raise ValueError("그림 파일이 없습니다")
        # stat 이후 파일 크기가 바뀌더라도 상한+1까지만 읽는다.
        if candidate.stat().st_size > MAX_IMAGE_BYTES:
            raise ValueError("그림 파일이 16MiB를 초과합니다")
        with candidate.open('rb') as stream:
            data = stream.read(MAX_IMAGE_BYTES + 1)
        if len(data) > MAX_IMAGE_BYTES:
            raise ValueError("그림 파일이 16MiB를 초과합니다")
        if self.count >= MAX_IMAGES or self.total + len(data) > MAX_TOTAL_IMAGE_BYTES:
            raise ValueError("문서의 그림 자원 상한(256개/64MiB)을 초과합니다")
        with pywarnings.catch_warnings():
            pywarnings.simplefilter('error', PILImage.DecompressionBombWarning)
            with PILImage.open(io.BytesIO(data)) as image:
                if image.format not in ('PNG', 'JPEG'):
                    raise ValueError("PNG/JPG 내용이 아닙니다")
                width, height = image.size
                if width <= 0 or height <= 0 or width * height > MAX_IMAGE_PIXELS:
                    raise ValueError("그림 픽셀 상한(4천만)을 초과합니다")
                extension = 'png' if image.format == 'PNG' else 'jpg'
                if (candidate.suffix.lower() == '.png') != (extension == 'png'):
                    raise ValueError("그림 확장자와 내용이 다릅니다")
                image.verify()
            # 압축 데이터도 실제로 해독해 손상된 그림을 게시하지 않는다.
            with PILImage.open(io.BytesIO(data)) as image:
                image.load()
        self.count += 1
        self.total += len(data)
        return Image(data, width, height, extension, html.unescape(alt))


@dataclass(frozen=True)
class GenerationResult:
    path: Path
    warnings: tuple[str, ...]
    paragraphs: int
    tables: int


def _warn(warnings: list[str], message: str) -> None:
    if message not in warnings:
        warnings.append(message)


def _reference(text: str, start: int, *, image=False):
    """괄호가 들어간 로컬 파일/URL도 닫는 괄호까지 하나의 참조로 읽는다."""
    prefix = r'!\[' if image else r'\['
    match = re.match(prefix + r'((?:\\.|[^\]\\])*)\]\(', text[start:])
    if not match:
        return None
    begin = index = start + match.end()
    depth = 1
    angle = False
    while index < len(text):
        char = text[index]
        if char == '\\' and index + 1 < len(text):
            index += 2
            continue
        if char == '<':
            angle = True
        elif char == '>':
            angle = False
        elif not angle and char == '(':
            depth += 1
        elif not angle and char == ')':
            depth -= 1
            if depth == 0:
                destination = text[begin:index].strip()
                # 선택 title은 그림 경로의 일부가 아니다.
                title = re.fullmatch(r'''(<[^>]*>|\S+?)\s+(?:"[^"]*"|'[^']*')''', destination)
                if title:
                    destination = title[1]
                return match[1], destination, index + 1
        index += 1
    return None


def inline_runs(text: str, warnings: list[str], bold=False, italic=False,
                _depth=0, *, _images=None) -> list[Run]:
    """강조·코드·링크 레이블·줄바꿈과 허용 디렉터리의 그림을 읽는다."""
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
        image = _reference(text, i, image=True) if text[i] == '!' else None
        if image:
            try:
                resource = (_images or _Images(None)).read(image[1], image[0])
            except Exception as exc:
                # Pillow의 형식별 해독 예외도 일반 글자 폴백으로 처리한다.
                reason = str(exc) if isinstance(exc, ValueError) else type(exc).__name__
                _warn(warnings, f"그림 삽입 실패: {reason}. Markdown 참조를 글자로 보존했습니다.")
                buf.append(text[i:image[2]])
            else:
                flush(); result.append(Run('', bold, italic, image=resource))
            i = image[2]; continue
        link = _reference(text, i) if text[i] == '[' else None
        if link:
            _warn(warnings, "하이퍼링크 대상 미지원: 표시 글자만 보존했습니다.")
            flush()
            result.extend(inline_runs(link[0], warnings, bold, italic, _depth + 1, _images=_images))
            i = link[2]; continue
        if text[i] == "`":
            ticks = re.match(r"`+", text[i:]).group()
            end = text.find(ticks, i + len(ticks))
            if end >= 0:
                flush(); result.append(Run(text[i + len(ticks):end], bold, italic, code=True))
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
                                     bold or len(delim) >= 2, italic or len(delim) != 2, _depth + 1, _images=_images))
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


def parse_markdown(source: str, *, image_dir: str | Path | None = None) -> Document:
    if not isinstance(source, str):
        raise TypeError("Markdown 입력은 문자열이어야 합니다.")
    if len(source.encode("utf-8")) > MAX_INPUT_BYTES:
        raise ValueError("Markdown 입력은 16MB 이하여야 합니다.")
    if any(not (c in "\t\n\r" or 0x20 <= ord(c) <= 0xD7FF or
                0xE000 <= ord(c) <= 0xFFFD or 0x10000 <= ord(c) <= 0x10FFFF) for c in source):
        raise ValueError("XML에 쓸 수 없는 제어 문자가 있습니다.")
    doc = Document()
    images = _Images(image_dir)

    def inline(text):
        return inline_runs(text, doc.warnings, _images=images)
    lines = source.lstrip("\ufeff").replace("\r\n", "\n").replace("\r", "\n").split("\n")
    pending: list[str] = []
    indents: list[int] = []

    def flush():
        if pending:
            # 원문의 줄바꿈(soft/hard)을 모두 보존한다. 문단 경계는 빈 줄이다.
            text = "\n".join(re.sub(r"(?: {2,}|\\)$", "", s) for s in pending)
            doc.blocks.append(Paragraph(inline(text)))
            pending.clear()

    i = 0
    while i < len(lines):
        line = lines[i]
        if not line.strip():
            flush(); indents.clear(); i += 1; continue
        fence = re.match(r"^\s*(`{3,}|~{3,})", line)
        if fence:
            flush(); indents.clear()
            token = fence[1]; content = []; i += 1
            while i < len(lines) and not re.match(r"^\s*" + re.escape(token[0]) + "{" + str(len(token)) + r",}\s*$", lines[i]):
                content.append(lines[i]); i += 1
            doc.blocks.append(Paragraph([Run("\n".join(content), code=True)], code=True)); i += 1; continue
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
                doc.blocks.append(Table([[inline(c) for c in row] for row in rows]))
                continue
        head = _HEADING.match(line)
        if head:
            flush(); indents.clear()
            level = len(head[1])
            doc.blocks.append(Paragraph(inline(head[2]), level=level))
            i += 1; continue
        item = _ITEM.match(line)
        if item:
            flush()
            width = len(item[1].expandtabs(4))
            while indents and indents[-1] >= width:
                indents.pop()
            indents.append(width)
            marker = "-" if item[2] in ("*", "+") else item[2]
            doc.blocks.append(Paragraph(inline(item[3].lstrip()),
                                        marker=marker, depth=len(indents) - 1))
            i += 1; continue
        # 목록의 들여쓴 후속 줄은 같은 항목의 줄바꿈으로 보존한다.
        if line.startswith((" ", "\t")) and doc.blocks and isinstance(doc.blocks[-1], Paragraph) and doc.blocks[-1].marker:
            doc.blocks[-1].runs.extend([Run("\n"), *inline(line.strip())])
        else:
            indents.clear(); pending.append(line.replace("\t", " "))
        i += 1
    flush()
    if not doc.blocks:
        doc.blocks.append(Paragraph([Run("")]))
    return doc


def _attrs(**values) -> str:
    return " ".join(f'{k}="{html.escape(str(v), quote=True)}"' for k, v in values.items())


def _header(has_code=False) -> str:
    langs = ("HANGUL", "LATIN", "HANJA", "JAPANESE", "OTHER", "SYMBOL", "USER")
    scripts = ("hangul", "latin", "hanja", "japanese", "other", "symbol", "user")
    mono = '<hh:font id="2" face="굴림체" type="TTF" isEmbedded="0"/>' if has_code else ''
    groups = 8 if has_code else 7  # 0=본문, 1~6=제목, 7=코드
    fonts = ''.join(f'<hh:fontface lang="{lang}" fontCnt="{3 if has_code else 2}">'
                    '<hh:font id="0" face="휴먼명조" type="TTF" isEmbedded="0"/>'
                    f'<hh:font id="1" face="한컴돋움" type="TTF" isEmbedded="0"/>{mono}</hh:fontface>' for lang in langs)
    borders = []
    for bid, color in ((1, "#E7EEF8"), (2, "#FFFFFF"), (3, None)):
        edges = ''.join(f'<hh:{edge}Border type="{"SOLID" if color else "NONE"}" width="0.12 mm" color="#AAB7C6"/>'
                        for edge in ("left", "right", "top", "bottom"))
        fill = f'<hc:fillBrush><hc:winBrush faceColor="{color}" hatchColor="#000000" alpha="0"/></hc:fillBrush>' if color else ''
        borders.append(f'<hh:borderFill id="{bid}" threeD="0" shadow="0" centerLine="NONE" breakCellSeparateLine="0">'
                       f'<hh:slash type="NONE" Crooked="0" isCounter="0"/><hh:backSlash type="NONE" Crooked="0" isCounter="0"/>{edges}{fill}</hh:borderFill>')
    chars = []
    for level in range(groups):
        for flags in range(4):
            props = ''.join(f'<hh:{tag} {_attrs(**dict.fromkeys(scripts, value))}/>' for tag, value in
                            (("fontRef", 2 if level == 7 else int(level > 0)), ("ratio", 100), ("spacing", 0), ("relSz", 100), ("offset", 0)))
            emphasis = ('<hh:bold/>' if flags & 1 or 1 <= level <= 6 else '') + ('<hh:italic/>' if flags & 2 else '')
            height = (1100, 1600, 1600, 1600, 1400, 1200, 1100, 1000)[level]
            chars.append(f'<hh:charPr id="{level * 4 + flags}" height="{height}" textColor="#202020" shadeColor="none" useFontSpace="0" useKerning="0" symMark="NONE" borderFillIDRef="3">'
                         f'{props}{emphasis}<hh:underline type="NONE" shape="SOLID" color="#000000"/>'
                         '<hh:strikeout shape="NONE" color="#000000"/><hh:outline type="NONE"/><hh:shadow type="NONE" color="#B2B2B2" offsetX="10" offsetY="10"/></hh:charPr>')
    paras = []
    for level in range(groups):
        margins = ''.join(f'<hc:{key} value="{value}" unit="HWPUNIT"/>' for key, value in
                          (("intent", 0), ("left", 0), ("right", 0), ("prev", 300 if level else 0), ("next", 300 if level else 0)))
        paras.append(f'<hh:paraPr id="{level}" tabPrIDRef="0" condense="0" fontLineHeight="0" snapToGrid="0" suppressLineNumbers="0" checked="0" textDir="AUTO">'
                     f'<hh:align horizontal="{"LEFT" if level else "JUSTIFY"}" vertical="BASELINE"/>'
                     '<hh:heading type="NONE" idRef="0" level="0"/>'
                     f'<hh:breakSetting breakLatinWord="KEEP_WORD" breakNonLatinWord="BREAK_WORD" widowOrphan="1" keepWithNext="{int(1 <= level <= 6)}" keepLines="0" pageBreakBefore="0" lineWrap="BREAK"/>'
                     f'<hh:autoSpacing eAsianEng="0" eAsianNum="0"/><hh:margin>{margins}</hh:margin><hh:lineSpacing type="PERCENT" value="160"/>'
                     '<hh:border borderFillIDRef="3" offsetLeft="0" offsetRight="0" offsetTop="0" offsetBottom="0" connect="0" ignoreMargin="0"/></hh:paraPr>')
    styles = ''.join(f'<hh:style id="{level}" type="PARA" name="{"코드" if level == 7 else ("제목 " + str(level)) if level else "바탕글"}" engName="{"Code" if level == 7 else ("Heading " + str(level)) if level else "Normal"}" paraPrIDRef="{level}" charPrIDRef="{level * 4}" nextStyleIDRef="0" langID="1042" lockForm="0"/>' for level in range(groups))
    return (XML + f'<hh:head xmlns:hh="{HH}" xmlns:hc="{HC}" version="1.5" secCnt="1">'
            '<hh:beginNum page="1" footnote="1" endnote="1" pic="1" tbl="1" equation="1"/><hh:refList>'
            f'<hh:fontfaces itemCnt="7">{fonts}</hh:fontfaces><hh:borderFills itemCnt="3">{"".join(borders)}</hh:borderFills>'
            f'<hh:charProperties itemCnt="{groups * 4}">{"".join(chars)}</hh:charProperties>'
            '<hh:tabProperties itemCnt="1"><hh:tabPr id="0" autoTabLeft="0" autoTabRight="0"/></hh:tabProperties>'
            f'<hh:paraProperties itemCnt="{groups}">{"".join(paras)}</hh:paraProperties><hh:styles itemCnt="{groups}">{styles}</hh:styles>'
            '</hh:refList><hh:compatibleDocument targetProgram="HWP201X"><hh:layoutCompatibility/></hh:compatibleDocument></hh:head>')


def _runs(runs: list[Run], level=0, *, picture=None, width=BODY_WIDTH) -> str:
    out = []
    for run in runs:
        cid = (7 if run.code else level) * 4 + int(run.bold) + 2 * int(run.italic)
        if run.image is not None:
            out.append(f'<hp:run charPrIDRef="{cid}">{picture(run.image, width)}</hp:run>')
            continue
        # hp:lineBreak의 tail에도 글자가 올 수 있으므로 XML 텍스트 순서를 보존한다.
        text = '<hp:lineBreak/>'.join(html.escape(t.expandtabs(4), quote=False) for t in run.text.split("\n"))
        out.append(f'<hp:run charPrIDRef="{cid}"><hp:t>{text}</hp:t></hp:run>')
    return ''.join(out)


class _Section:
    def __init__(self):
        self.serial = 0
        self.images: list[Image] = []

    def picture(self, image: Image, available_width: int) -> str:
        self.images.append(image)
        bid = f'image{len(self.images)}'
        pid = self.next_id()
        # 픽셀은 96dpi로 해석하고 본문/칸 폭과 한 쪽 높이 안에 비례 축소한다.
        scale = min(75, available_width / image.width, 70_000 / image.height)
        width, height = max(1, round(image.width * scale)), max(1, round(image.height * scale))
        matrix = ''.join(f'<hc:{kind} e1="1" e2="0" e3="0" e4="0" e5="1" e6="0"/>'
                         for kind in ('transMatrix', 'scaMatrix', 'rotMatrix'))
        points = ''.join(f'<hc:pt{n} x="{x}" y="{y}"/>' for n, (x, y) in
                         enumerate(((0, 0), (width, 0), (width, height), (0, height))))
        return (f'<hp:pic id="{pid}" zOrder="0" numberingType="PICTURE" textWrap="TOP_AND_BOTTOM" textFlow="BOTH_SIDES" lock="0" dropcapstyle="None" href="" groupLevel="0" instid="{pid}" reverse="0">'
                f'<hp:offset x="0" y="0"/><hp:orgSz width="{width}" height="{height}"/><hp:curSz width="{width}" height="{height}"/>'
                f'<hp:flip horizontal="0" vertical="0"/><hp:rotationInfo angle="0" centerX="{width // 2}" centerY="{height // 2}" rotateimage="1"/>'
                f'<hp:renderingInfo>{matrix}</hp:renderingInfo><hp:imgRect>{points}</hp:imgRect>'
                f'<hp:imgClip left="0" right="{width}" top="0" bottom="{height}"/><hp:inMargin left="0" right="0" top="0" bottom="0"/>'
                f'<hp:imgDim dimwidth="{width}" dimheight="{height}"/><hc:img binaryItemIDRef="{bid}" bright="0" contrast="0" effect="REAL_PIC" alpha="0"/><hp:effects/>'
                f'<hp:sz width="{width}" height="{height}" widthRelTo="ABSOLUTE" heightRelTo="ABSOLUTE" protect="0"/>'
                '<hp:pos treatAsChar="1" affectLSpacing="0" flowWithText="1" allowOverlap="0" holdAnchorAndSO="0" vertRelTo="PARA" horzRelTo="COLUMN" vertAlign="TOP" horzAlign="LEFT" vertOffset="0" horzOffset="0"/>'
                f'<hp:outMargin left="0" right="0" top="0" bottom="0"/><hp:shapeComment>{html.escape(image.alt)}</hp:shapeComment></hp:pic>')

    def next_id(self):
        self.serial += 1
        return self.serial

    def paragraph(self, p: Paragraph, prefix="", width=BODY_WIDTH) -> str:
        runs = p.runs
        if p.marker:
            runs = [Run("  " * p.depth + p.marker + " "), *runs]
        level = 7 if p.code else p.level
        return (f'<hp:p id="{self.next_id()}" paraPrIDRef="{level}" styleIDRef="{level}" pageBreak="0" columnBreak="0" merged="0">'
                f'{prefix}{_runs(runs, level, picture=self.picture, width=width)}</hp:p>')

    def table(self, table: Table) -> str:
        rows, cols = len(table.rows), len(table.rows[0])
        widths = [BODY_WIDTH // cols] * cols
        widths[-1] += BODY_WIDTH - sum(widths)
        body = []
        for r, row in enumerate(table.rows):
            cells = []
            for c, runs in enumerate(row):
                content = self.paragraph(Paragraph([Run(x.text, x.bold or r == 0, x.italic, x.image, x.code) for x in runs]), width=widths[c] - 280)
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


def _section(doc: Document, builder: _Section) -> str:
    sec = ('<hp:run charPrIDRef="0"><hp:secPr id="" textDirection="HORIZONTAL" spaceColumns="1134" tabStop="8000" tabStopVal="4000" tabStopUnit="HWPUNIT" outlineShapeIDRef="0" memoShapeIDRef="0" textVerticalWidthHead="0" masterPageCnt="0">'
           '<hp:grid lineGrid="0" charGrid="0" wonggojiFormat="0"/><hp:startNum pageStartsOn="BOTH" page="0" pic="0" tbl="0" equation="0"/>'
           '<hp:visibility hideFirstHeader="0" hideFirstFooter="0" hideFirstMasterPage="0" border="SHOW_ALL" fill="SHOW_ALL" hideFirstPageNum="0" hideFirstEmptyLine="0" showLineNumber="0"/>'
           '<hp:pagePr landscape="WIDELY" width="59528" height="84188" gutterType="LEFT_ONLY"><hp:margin header="3600" footer="3600" gutter="0" left="5669" right="5670" top="3600" bottom="3600"/></hp:pagePr>'
           '</hp:secPr><hp:ctrl><hp:colPr id="" type="NEWSPAPER" layout="LEFT" colCount="1" sameSz="1" sameGap="0"/></hp:ctrl></hp:run>')
    out = [builder.paragraph(Paragraph([Run("")]), sec)]
    out.extend(builder.table(b) if isinstance(b, Table) else builder.paragraph(b) for b in doc.blocks)
    return XML + f'<hs:sec xmlns:hs="{HS}" xmlns:hp="{HP}" xmlns:hc="{HC}">' + ''.join(out) + '</hs:sec>'


def render_hwpx(source: str, *, image_dir: str | Path | None = None) -> tuple[bytes, Document]:
    doc = parse_markdown(source, image_dir=image_dir)
    builder = _Section()
    section = _section(doc, builder)
    has_code = any(r.code for b in doc.blocks for r in
                   (b.runs if isinstance(b, Paragraph) else [r for row in b.rows for cell in row for r in cell]))
    parts = {
        "mimetype": "application/hwp+zip",
        "version.xml": XML + '<hv:HCFVersion xmlns:hv="http://www.hancom.co.kr/hwpml/2011/version" tagetApplication="WORDPROCESSOR" major="5" minor="0" micro="5" buildNumber="0" os="1" xmlVersion="1.4" application="DocFit Markdown" appVersion="1.0"/>',
        "META-INF/container.xml": XML + '<ocf:container xmlns:ocf="urn:oasis:names:tc:opendocument:xmlns:container"><ocf:rootfiles><ocf:rootfile full-path="Contents/content.hpf" media-type="application/hwpml-package+xml"/></ocf:rootfiles></ocf:container>',
        "META-INF/manifest.xml": XML + '<opf:manifest xmlns:opf="http://www.idpf.org/2007/opf/"><opf:item id="header" href="Contents/header.xml" media-type="application/xml"/><opf:item id="section0" href="Contents/section0.xml" media-type="application/xml"/></opf:manifest>',
        # 한/글의 OPF namespace는 끝의 /까지 필요하고 href는 ZIP 루트 기준이다.
        "Contents/content.hpf": XML + '<opf:package xmlns:opf="http://www.idpf.org/2007/opf/" version="3.0"><opf:metadata/><opf:manifest><opf:item id="header" href="Contents/header.xml" media-type="application/xml"/><opf:item id="section0" href="Contents/section0.xml" media-type="application/xml"/></opf:manifest><opf:spine><opf:itemref idref="header" linear="no"/><opf:itemref idref="section0" linear="yes"/></opf:spine></opf:package>',
        "Contents/header.xml": _header(has_code),
        "Contents/section0.xml": section,
    }
    items = ''
    for n, image in enumerate(builder.images, 1):
        name = f'BinData/image{n}.{image.extension}'
        parts[name] = image.data
        items += f'<opf:item id="image{n}" href="{name}" media-type="image/{"png" if image.extension == "png" else "jpeg"}" isEmbeded="1"/>'
    for name in ('Contents/content.hpf', 'META-INF/manifest.xml'):
        parts[name] = parts[name].replace('</opf:manifest>', items + '</opf:manifest>')
    stream = io.BytesIO()
    with ZipFile(stream, "w") as archive:
        for name, content in parts.items():
            if name.endswith((".xml", ".hpf")):
                ET.fromstring(content)  # XML 파싱은 defusedxml만 사용한다.
            info = ZipInfo(name, (2026, 1, 1, 0, 0, 0))
            info.compress_type = ZIP_STORED if name == "mimetype" else ZIP_DEFLATED
            archive.writestr(info, content.encode("utf-8") if isinstance(content, str) else content)
    return stream.getvalue(), doc


def generate_hwpx(source: str, target: str | Path, *, image_dir: str | Path | None = None) -> GenerationResult:
    """문자열을 새 HWPX로 쓴다. 기존 파일·심볼릭 링크는 절대 덮어쓰지 않는다."""
    target = Path(target)
    if target.suffix.lower() != ".hwpx":
        raise ValueError("결과 확장자는 .hwpx여야 합니다.")
    data, doc = render_hwpx(source, image_dir=image_dir)
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
