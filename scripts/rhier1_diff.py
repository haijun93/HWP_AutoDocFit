"""RHIER1 synthetic paragraph differential (no COM, no user documents)."""
import argparse
import json
import subprocess
import sys
from dataclasses import asdict
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from docfit_core.timemachine.content_search import ParagraphRecord, classify_paragraph_levels


def corpus():
    cases = [
        ('single.md', ['# 문서', '## 장', '### 절', '#### 주제', '본문']),
        ('multi.md', ['# 문서', '# 장', '## 절', '### 주제', '#### 본문']),
        ('numbers.txt', ['Ⅰ. 계획', '1. 목적', '가. 대상', '나. 범위', '2. 추진', 'Ⅱ. 실행']),
        ('list.md', ['# 제목', '1. 목록', '2. 목록', '가. 목록', '나. 목록']),
        ('body.txt', ['일반 본문', '1. 목록', '2. 목록']),
        ('chapters.txt', ['제1장 총칙', '본문', '제2장 계획']),
        ('symbols.txt', ['□ 상위', 'ㅇ 하위', '- 내용', '* 주석', '※ 참고']),
        ('labels.txt', ['제목1: 계획', '네모: 상위', '원： 하위', '바: 내용', '주석: 설명', '개요: 요약']),
        ('table.md', ['# 제목', '| 제목 | 내용 |', '| --- | --- |', '□ 상위', 'ㅇ 하위', '- 내용', '|', '||']),
    ]
    for name, texts in cases:
        yield name, [ParagraphRecord(i, t, 'body', 8, False) for i, t in enumerate(texts)]
    yield 'styles.hwpx', [ParagraphRecord(i, '스타일 제목', 'body', 8, False, style_name=s)
                          for i, s in enumerate(['제목', '제목1', '제목2', '제목3', 'heading1', 'heading2', 'heading3'])]
    yield 'title.hwpx', [ParagraphRecord(0, '연간 추진 계획', 'body', 8, True, table_cell_id=1, table_id=1),
                         ParagraphRecord(1, '부제 설명', 'body', 8, True, table_cell_id=1, table_id=1),
                         ParagraphRecord(2, '□ 추진', 'body', 8, False)]
    yield 'cell-context.hwpx', [
        ParagraphRecord(0, '# 표 제목', 'body', 8, True, table_cell_id=1, table_id=1),
        ParagraphRecord(1, '# 장 제목', 'body', 8, False),
        ParagraphRecord(2, '## 절 제목', 'body', 8, False),
        ParagraphRecord(3, '네모: 표 내용', 'body', 8, True, table_cell_id=2, table_id=1),
        ParagraphRecord(4, 'ㅇ 본문', 'body', 8, False),
    ]
    extra = [
        ['## 장', '### 절', '#### 주제'],
        ['제목: 계획', '개요: 내용', '주석1: 설명', '주석2: 추가', '당구: 참고', '점: 설명'],
        ['제1장', '제2장'], ['제1장 단독'], ['제1장 제목', '제2장 ' + '긴' * 65],
        ['Ⅰ. 장', 'Ⅱ. 장'], ['I. 장', 'II. 장'], ['1. 장', '2. 장'], ['가. 장', '나. 장'],
        ['Ⅰ. 장', '1. 절', '가. 주제', '(1) 하위', '① 항목', 'Ⅱ. 장', '2. 절', '나. 주제'],
        ['1. 항목', '3. 항목'], ['본문', '제1장 계획', '제2장 실행'],
        ['10. 계획', '11. 실행'], ['제목: 계획', '1． 계획', '2． 실행'],
        ['# 제목', '|---|---|', '1. 항목', '2. 항목'],
    ]
    for marker in ['□', 'ㅇ', '-', '◦', '■', 'ㅁ', '○', '※', '*', '**', '•', '·', '・', '–', '—', '−', '－', '①', '㉠', '㉮']:
        extra.append([f'{marker} 단독', f'{marker} 반복'])
    for i, texts in enumerate(extra):
        for suffix in ('md', 'hwpx'):
            yield f'extra-{i}.{suffix}', [ParagraphRecord(j, t, 'body', 8, False) for j, t in enumerate(texts)]


def compare(exe, *, dll=None, klib=None):
    differences, engine_differences, count = [], [], 0
    for path, records in corpus():
        expected = classify_paragraph_levels(path, records, backend='python')
        payload = {'paragraphs': [dict(asdict(p), source_path=path) for p in records]}
        done = subprocess.run([str(exe)], input=json.dumps(payload, ensure_ascii=False).encode(), capture_output=True, check=True)
        result = json.loads(done.stdout)
        with patch('docfit_app.hierarchy_bridge.analyze_levels', return_value=result):
            enabled = classify_paragraph_levels(path, records)
        if len(enabled) != len(expected) or [p.level for p in enabled] != [p.level for p in expected]:
            engine_differences.append({'path': path, 'error': 'engine enabled differs from Python'})
        backends = {'cli': result.get('search_levels', result.get('levels', []))}
        if dll:
            from docfit_app.hierarchy_bridge import run_library
            native = run_library(dll, payload['paragraphs'])
            backends['dll'] = (native or {}).get('search_levels', [])
        if klib:
            done = subprocess.run([str(klib), 'classify-levels', '7'],
                                  input=json.dumps(payload['paragraphs'], ensure_ascii=False).encode(),
                                  capture_output=True, check=True)
            backends['klib'] = json.loads(done.stdout)
        count += len(records)
        for backend, actual in backends.items():
            for i, record in enumerate(expected):
                found = actual[i] if i < len(actual) else None
                if found != record.level:
                    differences.append({'backend': backend, 'path': path, 'paragraph': i, 'text': record.text,
                                        'python': record.level, 'rust': found})
    return {'documents': len(list(corpus())), 'paragraphs': count, 'backends': list(backends),
            'difference_count': len(differences),
            'engine_difference_count': len(engine_differences), 'differences': differences,
            'engine_differences': engine_differences}

if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--exe', type=Path, required=True)
    parser.add_argument('--out', type=Path)
    parser.add_argument('--dll', type=Path)
    parser.add_argument('--klib', type=Path)
    args = parser.parse_args()
    result = compare(args.exe, dll=args.dll, klib=args.klib)
    if args.out:
        args.out.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding='utf-8')
    print(json.dumps(result, ensure_ascii=False, indent=2))
    sys.exit(bool(result['difference_count'] or result['engine_difference_count']))
