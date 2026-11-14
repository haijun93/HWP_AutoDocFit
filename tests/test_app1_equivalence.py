import copy
import unittest

from tools.app1_equivalence import DEFAULT_QUERIES, Reference, canonical, classify_difference, report


def document(key, text, scopes=('tm',), level=8):
    return dict(doc_key=key, path=key, date=100, ref_date='2026-11-14', scopes=list(scopes),
                paras=[dict(p=0, lv=level, t=text)])


class EquivalenceTests(unittest.TestCase):
    def test_sentence_and_scope_filters(self):
        ref = Reference([document('one.txt', '예산이다. 집행한다.'),
                         document('two.md', '예산 집행 계획.'), document('three.txt', '예산 집행.', ('box:1',))])
        self.assertEqual(ref.search('예산 집행', 'tm')['total_docs'], 1)
        self.assertEqual(ref.search('예산 집행 ext:txt', 'tm')['total_docs'], 0)
        self.assertEqual(ref.search('예산 집행', 'box:1')['total_docs'], 1)
        self.assertEqual(ref.search('예산 -계획', 'tm')['total_docs'], 1)

    def test_order_count_phrase_and_dropped(self):
        ref = Reference([document('one.txt', '집행 예산.'), document('two.txt', '예산집행.')])
        found = ref.search('예산집행 및', 'tm')
        self.assertEqual([h['doc_key'] for h in found['hits']], ['two.txt', 'one.txt'])
        self.assertEqual(found['dropped'], ['및'])
        self.assertEqual(ref.search('"예산 집행"', 'tm')['total_docs'], 1)

    def test_missing_service_never_passes_a(self):
        rows = [dict(legacy_differences=[])]
        result = report(rows, copy.deepcopy(rows), service_checked=False, documents=1)
        self.assertIsNone(result['comparison_a_differences'])
        self.assertTrue(result['deterministic'])

    def test_unknown_difference_remains_failure(self):
        ref = Reference([document('one.txt', '예산 집행.')])
        new = ref.search('예산', 'tm')
        differences = classify_difference([dict(path='unavailable.txt')], new, '예산', ref)
        self.assertIn('UNKNOWN', [d['kind'] for d in differences])

    def test_e1_requires_document_and_no_sentence_match(self):
        ref = Reference([document('one.txt', '예산이다. 집행한다.')])
        new = ref.search('예산 집행', 'tm')
        self.assertEqual(classify_difference([dict(path='one.txt')], new, '예산 집행', ref)[0]['kind'], 'E1')

    def test_legacy_path_is_mapped_to_document_key_before_comparison(self):
        doc = document('identity.txt', '예산 집행.')
        doc['path'] = 'original.txt'
        ref = Reference([doc])
        self.assertEqual(classify_difference([dict(path='original.txt')], ref.search('예산', 'tm'), '예산', ref), [])

    def test_canonical_ignores_generation_but_preserves_positions(self):
        result = Reference([document('one.txt', '예산 집행.')]).search('예산', 'tm')
        changed = copy.deepcopy(result)
        changed['generation'] = 2
        self.assertEqual(canonical(result), canonical(changed))
        changed['hits'][0]['sentences'][0]['para'] = 9
        self.assertNotEqual(canonical(result), canonical(changed))

    def test_default_queries_at_least_80_unique(self):
        self.assertGreaterEqual(len(DEFAULT_QUERIES), 80)
        self.assertEqual(len(DEFAULT_QUERIES), len(set(DEFAULT_QUERIES)))

    def test_phrase_normalization_difference_classified_as_e5(self):
        # 따옴표 구문("예산 집행") 질의 시 본문이 붙여 쓴 "예산집행"이라 legacy에서는
        # 공백 불일치로 매칭 실패(0건)했으나 kfind에서는 fold 비교형 정규화로 일치하는 경우 E5로 분류
        ref = Reference([document('plan.md', '예산집행 계획.')])
        new = ref.search('"예산 집행"', 'tm')
        diffs = classify_difference([], new, '"예산 집행"', ref)
        self.assertEqual(len(diffs), 1)
        self.assertEqual(diffs[0]['kind'], 'E5')
        self.assertEqual(diffs[0]['direction'], 'added')
        self.assertEqual(diffs[0]['doc_key'], 'plan.md')

    def test_phrase_normalization_with_tempfile(self):
        # PC 절대 경로 하드코딩 없이 tempfile 임시 디렉토리를 사용한 따옴표 구문 E5 검증
        import os
        import tempfile
        with tempfile.TemporaryDirectory() as tmpdir:
            file_path = os.path.join(tmpdir, 'plan.md')
            doc = document('plan.md', '예산집행 계획.')
            doc['path'] = file_path
            ref = Reference([doc])
            new = ref.search('"예산 집행"', 'tm')
            diffs = classify_difference([], new, '"예산 집행"', ref)
            self.assertEqual(len(diffs), 1)
            self.assertEqual(diffs[0]['kind'], 'E5')
            self.assertEqual(diffs[0]['direction'], 'added')
            self.assertEqual(diffs[0]['doc_key'], 'plan.md')

    def test_phrase_removed_difference_classified_as_e1(self):
        # 따옴표 구문("예산 집행") 질의 시 legacy는 문단 전체에서 찾았으나
        # kfind 문장 단위 인덱싱에서는 구문 일치 문장이 없어 탈락한 경우(removed) E1으로 분류
        ref = Reference([document('doc.txt', '예산이다. 집행한다.')])
        new = ref.search('"예산 집행"', 'tm')
        diffs = classify_difference([dict(path='doc.txt')], new, '"예산 집행"', ref)
        self.assertEqual(len(diffs), 1)
        self.assertEqual(diffs[0]['kind'], 'E1')
        self.assertEqual(diffs[0]['direction'], 'removed')
        self.assertEqual(diffs[0]['doc_key'], 'doc.txt')
