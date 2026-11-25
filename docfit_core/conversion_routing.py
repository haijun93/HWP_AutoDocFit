"""선택형 변환 라우팅. 기존 소비자는 이 모듈을 호출하지 않는다.

prepare -> native_attempt -> native_validation -> publish -> complete
                         +-> fallback -> kordoc_attempt -> publish -> complete
엔진 시도/검증 실패만 auto 폴백한다. 입력/출력 경로 오류·게시 충돌은 전파한다.
"""
from __future__ import annotations

from dataclasses import dataclass
import os
from pathlib import Path
import re
import tempfile
from zipfile import BadZipFile

from defusedxml.common import DefusedXmlException
from defusedxml import ElementTree as ET

from . import conversion_fidelity as cf
from .hwpx import validate_hwpx
from .input_markdown import adapt_to_markdown, UnsupportedInput, _open_zip_safely, W, A
from .markdown_hwpx import generate_hwpx, parse_markdown, Paragraph, Table


class NativeQualityError(ValueError):
    """지원 범위 밖 또는 손실 감지. auto에서만 kordoc으로 대체한다."""


@dataclass(frozen=True)
class RoutedConversion:
    path: Path
    engine: str
    states: tuple[str, ...]
    warnings: tuple[str, ...] = ()
    fallback_reason: str | None = None
    quality: dict | None = None


def configured_engine(settings: dict) -> str:
    """후속 소비자가 engine=으로 넘길 설정. 키 없는 기존 설정도 kordoc."""
    engine = settings.get('docx_engine', 'kordoc')
    if engine not in ('kordoc', 'native', 'auto'):
        raise ValueError(f'지원하지 않는 docx_engine: {engine}')
    return engine


def _coverage(source):
    """실측 범위 밖의 의미 요소가 조용히 빠지는 것을 보수적으로 차단한다."""
    if source.suffix.lower() not in ('.docx', '.pptx'):
        return
    with _open_zip_safely(source) as archive:
        for name in archive.namelist():
            is_doc = name == 'word/document.xml'
            is_slide = re.fullmatch(r'ppt/slides/slide\d+\.xml', name)
            if not (is_doc or is_slide):
                continue
            root = ET.fromstring(archive.read(name))
            for node in root.iter():
                local = node.tag.rsplit('}', 1)[-1]
                if local in ('chart', 'oMath', 'oMathPara', 'AlternateContent', 'smartTag', 'sdt', 'contentPart', 'videoFile', 'audioFile'):
                    raise NativeQualityError(f'미검증 의미 요소: {local}')
                if is_doc and node.tag == W + 'pStyle':
                    value = node.get(W + 'val', '')
                    if value not in ('Normal', 'ListParagraph', '') and not re.fullmatch(r'Heading[1-6]', value, re.I):
                        raise NativeQualityError(f'미검증 문단 스타일: {value}')
                if node.tag in (W + 'br', A + 'br') and node.get(W + 'type') == 'page':
                    raise NativeQualityError('쪽 구분 미지원')
        if source.suffix.lower() == '.pptx' and any(n.startswith('ppt/notesSlides/notesSlide') and n.endswith('.xml') for n in archive.namelist()):
            raise NativeQualityError('슬라이드 노트 미검증')


def _expected(document):
    """Markdown 의미 모델과 ZIP 직렬화 결과를 서로 다른 추출 경로로 비교한다."""
    headings, lists, tables, texts = [], [], [], []
    images = 0

    def text(runs):
        nonlocal images
        images += sum(r.image is not None for r in runs)
        return ''.join(r.text for r in runs)

    for block in document.blocks:
        if isinstance(block, Paragraph):
            value = text(block.runs)
            if block.level:
                headings.append([block.level, value])
            elif block.marker:
                lists.append([block.depth, block.marker[0].isdigit(), value])
            if value:
                texts.append(value)
        elif isinstance(block, Table):
            rows = [[text(cell) for cell in row] for row in block.rows]
            tables.append(rows)
            texts.extend(c for row in rows for c in row)
    return {'form': 'hwpx', 'headings': headings, 'lists': lists, 'tables': tables,
            'images': images, 'text': '\n'.join(texts)}


def _native(source, target, work, preset):
    if preset != '보고서':
        raise UnsupportedInput('native는 검증된 보고서 프리셋만 지원합니다')
    _coverage(source)
    if source.suffix.lower() in ('.md', '.markdown'):
        markdown, image_dir, warnings = source.read_text(encoding='utf-8-sig'), source.parent, ()
    else:
        adapted = adapt_to_markdown(source, work / 'assets')
        markdown, image_dir, warnings = adapted.markdown, adapted.image_dir, adapted.warnings
    if warnings:
        raise NativeQualityError('; '.join(warnings))
    document = parse_markdown(markdown, image_dir=image_dir)
    if document.warnings:
        raise NativeQualityError('; '.join(document.warnings))
    generated = generate_hwpx(markdown, target, image_dir=image_dir)
    if generated.warnings:
        raise NativeQualityError('; '.join(generated.warnings))
    validate_hwpx(target, structure_only=True, parse_xml=True)
    quality = cf.compare(_expected(document), cf.extract_hwpx(target))
    if not quality['verdict']['pass']:
        raise NativeQualityError(f'생성 품질 미달: {quality["metrics"]}')
    return quality


def _kordoc(source, target, work, preset):
    from . import kordoc_bridge as kb
    if source.suffix.lower() in ('.md', '.markdown'):
        kb.generate_hwpx(source, preset, target, image_dir=source.parent)
    elif source.suffix.lower() in ('.html', '.htm'):
        from .file_conversion import convert_to_pandoc_format
        markdown = work / 'kordoc.md'
        convert_to_pandoc_format(source, markdown)
        kb.generate_hwpx(markdown, preset, target, image_dir=source.parent)
    else:
        kb.convert_to_hwpx(source, target, preset=preset)
    validate_hwpx(target, structure_only=True, parse_xml=True)


def convert_to_hwpx_routed(path: str | Path, preset: str = '보고서', engine: str = 'auto',
                           *, target: str | Path | None = None) -> RoutedConversion:
    """새 API. 명시 auto=native 먼저, 설정 사용 시 engine=configured_engine(settings).

    설정 기본은 kordoc이다. native 손실 경고/품질 미달은 explicit native에서는
    오류, auto에서는 1회 폴백. 원본과 기존 출력은 어떤 엔진에서도 덮어쓰지 않는다.
    품질 검사는 조판·한/글 COM 인증을 뜻하지 않는다.
    """
    source = Path(path)
    if engine not in ('auto', 'native', 'kordoc'):
        raise ValueError(f'지원하지 않는 엔진: {engine}')
    if preset not in ('보고서', '계획서', '기안문'):
        raise ValueError(f'지원하지 않는 프리셋: {preset}')
    if source.is_symlink() or not source.is_file():
        raise ValueError('입력은 일반 파일이어야 합니다')
    target = Path(target) if target is not None else source.with_name(f'{source.stem}({preset}).hwpx')
    if target.suffix.lower() != '.hwpx' or source.resolve() == target.resolve():
        raise ValueError('원본과 다른 새 .hwpx 경로가 필요합니다')
    if target.exists() or target.is_symlink():
        raise FileExistsError(target)
    target.parent.mkdir(parents=True, exist_ok=True)
    states = ['prepare']
    reason, quality = None, None
    used = engine
    # 최종 결과와 같은 볼륨의 작업 폴더. 품질 판정 전에는 최종 경로를 만들지 않는다.
    with tempfile.TemporaryDirectory(prefix='.docfit-route-', dir=target.parent) as folder:
        work = Path(folder)
        staged = work / 'native.hwpx'
        if engine in ('auto', 'native'):
            states.append('native_attempt')
            try:
                quality = _native(source, staged, work, preset)
                states.append('native_validation')
                used = 'native'
            except (ValueError, BadZipFile, DefusedXmlException, ET.ParseError, KeyError, IndexError) as exc:
                if engine == 'native':
                    raise
                reason = f'{type(exc).__name__}: {exc}'
                states.append('fallback')
                used = 'kordoc'
        if used == 'kordoc':
            states.append('kordoc_attempt')
            staged = work / 'kordoc.hwpx'
            _kordoc(source, staged, work, preset)
        states.append('publish')
        # 같은 볼륨 hard link는 최종 이름을 배타적으로 확보한다. 충돌은 폴백하지 않는다.
        os.link(staged, target)
    states.append('complete')
    return RoutedConversion(target, used, tuple(states), (reason,) if reason else (), reason, quality)
