"""제목 표 승격과 규칙 버전 갱신 회귀 시험."""
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
from docfit_core.timemachine.content_search import (
    ParagraphRecord, classify_paragraph_levels, ContentSearchIndex,
    CURRENT_HIERARCHY_SCHEMA_VERSION,
)
from tests.test_timemachine_content_search import write_hwpx_paragraphs


class TitlePromotionTests(unittest.TestCase):
    def classify(self, prefix=0, explicit=False):
        rows = [ParagraphRecord(i, "삼체 문명", "body", 8, False) for i in range(prefix)]
        rows += [ParagraphRecord(prefix, "삼체 문명", "body", 8, True, table_cell_id=1),
                 ParagraphRecord(prefix+1, "삼체 문명", "body", 8, True, table_cell_id=1),
                 ParagraphRecord(prefix+2, "담당자", "body", 8, True, table_cell_id=2)]
        if explicit:
            rows.append(ParagraphRecord(prefix+3, "삼체 문명", "body", 8, False, style_name="제목"))
        with patch("docfit_app.hierarchy_bridge.analyze_levels", return_value=None):
            return classify_paragraph_levels("sample.hwpx", rows)

    def test_cell_fragments_and_leading_body_boundary(self):
        for prefix in (0, 1, 2):
            records = self.classify(prefix)
            self.assertEqual(["document_title", "document_title", "table_cell"],
                             [p.level for p in records[prefix:]])
        self.assertEqual("table_cell", self.classify(3)[3].level)
        self.assertEqual("table_cell", self.classify(explicit=True)[0].level)

    def test_only_first_table_is_considered(self):
        with tempfile.TemporaryDirectory(dir="E:/docfit-tmp") as tmp:
            path = Path(tmp) / "tables.hwpx"
            write_hwpx_paragraphs(path, [{"text": "1234", "is_table_cell": True},
                                        {"text": "삼체 문명", "is_table_cell": True}])
            from docfit_core.timemachine.content_search import extract_paragraphs
            self.assertEqual(["table_cell", "table_cell"], [p.level for p in extract_paragraphs(path)])

    def test_engine_path_and_breadcrumb_preserve_whole_title(self):
        from docfit_core.timemachine import hierarchy_path
        records = [ParagraphRecord(0, "삼체 문명", "body", 8, True, table_cell_id=1),
                   ParagraphRecord(1, "이주 계획", "body", 8, True, table_cell_id=1),
                   ParagraphRecord(2, "행성 조사", "body", 8, False)]
        with patch("docfit_app.hierarchy_bridge.analyze_levels", return_value={"levels": [8, 8, 8]}):
            records = classify_paragraph_levels("sample.hwpx", records)
        self.assertEqual(["document_title", "document_title", "body"], [p.level for p in records])
        rows = [dict(paragraph_index=p.paragraph_index, body=p.text, level=p.level,
                     level_rank=p.level_rank, table_cell_id=p.table_cell_id) for p in records]
        self.assertIsNone(hierarchy_path.document_title_text(rows))
        self.assertEqual("삼체 문명 이주 계획", hierarchy_path.breadcrumb_text(hierarchy_path.breadcrumb(rows, 2)))
