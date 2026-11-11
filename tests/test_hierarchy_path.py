"""문단 위계 순위로 상위 경로(장 > 절 > 항)를 만드는 순수 함수 시험."""
import os
import tempfile
import unittest

from docfit_core.timemachine import hierarchy_path as hp


def rows(*items):
    return [{"paragraph_index": i, "level_rank": rank, "level": name, "body": text}
            for i, (rank, name, text) in enumerate(items)]


DOC = rows(
    (0, "document_title", "2026년 관광시설 정비계획"),
    (1, "chapter", "Ⅰ. 시설물 정비"),
    (2, "section", "1. 관광안내판 철거"),
    (4, "square", "□ 전기시설 분리발주"),
    (5, "circle", "ㅇ 전기설비 해체공사는 별도 발주를 원칙으로 한다"),
    (8, "body", "세부 일정은 붙임과 같다"),
    (1, "chapter", "Ⅱ. 예산"),
    (4, "square", "□ 총사업비 3억원"),
    (8, "table_cell", "구분"),
)


class HierarchyPathTest(unittest.TestCase):
    def test_parents_follow_nearest_higher_level(self):
        ranks = [r["level_rank"] for r in DOC]
        self.assertEqual([None, 0, 1, 2, 3, 4, 0, 6, 7], hp.parents(ranks))

    def test_breadcrumb_restores_chapter_section_item(self):
        trail = hp.breadcrumb(DOC, 4)
        self.assertEqual(
            ["2026년 관광시설 정비계획", "Ⅰ. 시설물 정비", "1. 관광안내판 철거", "□ 전기시설 분리발주"],
            [step["text"] for step in trail])
        self.assertEqual("Ⅰ. 시설물 정비", trail[1]["text"])
        self.assertEqual(
            "2026년 관광시설 정비계획 > Ⅱ. 예산 > □ 총사업비 3억원", hp.breadcrumb_text(hp.breadcrumb(DOC, 8)))

    def test_new_chapter_does_not_inherit_previous_section(self):
        self.assertEqual(["2026년 관광시설 정비계획", "Ⅱ. 예산"],
                         [step["text"] for step in hp.breadcrumb(DOC, 7)])

    def test_body_paragraphs_are_never_parents_and_missing_index_is_empty(self):
        flat = rows((8, "body", "가"), (8, "body", "나"), (5, "circle", "ㅇ 다"), (8, "body", "라"))
        self.assertEqual([], hp.breadcrumb(flat, 1))
        self.assertEqual(["ㅇ 다"], [s["text"] for s in hp.breadcrumb(flat, 3)])
        self.assertEqual([], hp.breadcrumb(flat, 99))

    def test_long_heading_is_shortened_and_context_hits_counted(self):
        long_doc = rows((1, "chapter", "가" * 200), (8, "body", "본문"))
        trail = hp.breadcrumb(long_doc, 1)
        self.assertEqual(hp.MAX_HEADING_CHARS, len(trail[0]["text"]))
        self.assertTrue(trail[0]["text"].endswith("…"))
        self.assertEqual(1, hp.context_term_hits(hp.breadcrumb(DOC, 4), ["분리발주", "예산"]))

    def test_search_results_carry_breadcrumb(self):
        from docfit_core.timemachine.content_search import ContentSearchIndex, LEVEL_RANKS
        self.assertEqual(hp.BODY_RANK, LEVEL_RANKS["body"])
        with tempfile.TemporaryDirectory() as folder:
            source = os.path.join(folder, "정비계획.md")
            with open(source, "w", encoding="utf-8") as handle:
                handle.write("# 시설물 정비\n\n## 관광안내판 철거\n\n전기설비 해체공사는 별도 발주를 원칙으로 한다\n")
            index = ContentSearchIndex(os.path.join(folder, "content.sqlite3"))
            try:
                index.index_file(source)
                found = index.search("해체공사")
            finally:
                index.close()
        self.assertEqual(1, len(found))
        self.assertIn("breadcrumb", found[0])
        self.assertIn("breadcrumb_text", found[0])
        self.assertIn("context_hits", found[0])
        self.assertEqual(" > ".join(found[0]["breadcrumb"]), found[0]["breadcrumb_text"])


if __name__ == "__main__":
    unittest.main()
