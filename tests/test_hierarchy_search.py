import unittest

from docfit_core.hierarchy_search import build_index, search


class HierarchySearchTest(unittest.TestCase):
    def setUp(self):
        self.index = build_index({"nodes": [
            {"id": 1, "role": "chapter", "level": 2, "text": "시설물 정비", "parent_id": None},
            {"id": 2, "role": "section", "level": 3, "text": "관광안내판 철거", "parent_id": 1},
            {"id": 3, "role": "detail", "level": 4, "text": "전기시설 분리발주", "parent_id": 2},
            {"id": 4, "role": "square", "level": 5, "text": "전기설비 해체공사는 별도 발주를 원칙으로 한다", "parent_id": 3},
        ]}, document_name="2026년 관광시설 정비계획.hwpx")

    def test_breadcrumb_restores_parent_context(self):
        result = search(self.index, "전기공사 분리발주")
        self.assertTrue(result)
        self.assertEqual(result[0]["breadcrumb"], "시설물 정비 > 관광안내판 철거 > 전기시설 분리발주")

    def test_empty_query_and_limit(self):
        self.assertEqual(search(self.index, "", limit=5), [])
        self.assertEqual(search(self.index, "분리발주", limit=0), [])


if __name__ == "__main__":
    unittest.main()
