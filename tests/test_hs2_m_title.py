"""실제 제목 칸 구조(한 칸 안의 부제·본제 두 문단) 회귀 시험."""
from pathlib import Path
import tempfile
import unittest
from zipfile import ZipFile

from docfit_core.timemachine import hierarchy_path as hp
from docfit_core.timemachine.content_search import ContentSearchIndex


class TitleCellTests(unittest.TestCase):
    def test_same_cell_fragments_skip_symbols_and_shorten(self):
        rows = [dict(paragraph_index=i, body=text, level="table_cell", level_rank=8,
                     table_cell_id=cell)
                for i, (text, cell) in enumerate([
                    ("1234 ※ -", 1), ("삼체 문명 존속을 위한", 2),
                    ("지구 " + "이주 계획 " * 20, 2), ("담당자 연락처", 3)])]
        before = [dict(row) for row in rows]
        title = hp.document_title_text(list(reversed(rows)))
        self.assertEqual(hp.shorten(rows[1]["body"] + " " + rows[2]["body"]), title)
        self.assertEqual(60, len(title))
        self.assertEqual(before, rows)

    def test_index_persists_cell_groups_without_changing_ranking(self):
        with tempfile.TemporaryDirectory(dir="E:/docfit-tmp") as tmp:
            root = Path(tmp)
            path = root / "plan.hwpx"
            with ZipFile(path, "w") as archive:
                archive.writestr("Contents/header.xml", '<head/>')
                archive.writestr("Contents/section0.xml", '''
<hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section"
 xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph">
 <hp:p><hp:run><hp:tbl><hp:tr>
 <hp:tc><hp:subList>
 <hp:p><hp:run><hp:t>삼체 문명 존속을 위한</hp:t></hp:run></hp:p>
 <hp:p><hp:run><hp:t>지구 이주·점령 계획(안) 보고</hp:t></hp:run></hp:p>
 </hp:subList></hp:tc>
 <hp:tc><hp:subList><hp:p><hp:run><hp:t>’26. 11. 12.(목)</hp:t></hp:run></hp:p></hp:subList></hp:tc>
 </hp:tr></hp:tbl></hp:run></hp:p>
 <hp:p><hp:run><hp:t>삼체 행성 환경을 조사한다</hp:t></hp:run></hp:p>
 <hp:p/><hp:p/>
</hs:sec>''')
            db = root / "content.sqlite3"
            index = ContentSearchIndex(db)
            try:
                result = index.index_file(path)
                self.assertTrue(result["indexed"], result)
                levels = [tuple(row) for row in index.db.execute(
                    "SELECT level,level_rank FROM content_paragraph_fts ORDER BY rowid")]
                self.assertEqual([("table_cell", 8)] * 3 + [("body", 8)], levels)
                after = index.keyword_search("삼체 행성")
                index.db.execute("DELETE FROM content_table_cells")
                before = index.keyword_search("삼체 행성")
                for field in ("score", "relevance_rank", "snippet", "position"):
                    self.assertEqual(before[0][field], after[0][field])
                self.assertEqual("삼체 문명 존속을 위한 지구 이주·점령 계획(안) 보고",
                                 after[0]["breadcrumb_text"])
                index.remove_file(path)
                self.assertTrue(index.index_file(path)["indexed"])
            finally:
                index.close()
            index = ContentSearchIndex(db)
            try:
                self.assertEqual(after[0]["breadcrumb_text"],
                                 index.keyword_search("삼체 행성")[0]["breadcrumb_text"])
                index.remove_file(path)
                self.assertEqual(0, index.db.execute("SELECT COUNT(*) FROM content_table_cells").fetchone()[0])
            finally:
                index.close()
