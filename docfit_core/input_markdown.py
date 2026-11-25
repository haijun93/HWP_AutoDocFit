"""의존성 추가 없는 선택형 입력 어댑터. 지원하지 않는 의미 요소는 경고한다.

Office 압축은 기존 안전 검사로 읽기만 한다. 그림은 관계로 지정된 패키지 내부
PNG/JPEG만 별도 작업 디렉터리에 복사하며 외부 URL은 읽지 않는다.
"""
from __future__ import annotations

from dataclasses import dataclass
from html.parser import HTMLParser
import posixpath
from pathlib import Path
import re

from defusedxml import ElementTree as ET

from docfit_core.timemachine.extractors_ooxml import _open_zip_safely

W = '{http://schemas.openxmlformats.org/wordprocessingml/2006/main}'
A = '{http://schemas.openxmlformats.org/drawingml/2006/main}'
P = '{http://schemas.openxmlformats.org/presentationml/2006/main}'
S = '{http://schemas.openxmlformats.org/spreadsheetml/2006/main}'
R = '{http://schemas.openxmlformats.org/officeDocument/2006/relationships}'


@dataclass(frozen=True)
class AdaptedMarkdown:
    markdown: str
    image_dir: Path
    warnings: tuple[str, ...]


class UnsupportedInput(ValueError):
    pass


def escape(text):
    text = text.replace('&', '&amp;').replace('<', '&lt;').replace('>', '&gt;')
    return re.sub(r'([\\`*_{}\[\]()#+.!|~\-])', r'\\\1', text).replace('\r\n', '\n').replace('\n', '<br>')


def table(rows):
    if not rows or not rows[0]:
        return ''
    width = max(map(len, rows))
    lines = ['| ' + ' | '.join([*r, *([''] * (width - len(r)))]) + ' |' for r in rows]
    lines.insert(1, '|' + '---|' * width)
    return '\n'.join(lines)


class Package:
    def __init__(self, source, assets, warnings):
        self.archive = _open_zip_safely(source)
        self.assets, self.warnings = assets, warnings
        self.images = 0

    def xml(self, part):
        return ET.fromstring(self.archive.read(part))

    def relationships(self, part):
        folder, name = posixpath.split(part)
        rel = posixpath.join(folder, '_rels', name + '.rels')
        if rel not in self.archive.namelist():
            return {}
        return {n.get('Id'): n for n in self.xml(rel)}

    def target(self, part, relation):
        if relation is None or relation.get('TargetMode') == 'External':
            raise UnsupportedInput('외부/누락 관계는 지원하지 않습니다')
        target = relation.get('Target', '').replace('\\', '/')
        if ':' in target:
            raise UnsupportedInput('외부 관계 경로')
        target = posixpath.normpath(target.lstrip('/') if target.startswith('/') else posixpath.join(posixpath.dirname(part), target))
        if target.startswith('../') or target not in self.archive.namelist():
            raise UnsupportedInput('패키지 밖/누락 관계 경로')
        return target

    def picture(self, part, rid, alt=''):
        try:
            member = self.target(part, self.relationships(part).get(rid))
            suffix = Path(member).suffix.lower()
            if suffix not in ('.png', '.jpg', '.jpeg'):
                raise UnsupportedInput('PNG/JPEG 외 그림')
            data = self.archive.read(member)
            if len(data) > 16 * 1024 * 1024 or self.images >= 256:
                raise UnsupportedInput('그림 자원 상한 초과')
            self.images += 1
            name = f'image{self.images}{suffix}'
            (self.assets / name).write_bytes(data)
            return f'![{escape(alt)}]({name})'
        except (ValueError, KeyError) as exc:
            self.warnings.append(f'그림 미지원: {exc}')
            return ''

    def check(self, root, names):
        present = {n.tag.rsplit('}', 1)[-1] for n in root.iter()}
        for name in sorted(present & set(names)):
            self.warnings.append(f'미지원 요소: {name}')


def docx(package):
    part = 'word/document.xml'
    root = package.xml(part)
    package.check(root, ('gridSpan', 'vMerge', 'altChunk', 'object', 'footnoteReference', 'endnoteReference',
                         'ins', 'del', 'hyperlink', 'fldChar', 'txbxContent', 'sym'))
    for rel in package.relationships(part).values():
        if rel.get('Type', '').rsplit('/', 1)[-1] in ('header', 'footer'):
            package.warnings.append('미지원 요소: 머리글/바닥글')
    numbering = {}
    if 'word/numbering.xml' in package.archive.namelist():
        nums = package.xml('word/numbering.xml')
        abstracts = {n.get(W + 'abstractNumId'): n for n in nums.findall(W + 'abstractNum')}
        for num in nums.findall(W + 'num'):
            ref = num.find(W + 'abstractNumId')
            abstract = abstracts.get(ref.get(W + 'val')) if ref is not None else None
            if abstract is not None:
                for lvl in abstract.findall(W + 'lvl'):
                    fmt = lvl.find(W + 'numFmt')
                    start = lvl.find(W + 'start')
                    numbering[(num.get(W + 'numId'), lvl.get(W + 'ilvl'))] = (
                        fmt.get(W + 'val') if fmt is not None else 'bullet',
                        int(start.get(W + 'val', '1')) if start is not None else 1)
            if num.find(W + 'lvlOverride') is not None:
                package.warnings.append('미지원 요소: 번호 시작값 재정의')
    counters = {}

    def paragraph(p, in_cell=False):
        runs = []
        for run in p.findall('.//' + W + 'r'):
            text = ''.join(escape(n.text or '') if n.tag == W + 't' else '<br>' if n.tag in (W + 'br', W + 'cr') else ' ' if n.tag == W + 'tab' else '' for n in run)
            props = run.find(W + 'rPr')
            if props is not None and text:
                for tag, mark in (('i', '*'), ('b', '**')):
                    node = props.find(W + tag)
                    if node is not None and node.get(W + 'val', '1') not in ('0', 'false', 'off'):
                        text = mark + text + mark
            for blip in run.findall('.//' + A + 'blip'):
                if blip.get(R + 'embed'):
                    text += package.picture(part, blip.get(R + 'embed'))
                else:
                    package.warnings.append('미지원 요소: 외부 그림')
            runs.append(text)
        text = ''.join(runs)
        if in_cell:
            return text
        props = p.find(W + 'pPr')
        if props is not None:
            style = props.find(W + 'pStyle')
            heading = re.fullmatch(r'Heading([1-6])', style.get(W + 'val', ''), re.I) if style is not None else None
            outline = props.find(W + 'outlineLvl')
            if heading:
                return '#' * int(heading[1]) + ' ' + text
            if outline is not None and int(outline.get(W + 'val', '9')) < 6:
                return '#' * (int(outline.get(W + 'val')) + 1) + ' ' + text
            num = props.find(W + 'numPr')
            if num is not None:
                rid, level = num.find(W + 'numId'), num.find(W + 'ilvl')
                key = (rid.get(W + 'val') if rid is not None else '', level.get(W + 'val', '0') if level is not None else '0')
                fmt, start = numbering.get(key, ('unknown', 1))
                if fmt not in ('bullet', 'decimal'):
                    package.warnings.append(f'미지원 번호 형식: {fmt}')
                depth = int(key[1])
                counters[key] = counters.get(key, start - 1) + 1
                for deeper in [k for k in counters if k[0] == key[0] and int(k[1]) > depth]:
                    del counters[deeper]
                marker = '-' if fmt == 'bullet' else f'{counters[key]}.'
                return '  ' * depth + marker + ' ' + text
        return text

    out = []
    for node in root.find(W + 'body'):
        if node.tag == W + 'p':
            out.append(paragraph(node))
        elif node.tag == W + 'tbl':
            if node.findall('.//' + W + 'tbl'):
                package.warnings.append('미지원 요소: 중첩 표')
            rows = [['<br>'.join(paragraph(p, True) for p in cell.findall(W + 'p'))
                     for cell in row.findall(W + 'tc')] for row in node.findall(W + 'tr')]
            out.append(table(rows))
    return '\n\n'.join(out)


def xlsx(package):
    workbook = 'xl/workbook.xml'
    root = package.xml(workbook)
    shared = []
    if 'xl/sharedStrings.xml' in package.archive.namelist():
        shared = [''.join(n.text or '' for n in item.iter(S + 't')) for item in package.xml('xl/sharedStrings.xml')]
    out = []
    for sheet in root.findall(S + 'sheets/' + S + 'sheet'):
        part = package.target(workbook, package.relationships(workbook).get(sheet.get(R + 'id')))
        body = package.xml(part)
        package.check(body, ('mergeCell', 'drawing', 'legacyDrawing', 'hyperlink', 'f', 'conditionalFormatting'))
        if sheet.get('state', 'visible') != 'visible':
            package.warnings.append('미지원 요소: 숨김 시트')
        cells, max_row, max_col = {}, 0, 0
        for row in body.findall(S + 'sheetData/' + S + 'row'):
            if row.get('hidden') == '1':
                package.warnings.append('미지원 요소: 숨김 행')
            for cell in row.findall(S + 'c'):
                match = re.fullmatch(r'([A-Z]+)([1-9]\d*)', cell.get('r', ''))
                if not match:
                    raise UnsupportedInput('셀 주소 누락/오류')
                col = 0
                for char in match[1]:
                    col = col * 26 + ord(char) - 64
                r = int(match[2])
                if col > 256 or r > 10000 or col * r > 100000:
                    raise UnsupportedInput('표 변환 범위 상한 초과')
                value = cell.findtext(S + 'v', '')
                if cell.get('t') == 's':
                    value = shared[int(value)]
                elif cell.get('t') == 'inlineStr':
                    value = ''.join(n.text or '' for n in cell.iter(S + 't'))
                elif cell.get('t') == 'b':
                    value = 'TRUE' if value == '1' else 'FALSE'
                if cell.get('s') not in (None, '0'):
                    package.warnings.append('미지원 요소: 숫자/날짜 표시 서식')
                cells[(r, col)] = escape(value)
                max_row, max_col = max(max_row, r), max(max_col, col)
        if any(c.get('hidden') == '1' for c in body.findall(S + 'cols/' + S + 'col')):
            package.warnings.append('미지원 요소: 숨김 열')
        rows = [[cells.get((r, c), '') for c in range(1, max_col + 1)] for r in range(1, max_row + 1)]
        if rows:
            out.append(table(rows))
    return '\n\n'.join(out)


def pptx(package):
    pres = 'ppt/presentation.xml'
    root = package.xml(pres)
    out = []
    for slide in root.findall(P + 'sldIdLst/' + P + 'sldId'):
        part = package.target(pres, package.relationships(pres).get(slide.get(R + 'id')))
        body = package.xml(part)
        package.check(body, ('grpSp', 'chart', 'graphicDataUnknown', 'oleObj', 'hMerge', 'vMerge', 'gridSpan', 'rowSpan', 'hlinkClick'))
        tree = body.find(P + 'cSld/' + P + 'spTree')
        for node in tree:
            if node.tag == P + 'sp':
                placeholder = node.find('.//' + P + 'ph')
                title = placeholder is not None and placeholder.get('type') in ('title', 'ctrTitle')
                for para in node.findall('.//' + A + 'p'):
                    text = ''.join(escape(n.text or '') if n.tag == A + 't' else '<br>' if n.tag == A + 'br' else '' for n in para.iter())
                    if not text:
                        continue
                    props = para.find(A + 'pPr')
                    depth = int(props.get('lvl', '0')) if props is not None else 0
                    bullet = placeholder is not None and placeholder.get('type') == 'body'
                    bullet = bullet or (props is not None and any(props.find(A + t) is not None for t in ('buChar', 'buAutoNum')))
                    ordered = props is not None and props.find(A + 'buAutoNum') is not None
                    marker = '1.' if ordered else '-'
                    if ordered:
                        package.warnings.append('미지원 요소: PPT 자동 번호 시작/연속')
                    out.append('# ' + text if title else '  ' * depth + marker + ' ' + text if bullet else text)
            elif node.tag == P + 'graphicFrame':
                tbl = node.find('.//' + A + 'tbl')
                if tbl is None:
                    package.warnings.append('미지원 요소: 슬라이드 그래픽')
                    continue
                rows = [['<br>'.join(''.join(escape(n.text or '') for n in p.iter(A + 't')) for p in cell.findall('.//' + A + 'p'))
                         for cell in row.findall(A + 'tc')] for row in tbl.findall(A + 'tr')]
                out.append(table(rows))
            elif node.tag == P + 'pic':
                blip = node.find('.//' + A + 'blip')
                if blip is not None:
                    out.append(package.picture(part, blip.get(R + 'embed')))
    return '\n\n'.join(out)


class HtmlMarkdown(HTMLParser):
    """의미 HTML의 제목·목록·표·로컬 그림. CSS/병합/스크립트는 손실 경고."""
    def __init__(self, source, assets):
        super().__init__(convert_charrefs=True)
        self.source, self.assets = source, assets
        self.out, self.warnings, self.lists = [], [], []
        self.rows, self.row, self.cell = None, None, None
        self.skip = 0
        self.images = 0

    def emit(self, value):
        if self.cell is not None:
            self.cell.append(value)
        else:
            self.out.append(value)

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if tag in ('head', 'script', 'style'):
            if tag != 'head':
                self.warnings.append(f'미지원 HTML: {tag}')
            self.skip += 1
        if self.skip:
            return
        if any(k in attrs for k in ('style', 'rowspan', 'colspan')):
            self.warnings.append('미지원 HTML: CSS/병합 셀')
        if re.fullmatch('h[1-6]', tag):
            self.emit('\n\n' + '#' * int(tag[1]) + ' ')
        elif tag in ('p', 'div', 'section'):
            self.emit('\n\n')
        elif tag == 'br':
            self.emit('<br>')
        elif tag in ('ul', 'ol'):
            self.lists.append([tag, int(attrs.get('start', '1')) - 1])
        elif tag == 'li':
            if not self.lists:
                self.warnings.append('미지원 HTML: 목록 밖 li')
                return
            self.lists[-1][1] += 1
            marker = '-' if self.lists[-1][0] == 'ul' else f'{self.lists[-1][1]}.'
            self.emit('\n' + '  ' * (len(self.lists) - 1) + marker + ' ')
        elif tag in ('b', 'strong', 'em', 'i'):
            self.emit('**' if tag in ('b', 'strong') else '*')
        elif tag == 'a':
            self.warnings.append('미지원 HTML: 링크 대상')
        elif tag == 'table':
            if self.rows is not None:
                raise UnsupportedInput('중첩 HTML 표')
            self.rows = []
        elif tag == 'tr':
            self.row = []
        elif tag in ('td', 'th'):
            self.cell = []
        elif tag == 'img':
            try:
                name = attrs.get('src', '')
                if not name or ':' in name or '\\' in name or name.startswith('/') or '..' in Path(name).parts:
                    raise UnsupportedInput('외부/상위 경로 그림')
                path = (self.source.parent / name).resolve()
                if not path.is_relative_to(self.source.parent.resolve()) or not path.is_file():
                    raise UnsupportedInput('누락/이탈 그림')
                if path.suffix.lower() not in ('.png', '.jpg', '.jpeg') or path.stat().st_size > 16 * 1024 * 1024 or self.images >= 256:
                    raise UnsupportedInput('그림 형식/상한')
                self.images += 1
                target = self.assets / f'image{self.images}{path.suffix.lower()}'
                target.write_bytes(path.read_bytes())
                self.emit(f'![{escape(attrs.get("alt", ""))}]({target.name})')
            except (ValueError, OSError) as exc:
                self.warnings.append(f'미지원 HTML 그림: {exc}')

    def handle_endtag(self, tag):
        if tag in ('head', 'script', 'style') and self.skip:
            self.skip -= 1
            return
        if self.skip:
            return
        if tag in ('ul', 'ol') and self.lists:
            self.lists.pop()
            if not self.lists:
                self.emit('\n\n')
        elif tag in ('td', 'th') and self.cell is not None:
            self.row.append(''.join(self.cell).strip())
            self.cell = None
        elif tag == 'tr' and self.rows is not None:
            self.rows.append(self.row)
            self.row = None
        elif tag == 'table' and self.rows is not None:
            self.emit('\n\n' + table(self.rows) + '\n\n')
            self.rows = None
        elif tag in ('b', 'strong', 'em', 'i'):
            self.emit('**' if tag in ('b', 'strong') else '*')
        elif tag in ('p', 'div', 'section') or re.fullmatch('h[1-6]', tag):
            self.emit('\n\n')

    def handle_data(self, data):
        if not self.skip:
            self.emit(escape(re.sub(r'\s+', ' ', data)))


def adapt_to_markdown(source: str | Path, image_dir: str | Path) -> AdaptedMarkdown:
    """source를 읽기만 하고, 존재하지 않는 작업 디렉터리에 그림 사본을 만든다."""
    source, assets = Path(source), Path(image_dir)
    if source.is_symlink() or not source.is_file():
        raise ValueError('입력은 일반 파일이어야 합니다')
    fmt = source.suffix.lower()
    if fmt not in ('.docx', '.xlsx', '.pptx', '.html', '.htm'):
        raise UnsupportedInput(f'미지원 입력: {fmt}')
    assets.mkdir(parents=True, exist_ok=False)
    warnings = []
    if fmt in ('.html', '.htm'):
        parser = HtmlMarkdown(source, assets)
        parser.feed(source.read_text(encoding='utf-8-sig'))
        parser.close()
        markdown, warnings = ''.join(parser.out), parser.warnings
    else:
        package = Package(source, assets, warnings)
        try:
            markdown = {'.docx': docx, '.xlsx': xlsx, '.pptx': pptx}[fmt](package)
        finally:
            package.archive.close()
    if not markdown.strip():
        raise UnsupportedInput('변환할 본문이 없습니다')
    # 같은 목록의 연속 문단 사이 빈 줄은 생성기의 목록 깊이 상태를 초기화한다.
    markdown = re.sub(r'(^[ \t]*(?:-|\d+\.) [^\n]*)\n\n(?=[ \t]*(?:-|\d+\.) )', r'\1\n', markdown, flags=re.M)
    return AdaptedMarkdown(markdown.strip() + '\n', assets, tuple(dict.fromkeys(warnings)))
