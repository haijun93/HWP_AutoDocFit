"""APP1 전환 후 회귀 도구. 사본 SQLite와 격리 시험 파이프만 사용한다.

python -m tools.app1_equivalence --corpus COPIES --pipe PIPE.test.NAME --out REPORT
--reference-only는 서비스 미확인 보고서를 만들며 비교 A 합격으로 판정하지 않는다.

2026-11-14 변경(유형 4): 따옴표 구문("…") 질의 비교 시 parsed['phrases']가 누락되어
UNKNOWN으로 분류되던 문제를 수정. raw_terms에 phrases를 포함하고 원문 공백 불일치
구문 매칭을 E5(비교형 정규화 차이)로 정상 분류하도록 보완.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import tempfile

from docfit_app.kfind_client import KfindClient
from docfit_core.timemachine.content_search import ContentSearchIndex
from docfit_core.timemachine.kfind_records import doc_from_records
from tools.app1_reference.fold_ref import fold
from tools.app1_reference.oracle import Oracle, QUERIES
from tools.app1_reference.sentence_ref import index_spans

DEFAULT_QUERIES = list(dict.fromkeys(QUERIES + [
    '예산 ext:txt', '예산 ext:md', '예산 path:문서', '예산 path:없음',
    '예산 after:2020', '예산 before:2099', '예산 -집행', '예산 -없는말',
    '"예산 집행"', '예산 "추진 계획"', '예산 ext:txt -관리', '계획 after:2020 before:2099',
    '가', '가 예산', '및 계획', 'ㅇ 관리', 'ㄱ', 'ABC', '1234', '2026 예산', 'Plan', 'Budget 2026',
    '문화', '교육', '환경', '안전', '복지', '도로', '시설', '협의', '지원', '결과', '현황',
]))


def canonical(response):
    """세대·시간을 빼고 비교 A의 문서/문장 수·순서·위치를 남긴다."""
    return dict(total_docs=response['total_docs'], total_sentences=response['total_sentences'],
        dropped=response.get('dropped', []), hits=[dict(doc_key=h['doc_key'],
        adjacent=bool(h['adjacent']), level=h['level'], count=h['count'],
        sentences=sorted((s['para'], s['ord']) for s in h['sentences']))
        for h in response['hits'][:100]])


class Reference:
    def __init__(self, docs):
        self.docs = docs
        self.by_id = {i + 1: d for i, d in enumerate(docs)}
        corpus = []
        for doc_id, d in self.by_id.items():
            sents = [(p['p'], p['lv'], p['t'][start:start + length]) for p in d['paras']
                     for start, length in index_spans(p['t'])]
            corpus.append(dict(doc=doc_id, date=d['date'], sents=sents))
        self.oracle = Oracle(corpus)

    def search(self, query, scope):
        parsed = ContentSearchIndex.parse_query(query)
        if parsed['near'] is not None:
            raise ValueError('near는 legacy 전용이므로 비교 A 대상이 아닙니다')
        words, dropped = self.oracle.resolve(' '.join(parsed['terms']), 'dict', parsed['phrases'])
        excluded = set().union(*(self.oracle.doc_set(s) for s in parsed['exclude']))
        allowed = set()
        for doc_id, d in self.by_id.items():
            path = d['path']
            date = d.get('ref_date') or ''
            if scope not in d['scopes'] or doc_id in excluded:
                continue
            if parsed['ext'] and Path(path).suffix.lower().lstrip('.') not in parsed['ext']:
                continue
            if any(s.casefold() not in path.casefold() for s in parsed['path']):
                continue
            if parsed['after'] and (not date or date < parsed['after']):
                continue
            if parsed['before'] and (not date or date >= parsed['before']):
                continue
            allowed.add(doc_id)
        matched = {}
        if words:
            for doc_id, date, ord_, para, level, text in self.oracle.sents:
                if doc_id not in allowed:
                    continue
                strict = [w in text for w, _ in words]
                if all(s or (pieces and all(p in text for p in pieces))
                       for s, (_, pieces) in zip(strict, words)):
                    h = matched.setdefault(doc_id, dict(doc_key=self.by_id[doc_id]['doc_key'],
                        adjacent=False, level=255, count=0, date=date, sentences=[]))
                    h['adjacent'] |= all(strict)
                    h['level'] = min(h['level'], level)
                    h['count'] += 1
                    h['sentences'].append(dict(para=para, ord=ord_))
        hits = sorted(matched.items(), key=lambda pair: (-pair[1]['adjacent'], pair[1]['level'],
                      -pair[1]['count'], -pair[1]['date'], pair[0]))
        return dict(total_docs=len(hits), total_sentences=sum(h['count'] for _, h in hits),
                    dropped=dropped, hits=[h for _, h in hits], words=words)


def classify_difference(old, new, query, reference):
    """근거가 있는 종류만 붙인다. 모르는 차이는 UNKNOWN으로 남겨 실패시킨다."""
    new_keys = [h['doc_key'] for h in new['hits']]
    keys_by_path = {os.path.normcase(os.path.normpath(d['path'])): d['doc_key'] for d in reference.docs}
    old = [dict(h, path=keys_by_path.get(os.path.normcase(os.path.normpath(h['path'])), h['path'])) for h in old]
    old_keys = [h['path'] for h in old]
    if old_keys == new_keys:
        return []
    evidence = []
    parsed = ContentSearchIndex.parse_query(query)
    raw_terms = [*parsed['terms'], *parsed['phrases']]
    words = [fold(t) for t in raw_terms if fold(t)]
    docs = {d['doc_key']: d for d in reference.docs}
    for key in sorted(set(old_keys) - set(new_keys)):
        row = next(r for r in old if r['path'] == key)
        doc = docs.get(key)
        body = ' '.join(p['t'] for p in doc['paras']) if doc else ''
        if row.get('context_hits') or row.get('_context_match'):
            kind = 'E4'
        elif words and all(w in fold(body) for w in words) and not any(
                all(w in fold(p['t'][start:start + length]) for w in words)
                for p in doc['paras'] for start, length in index_spans(p['t'])):
            kind = 'E1'
        else:
            kind = 'UNKNOWN'
        evidence.append(dict(kind=kind, doc_key=key, direction='removed'))
    for key in sorted(set(new_keys) - set(old_keys)):
        hit = next(h for h in new['hits'] if h['doc_key'] == key)
        doc = docs.get(key)
        body = ' '.join(p['t'] for p in doc['paras']) if doc else ''
        if any(p for _, p in new.get('words', [])):
            kind = 'E2'
        elif words and all(w in fold(body) for w in words) and (
                not all(w in body for w in words) or not all(t in body for t in raw_terms)):
            kind = 'E5'
        else:
            kind = 'UNKNOWN'
        evidence.append(dict(kind=kind, doc_key=key, direction='added'))
    if set(old_keys) == set(new_keys):
        # E3는 순서가 바뀐 쌍에서 adjacent 우선이 실제 역전된 경우만 인정한다.
        adj = {h['doc_key']: h['adjacent'] for h in new['hits']}
        inversions = [(a, b) for i, a in enumerate(new_keys) for b in new_keys[i+1:]
                      if old_keys.index(a) > old_keys.index(b)]
        kind = 'E3' if inversions and all(adj[a] and not adj[b] for a, b in inversions) else 'UNKNOWN'
        evidence.append(dict(kind=kind, direction='order'))
    return evidence


def compare_run(reference, index, scope, queries, client=None):
    rows = []
    for i, query in enumerate(queries):
        expected = reference.search(query, scope)
        old = index.keyword_search(query, limit=100)
        row = dict(query=query, expected=canonical(expected),
                   legacy_differences=classify_difference(old, expected, query, reference))
        if client is not None:
            p = ContentSearchIndex.parse_query(query)
            filters = {k: p[k] for k in ('ext', 'path', 'after', 'before', 'exclude', 'phrases')}
            actual = client.query(' '.join(p['terms']), scope, top=100, refs=100,
                                  filters=filters, request_id=i + 1)
            row['actual'] = canonical(actual)
            count = client.count(' '.join(p['terms']), scope, filters=filters, request_id=len(queries)+i+1)
            row['count_equal'] = all(count[k] == expected[k] for k in ('total_docs', 'total_sentences'))
            row['equal'] = row['actual'] == row['expected'] and row['count_equal']
        rows.append(row)
    return rows


def report(rows, repeat, *, service_checked, documents):
    return dict(schema=1, documents=documents, queries=len(rows), service_checked=service_checked,
        comparison_b_backend='sqlite_keyword',
        comparison_a_differences=sum(not r.get('equal', False) for r in rows) if service_checked else None,
        comparison_b_unexplained=sum(d['kind'] == 'UNKNOWN' for r in rows for d in r['legacy_differences']),
        deterministic=rows == repeat, rows=rows)


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--corpus', type=Path, required=True, help='사용자가 준비한 문서 사본 폴더(읽기만 함)')
    ap.add_argument('--queries', type=Path, help='UTF-8 검색어 한 줄씩')
    ap.add_argument('--scope', default='tm', help='tm / box:<id> / nb:<id>')
    ap.add_argument('--pipe', help='.test.가 포함된 전용 시험 파이프')
    ap.add_argument('--reference-only', action='store_true')
    ap.add_argument('--out', type=Path, required=True, help='확장자 없는 보고서 경로')
    args = ap.parse_args(argv)
    if not args.reference_only and (not args.pipe or '.test.' not in args.pipe):
        ap.error('실서비스 오염 방지: .test.가 포함된 전용 시험 파이프가 필요합니다')
    if not args.corpus.is_dir():
        ap.error('문서 사본 폴더가 없습니다')
    queries = args.queries.read_text(encoding='utf-8').splitlines() if args.queries else DEFAULT_QUERIES
    queries = [q for q in queries if q.strip()]
    client = None if args.reference_only else KfindClient(args.pipe)
    with tempfile.TemporaryDirectory(prefix='app1-equiv-') as tmp:
        copied = Path(tmp) / 'corpus'
        shutil.copytree(args.corpus, copied)
        index = ContentSearchIndex(Path(tmp) / 'content.sqlite')
        docs = []
        try:
            for path in sorted(copied.rglob('*')):
                if path.is_file():
                    result = index.index_file(path)
                    if result.get('error'):
                        raise RuntimeError(f'색인 실패: {path.name}: {result}')
                    stat = path.stat()
                    docs.append(doc_from_records(str(path), index.paragraphs_for_kfind(path), stat,
                        __import__('datetime').datetime.fromtimestamp(max(stat.st_ctime, stat.st_mtime)).strftime('%Y-%m-%d'),
                        [args.scope]))
            if not docs:
                raise ValueError('사본 폴더에 색인 문서가 없습니다')
            if client:
                status = client.status()
                if status.get('docs', 0):
                    raise ValueError('비어 있는 시험 서비스가 필요합니다')
                for doc in docs:
                    client.ingest(doc)
            reference = Reference(docs)
            rows = compare_run(reference, index, args.scope, queries, client)
            repeated = compare_run(reference, index, args.scope, queries, client)
            result = report(rows, repeated, service_checked=client is not None, documents=len(docs))
            # 임시 루트는 두 번 실행해도 결과 파일에 남지 않게 정규화한다.
            encoded = json.dumps(result, ensure_ascii=False, indent=2)
            for root in (str(copied), os.path.normcase(str(copied))):
                encoded = encoded.replace(json.dumps(root)[1:-1], '<CORPUS>')
            args.out.parent.mkdir(parents=True, exist_ok=True)
            args.out.with_suffix('.json').write_text(encoded + '\n', encoding='utf-8')
            digest = hashlib.sha256((encoded + '\n').encode('utf-8')).hexdigest()
            args.out.with_suffix('.md').write_text(
                f'# APP1 동등성 회귀 보고\n\n문서 {len(docs)}개, 검색어 {len(queries)}개, 같은 입력 2회 일치: {result["deterministic"]}.\n\n'
                f'실제 서비스 확인: {client is not None}. 비교 A 차이: {result["comparison_a_differences"]}. '
                f'비교 B 미설명: {result["comparison_b_unexplained"]}.\n\n'
                '비교 B는 SQLite 본문검색만 대조합니다. klib Tantivy·mentions 정렬은 이 보고의 검증 범위 밖입니다.\n\n'
                f'JSON SHA256: {digest}\n\n서비스를 확인하지 않은 실행은 비교 A 통과가 아닙니다. '
                'E6·E7·E8은 증거가 부족하면 UNKNOWN으로 유지합니다.\n', encoding='utf-8')
            return 0 if client and result['comparison_a_differences'] == 0 and result['deterministic'] else 2
        finally:
            index.close()
            if client:
                client.close()


if __name__ == '__main__':
    raise SystemExit(main())
