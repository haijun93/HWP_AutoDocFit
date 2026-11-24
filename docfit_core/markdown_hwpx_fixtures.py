"""KORDOCB2 계열 합성 Markdown 11종. 사용자 문서·외부 자원 없음."""
from __future__ import annotations

from .conversion_fixtures import DOC_SPECS, expected_from_blocks, render_markdown


def fixtures() -> dict[str, tuple[str, dict]]:
    cases = {
        name: (render_markdown(blocks), expected_from_blocks(blocks))
        for name, blocks in DOC_SPECS.items()
    }
    blocks = {
        "headings": [("h", 1, "공공 보고서"), ("h", 2, "추진 배경"), ("h", 3, "세부 계획"), ("p", "기본 내용")],
        "public_markers": [("ul", [(0, "추진 방향"), (1, "운영 방식"), (2, "세부 내용"), (0, "참고 사항")])],
        "bullets": [("ul", [(0, "일반 하이픈"), (0, "별표 항목"), (0, "추가 항목")])],
        "ordered": [("ol", [(0, "계획 수립"), (0, "시범 운영"), (0, "전면 시행")])],
        "nested": [("ul", [(0, "상위 가"), (1, "하위 가"), (2, "세부 가"), (1, "하위 나"), (0, "상위 나")])],
        "emphasis": [("h", 1, "강조 시험"), ("p", "일반 굵게 기울임 동시 강조 끝"),
                     ("table", [["항목", "설명"], ["굵은 칸", "기울인 칸"]])],
        "linebreaks": [("h", 1, "줄바꿈 시험"), ("p", "첫째 줄 둘째 줄 셋째 줄 넷째 줄"),
                       ("table", [["항목", "설명"], ["줄 하나 줄 둘", "A B"]])],
        "mixed": [("h", 1, "2026년 업무 보고"), ("p", "한국어 English 1,234 & <태그>"),
                  ("ul", [(0, "접수 120건"), (1, "처리 118건")]),
                  ("table", [["구분", "값"], ["합계", "1,234"], ["부호", "A | B"]]), ("p", "끝.")],
    }
    for name, spec in blocks.items():
        cases[name] = (render_markdown(spec), expected_from_blocks(spec))
    overrides = {
        "public_markers": "□\t  추진 방향\n  ㅇ   운영 방식\n    -\t세부 내용\n※  참고 사항\n",
        "bullets": "- 일반 하이픈\n* 별표 항목\n+ 추가 항목\n",
        "emphasis": "# 강조 시험\n\n일반 **굵게** *기울임* ***동시 강조*** 끝\n\n| 항목 | 설명 |\n| --- | --- |\n| **굵은 칸** | *기울인 칸* |\n",
        "linebreaks": "# 줄바꿈 시험\n\n첫째 줄  \n둘째 줄\\\n셋째 줄<br>넷째 줄\n\n| 항목 | 설명 |\n| --- | --- |\n| 줄 하나<br>줄 둘 | A<br/>B |\n",
        "mixed": "# 2026년 업무 보고\n\n한국어 English 1,234 & <태그>\n\n- 접수 120건\n  - 처리 118건\n\n| 구분 | 값 |\n| --- | --- |\n| 합계 | 1,234 |\n| 부호 | A \\| B |\n\n끝.\n",
    }
    for name, source in overrides.items():
        cases[name] = (source, cases[name][1])
    return cases


def fixtures_v2() -> dict[str, tuple[str, dict]]:
    """기존 11종을 그대로 포함한다. 정답은 출력과 독립적인 블록 명세다."""
    cases = fixtures()
    specs = {
        'heading_456': ('#### 네 단계\n\n##### 다섯 단계\n\n###### 여섯 단계\n\n본문',
                        [('h', 4, '네 단계'), ('h', 5, '다섯 단계'), ('h', 6, '여섯 단계'), ('p', '본문')]),
        'links': ('[한글 링크](https://example.invalid/a?x=1&y=2) 및 [**강조**](mailto:test@example.invalid)',
                  [('p', '한글 링크 및 강조')]),
        'code': ('```python\ndef sample():\n    return "<br> **원문**"\n\nprint(sample())\n```\n\n끝',
                 [('p', 'def sample():\n    return "<br> **원문**"\n\nprint(sample())'), ('p', '끝')]),
        'png_wide': ('# 넓은 그림\n\n![넓은 합성 그림](wide.png)\n\n끝',
                     [('h', 1, '넓은 그림'), ('img', '넓은 합성 그림'), ('p', '끝')]),
        'jpeg_relative': ('앞 ![한글 대체글 & 설명](images/photo.jpg) 뒤',
                          [('p', '앞  뒤'), ('img', '한글 대체글 & 설명')]),
        'table_picture': ('| 그림 | 설명 |\n| --- | --- |\n| ![칸 안 그림](wide.png) | 칸 **내용** |',
                          [('table', [['그림', '설명'], ['', '칸 내용']]), ('img', '칸 안 그림')]),
        'code_inline': ('코드 `a < b && **c**` 끝\n\n~~~~\n![그림 아님](missing.png)\n\t코드 탭\n~~~~~',
                        [('p', '코드 a < b && **c** 끝'), ('p', '![그림 아님](missing.png)\n    코드 탭')]),
        'breaks_mixed': ('#### 줄바꿈 제목\n\n가  \n나\\\n다<br/>라<br />마\n\n| A | B |\n| --- | --- |\n| 첫<br>둘<br/>셋 | 끝 |',
                         [('h', 4, '줄바꿈 제목'), ('p', '가 나 다 라 마'), ('table', [['A', 'B'], ['첫 둘 셋', '끝']])]),
    }
    for name, (source, blocks) in specs.items():
        cases[name] = source, expected_from_blocks(blocks)
    return cases


def write_images(directory):
    """사용자 파일 없이 PNG/JPG 자원을 만든다."""
    import io
    from pathlib import Path
    from PIL import Image
    from .conversion_fixtures import png_bytes

    root = Path(directory)
    (root / 'images').mkdir(parents=True, exist_ok=True)
    (root / 'image1.png').write_bytes(png_bytes())
    (root / 'wide.png').write_bytes(png_bytes(1200, 300))
    stream = io.BytesIO()
    Image.new('RGB', (80, 120), (40, 80, 160)).save(stream, format='JPEG')
    (root / 'images' / 'photo.jpg').write_bytes(stream.getvalue())
