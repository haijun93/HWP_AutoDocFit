import tempfile
import unittest
import time
import sqlite3
import os
from pathlib import Path
from zipfile import ZipFile
from unittest.mock import patch

from docfit_core.timemachine.content_search import (
    ContentSearchIndex,
    extract_text,
    extract_paragraphs,
    classify_paragraph_levels,
    LEVEL_ORDER,
    LEVEL_LABELS,
    CURRENT_HIERARCHY_SCHEMA_VERSION,
    ParagraphRecord,
)


def write_hwpx_paragraphs(path: Path, paragraphs: list[dict]) -> None:
    """HWPX fixture 제작 helper. 외부 템플릿 없이 최소 XML Zip을 구성한다."""
    header_styles = []
    style_id_map = {}
    sid_counter = 0
    for p in paragraphs:
        sname = p.get("style_name")
        if sname and sname not in style_id_map:
            style_id_map[sname] = str(sid_counter)
            header_styles.append(f'<hh:style id="{sid_counter}" type="para" name="{sname}" />')
            sid_counter += 1

    header_xml = f"""<?xml version="1.0" encoding="utf-8"?>
    <hh:head xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head">
      <hh:styles>
        {''.join(header_styles)}
      </hh:styles>
    </hh:head>"""

    body_elements = []
    for p in paragraphs:
        text = p.get("text", "")
        sname = p.get("style_name")
        sid_attr = f' styleIDRef="{style_id_map[sname]}"' if (sname and sname in style_id_map) else ""
        olvl = p.get("outline_level")
        olvl_attr = f' outlineLevel="{olvl}"' if olvl is not None else ""
        is_tbl = p.get("is_table_cell", False)

        p_inner = f'<hp:run><hp:t>{text}</hp:t></hp:run>'
        if is_tbl:
            body_elements.append(
                f'<hp:p><hp:run><hp:tbl><hp:tr><hp:tc><hp:p{sid_attr}{olvl_attr}>{p_inner}</hp:p></hp:tc></hp:tr></hp:tbl></hp:run></hp:p>'
            )
        else:
            body_elements.append(f'<hp:p{sid_attr}{olvl_attr}>{p_inner}</hp:p>')

    section_xml = f"""<?xml version="1.0" encoding="utf-8"?>
    <hp:sec xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph">
      {''.join(body_elements)}
    </hp:sec>"""

    path.parent.mkdir(parents=True, exist_ok=True)
    with ZipFile(path, "w") as archive:
        archive.writestr("Contents/header.xml", header_xml)
        archive.writestr("Contents/section0.xml", section_xml)


class ContentSearchTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.index = ContentSearchIndex(self.root / "content.sqlite")

    def tearDown(self):
        self.index.close()
        self.tmp.cleanup()

    def test_context_search_results_match_across_all_synthetic_queries(self):
        """10개 기준 질의의 전체 결과 필드(정렬·문단·경로·문맥)가 유지된다."""
        with self.index._lock, self.index.db:
            for doc in range(300):
                path = f"{self.root}/doc{doc:03}.md"
                paragraphs = [(0, "document_title", 0, "# 시설 사업"),
                              (1, "chapter", 1, f"## 관광안내판 구역{doc % 10:03}")]
                paragraphs.extend((p + 2, "body", 8,
                                   f"해체공사 공정{p % 10:03} 시행 계획과 안전관리 {p}")
                                  for p in range(38))
                self.index.db.execute(
                    "INSERT INTO content_meta(path,name,extension,mtime,ctime,hierarchy_schema_version) "
                    "VALUES(?,?,?,?,?,?)",
                    (path, Path(path).name, "md", 1, 1, CURRENT_HIERARCHY_SCHEMA_VERSION))
                for paragraph_index, level, rank, body in paragraphs:
                    self.index.db.execute(
                        "INSERT INTO content_paragraph_fts(path,name,paragraph_index,level,level_rank,body) "
                        "VALUES(?,?,?,?,?,?)",
                        (path, Path(path).name, paragraph_index, level, rank, body))
        queries = [f"관광안내판 해체공사 구역{i:03}" for i in range(5)]
        queries += [f"해체공사 공정{i:03}" for i in range(5)]
        for query in queries:
            results = self.index.search(query, limit=500)
            self.assertTrue(results, query)
            self.assertEqual([row["relevance_rank"] for row in results], list(range(1, len(results) + 1)))
            self.assertEqual(len(results), 300 if query.startswith("해체공사 공정") else 30)
            for row in results:
                self.assertEqual(row["breadcrumb"], ["시설 사업", row["name"] and f"관광안내판 구역{int(row['name'][3:6]) % 10:03}"])
                self.assertIn("context_hits", row)
                self.assertIn("snippet", row)

    def test_engine_enabled_converts_levels_for_index(self):
        from unittest.mock import patch
        from docfit_core.timemachine.content_search import classify_paragraph_levels
        records = [ParagraphRecord(i, text, "body", LEVEL_ORDER.index("body"), False)
                   for i, text in enumerate(("제목", "본문"))]
        with patch("docfit_app.hierarchy_bridge.analyze_levels",
                   return_value={"levels": [0, 5]}):
            result = classify_paragraph_levels("dummy.txt", records)
        self.assertEqual([p.level for p in result], ["document_title", "circle"])

    def test_engine_unavailable_preserves_python_index_classification(self):
        from unittest.mock import patch
        records = [ParagraphRecord(0, "ㅁ 항목", "body", LEVEL_ORDER.index("body"), False)]
        with patch("docfit_app.hierarchy_bridge.analyze_levels", return_value=None):
            result = classify_paragraph_levels("dummy.txt", records)
        self.assertEqual(result[0].level, "square")

    def test_title_fallback_text_labels_and_existing_title(self):
        for suffix in ("txt", "md"):
            for prefix in ("제목1:", "제목:"):
                with self.subTest(suffix=suffix, prefix=prefix):
                    path = self.root / ("title." + suffix)
                    path.write_text(prefix + " 삼체 계획\n제목: 두 번째\n삼체 행성", encoding="utf-8")
                    records = extract_paragraphs(path)
                    self.assertEqual([p.level for p in records], ["document_title", "body", "body"])
                    self.assertEqual(records[0].text, prefix + " 삼체 계획")
        path = self.root / "existing.md"
        path.write_text("# 명시적 제목\n제목: 준말", encoding="utf-8")
        self.assertEqual([p.level for p in extract_paragraphs(path)], ["document_title", "body"])

    def test_title_fallback_keeps_hwpx_table_levels_after_rule_revert(self):
        for count in (0, 2, 3):
            with self.subTest(preceding_body=count):
                path = self.root / f"table{count}.hwpx"
                write_hwpx_paragraphs(path, [
                    *[{"text": "일반 본문 문장"} for _ in range(count)],
                    {"text": "", "is_table_cell": True},
                    {"text": "삼체 계획", "is_table_cell": True},
                    {"text": "다음 칸", "is_table_cell": True},
                ])
                # helper는 항목마다 별도 표를 만든다. 빈 첫 표는 제목 후보가 아니다.
                self.assertFalse(any(p.level == "document_title" for p in extract_paragraphs(path)))
                write_hwpx_paragraphs(path, [
                    *[{"text": "일반 본문 문장"} for _ in range(count)],
                    {"text": "삼체 계획", "is_table_cell": True},
                    {"text": "다음 칸", "is_table_cell": True},
                ])
                records = extract_paragraphs(path)
                self.assertEqual(records[count].level, "table_cell")
                self.assertTrue(records[count].is_table_cell)
                self.assertEqual(records[count + 1].level, "table_cell")

    def test_title_label_reaches_keyword_search_breadcrumb(self):
        path = self.root / "labeled.txt"
        path.write_text("제목1: 삼체 문명 대응 계획\n□ 추진 배경\n삼체 행성 상황", encoding="utf-8")
        self.assertTrue(self.index.index_file(path)["indexed"])
        result = self.index.keyword_search("삼체 행성")
        self.assertEqual(len(result), 1)
        self.assertEqual(result[0]["breadcrumb"], ["제목1: 삼체 문명 대응 계획", "□ 추진 배경"])

    def test_title_fallback_preserves_engine_title_and_applies_after_engine(self):
        records = [ParagraphRecord(0, "제목1: 삼체 계획", "body", 8, False),
                   ParagraphRecord(1, "별도 제목", "body", 8, False)]
        with patch("docfit_app.hierarchy_bridge.analyze_levels", return_value={"levels": [8, 0]}):
            self.assertEqual([p.level for p in classify_paragraph_levels("a.txt", records)],
                             ["body", "document_title"])
        with patch("docfit_app.hierarchy_bridge.analyze_levels", return_value={"levels": [8, 8]}):
            self.assertEqual([p.level for p in classify_paragraph_levels("a.txt", records)],
                             ["document_title", "body"])

    def test_txt_search_snippet_and_incremental_update(self):
        path = self.root / "보고서.txt"
        path.write_text("분기별 업무 실적 검색 대상 문장", encoding="utf-8")
        self.assertTrue(self.index.index_file(path)["indexed"])
        self.assertFalse(self.index.index_file(path)["indexed"])
        found = self.index.search("업무 실적", roots=[str(self.root)])
        self.assertEqual(found[0]["name"], path.name)
        self.assertIn("업무 실적", found[0]["snippet"])
        path.write_text("새로운 내용", encoding="utf-8")
        self.assertTrue(self.index.index_file(path)["indexed"])
        self.assertEqual(self.index.search("업무 실적"), [])

    def test_excluded_filename_is_not_indexed_or_searchable(self):
        path = self.root / " '#IEeEE legacy'.txt"
        path.write_text("needleword hidden", encoding="utf-8")
        self.assertFalse(self.index.index_file(path)["indexed"])
        self.assertEqual(self.index.search("needleword"), [])

    def test_hwpx_uses_defused_xml_extractor(self):
        path = self.root / "문서.hwpx"
        with ZipFile(path, "w") as archive:
            archive.writestr("Contents/header.xml", "<head/>")
            archive.writestr("Contents/section0.xml", '<root><p>타임머신 본문 검색</p></root>')
        self.assertIn("타임머신 본문", extract_text(path))
        self.index.index_file(path)
        self.assertEqual(len(self.index.search("타임머신")), 1)

    def test_failures_are_retained_and_trash_is_filtered(self):
        broken = self.root / "bad.docx"
        broken.write_text("not a zip", encoding="utf-8")
        self.assertIsNotNone(self.index.index_file(broken)["error"])
        self.assertEqual(len(self.index.errors([str(self.root)])), 1)
        trash = self.root / "$Recycle.Bin"
        trash.mkdir()
        hidden = trash / "hidden.txt"
        hidden.write_text("secretword", encoding="utf-8")
        self.index.index_file(hidden)
        self.assertEqual(self.index.search("secretword"), [])

    def test_query_operators_and_korean_substrings(self):
        p1 = self.root / "a-report.txt"
        p2 = self.root / "b-other.txt"
        p1.write_text("대한민국의 행정문서 검색 대상 문구 그리고 빠른 갈색 여우", encoding="utf-8")
        p2.write_text("대한민국 행정문서 검색 대상", encoding="utf-8")
        self.index.index_file(p1); self.index.index_file(p2)
        self.assertEqual(len(self.index.search("대한민국의")), 1)
        self.assertEqual(len(self.index.search("행정")), 2)
        self.assertEqual([x["name"] for x in self.index.search('"검색 대상" -빠른')], [p2.name])
        self.assertEqual(set(x["name"] for x in self.index.search("ext:txt 대한민국")), {p1.name, p2.name})
        self.assertEqual([x["name"] for x in self.index.search("path:a-report 대한민국")], [p1.name])
        self.assertEqual([x["name"] for x in self.index.search("대한민국 빠른 ~5")], [p1.name])

    def test_rank_snippet_highlight_and_filename_cache(self):
        common = self.root / "common.txt"
        strong = self.root / "strong.txt"
        common.write_text("업무 검색 결과", encoding="utf-8")
        strong.write_text("업무 검색 업무 검색 업무 검색", encoding="utf-8")
        self.index.index_file(common); self.index.index_file(strong)
        found = self.index.search("업무 검색")
        self.assertEqual(found[0]["name"], strong.name)
        self.assertTrue(found[0]["highlights"])
        self.assertEqual(found[0]["snippet"][slice(*found[0]["highlights"][0])], "업무")
        names = self.index.search_files("strong txt")
        self.assertEqual(names[0]["name"], strong.name)

    def test_sql_document_aggregation_matches_legacy_top20_and_snippets(self):
        # Small synthetic FTS corpus with ties, hierarchy levels, and multiple hits.
        with self.index._lock, self.index.db:
            for doc in range(24):
                path = f"{self.root}/synthetic-{doc:02}.txt"
                self.index.db.execute(
                    "INSERT INTO content_meta(path,name,extension,mtime,ctime,hierarchy_schema_version) VALUES(?,?,?,?,?,?)",
                    (path, Path(path).name, "txt", 1_800_000_000 + doc, 1_800_000_000 + doc,
                     CURRENT_HIERARCHY_SCHEMA_VERSION),
                )
                paragraphs = [
                    (0, "chapter", 1, f"needle alpha doc{doc} " + "x " * (doc % 4)),
                    (1, "body", 8, "alpha elsewhere"),
                    (2, "body", 8, ("needle " * (1 + doc % 3)) + "ending"),
                ]
                for idx, level, rank, body in paragraphs:
                    self.index.db.execute(
                        "INSERT INTO content_paragraph_fts(path,name,paragraph_index,level,level_rank,body) VALUES(?,?,?,?,?,?)",
                        (path, Path(path).name, str(idx), level, rank, body),
                    )

        # Legacy reference: materialize matched paragraphs and apply the former
        # Python occurrence count, hierarchy/date ordering, and snippet slicing.
        with self.index._lock:
            rows = self.index.db.execute("""
                SELECT path, CAST(paragraph_index AS INTEGER) AS paragraph_index, level, level_rank, body,
                       bm25(content_paragraph_fts) AS bm25_rank
                FROM content_paragraph_fts WHERE content_paragraph_fts MATCH ?
                ORDER BY path, CAST(paragraph_index AS INTEGER)
            """, ('"needle" OR "alpha"',)).fetchall()
        grouped = {}
        for row in rows:
            grouped.setdefault(row["path"], []).append(row)
        terms = ["needle", "alpha"]
        expected = []
        for path, paras in grouped.items():
            joined = "\n".join(p["body"] or "" for p in paras)
            folded = joined.casefold()
            if any(term not in folded for term in terms):
                continue
            counts = {term: folded.count(term) for term in terms}
            best = min((p for p in paras if any(t in (p["body"] or "").casefold() for t in terms)),
                       key=lambda p: (int(p["level_rank"] or 8), p["paragraph_index"]))
            expected.append({
                "path": path,
                "total_matches": sum(counts.values()),
                "_best_level_rank": int(best["level_rank"] or 8),
                "_document_date": 1_800_000_000 + int(path[-6:-4]),
                "_bm25_score": min(float(p["bm25_rank"] or 0) for p in paras),
                "body": best["body"],
            })
        expected.sort(key=lambda x: (x["_best_level_rank"], -x["total_matches"], 0,
                                     -x["_document_date"], x["_bm25_score"], x["path"].casefold()))
        expected = expected[:20]
        for item in expected:
            body = item.pop("body")
            start = max(0, body.casefold().find("needle") - 70)
            item["snippet"] = body[start:min(len(body), body.casefold().find("needle") + 180)]

        actual = self.index.search("needle alpha", roots=[str(self.root)], limit=20)
        self.assertEqual([r["path"] for r in actual], [r["path"] for r in expected])
        self.assertEqual([r["total_matches"] for r in actual], [r["total_matches"] for r in expected])
        self.assertEqual([r["snippet"] for r in actual], [r["snippet"] for r in expected])

    def test_keyword_search_uses_bm25_and_limits_locations(self):
        weak = self.root / "a_weak.txt"
        strong = self.root / "z_strong.txt"
        weak.write_text("순위검색", encoding="utf-8")
        strong.write_text("순위검색 " + "잡음 " * 100, encoding="utf-8")
        self.index.index_file(weak); self.index.index_file(strong)
        found = self.index.keyword_search("순위검색")
        self.assertEqual(found[0]["name"], weak.name)
        many = self.root / "many.txt"
        many.write_text("\n".join(["세위치검색 문단"] * 5), encoding="utf-8")
        self.index.index_file(many)
        result = next(row for row in self.index.keyword_search("세위치검색") if row["name"] == many.name)
        self.assertLessEqual(len(result["locations"]), 3)

    def test_keyword_search_adds_breadcrumbs_only_for_selected_locations(self):
        path = self.root / "breadcrumb.txt"
        raw = str(path.resolve())
        with self.index._lock, self.index.db:
            self.index.db.execute(
                "INSERT INTO content_meta(path,name,extension,mtime,ctime,hierarchy_schema_version)"
                " VALUES(?,?,?,?,?,?)", (raw, path.name, "txt", 1.0, 1.0, CURRENT_HIERARCHY_SCHEMA_VERSION))
            self.index.db.execute(
                "INSERT INTO content_paragraph_fts(path,name,paragraph_index,level,level_rank,body,location_label)"
                " VALUES(?,?,?,?,?,?,?)", (raw, path.name, 0, "chapter", 1, "제1장 경로", "문단 1"))
            self.index.db.execute(
                "INSERT INTO content_paragraph_fts(path,name,paragraph_index,level,level_rank,body,location_label)"
                " VALUES(?,?,?,?,?,?,?)", (raw, path.name, 1, "body", 8, "경로검색 본문", "문단 2"))
            self.index.db.execute(
                "INSERT INTO content_paragraph_fts(path,name,paragraph_index,level,level_rank,body,location_label)"
                " VALUES(?,?,?,?,?,?,?)", (raw, path.name, 2, "section", 2, "제1절 다른 경로", "문단 3"))
            self.index.db.execute(
                "INSERT INTO content_paragraph_fts(path,name,paragraph_index,level,level_rank,body,location_label)"
                " VALUES(?,?,?,?,?,?,?)", (raw, path.name, 3, "body", 8, "경로검색 두번째 위치", "문단 4"))

        result = self.index.keyword_search("경로검색", limit=1, offset=0)[0]
        locations = result["locations"]
        self.assertEqual(locations[0]["breadcrumb"], ["제1장 경로"])
        self.assertEqual(locations[0]["breadcrumb_text"], "제1장 경로")
        self.assertEqual(locations[1]["breadcrumb"], ["제1장 경로", "제1절 다른 경로"])
        self.assertEqual(result["breadcrumb"], locations[0]["breadcrumb"])
        self.assertEqual(result["breadcrumb_text"], locations[0]["breadcrumb_text"])

    def test_keyword_filename_field_weight_three_changes_rank(self):
        named = self.root / "가중치검색.txt"
        body = self.root / "other.txt"
        named.write_text("가중치검색", encoding="utf-8")
        body.write_text("가중치검색 " + "잡음 " * 300, encoding="utf-8")
        self.index.index_file(named); self.index.index_file(body)
        self.assertEqual(self.index.keyword_search("가중치검색")[0]["name"], named.name)

    def test_keyword_hierarchy_boost_changes_rank(self):
        heading = self.root / "heading.hwpx"
        body = self.root / "body.txt"
        write_hwpx_paragraphs(heading, [{"text": "위계점수검색", "style_name": "제목", "outline_level": 0}])
        body.write_text("위계점수검색 " + "잡음 " * 300, encoding="utf-8")
        self.index.index_file(heading); self.index.index_file(body)
        self.assertEqual(self.index.keyword_search("위계점수검색")[0]["name"], heading.name)

    def test_search_optimized_paragraph_query_matches_original_query(self):
        """Required-column/schema-limited query preserves the prior result rows."""
        docs = [
            ("/synthetic/current-a.txt", "current-a.txt", 1, "chapter", 1, "alpha needle beta"),
            ("/synthetic/current-a.txt", "current-a.txt", 2, "body", 8, "needle tail"),
            ("/synthetic/current-b.txt", "current-b.txt", 0, "section", 2, "beta alpha"),
            ("/synthetic/legacy.txt", "legacy.txt", 0, "body", 8, "alpha needle beta"),
            ("/synthetic/unmatched.txt", "unmatched.txt", 0, "body", 8, "unrelated text"),
        ]
        with self.index._lock, self.index.db:
            self.index.db.executemany(
                "INSERT INTO content_paragraph_fts(path,name,paragraph_index,level,level_rank,body) VALUES(?,?,?,?,?,?)",
                docs,
            )
            self.index.db.executemany(
                "INSERT INTO content_meta(path,size,mtime,ctime,hierarchy_schema_version) VALUES(?,?,?,?,?)",
                [
                    ("/synthetic/current-a.txt", 1, 1.0, 1.0, CURRENT_HIERARCHY_SCHEMA_VERSION),
                    ("/synthetic/current-b.txt", 1, 1.0, 1.0, CURRENT_HIERARCHY_SCHEMA_VERSION),
                    ("/synthetic/legacy.txt", 1, 1.0, 1.0, 0),
                    ("/synthetic/unmatched.txt", 1, 1.0, 1.0, CURRENT_HIERARCHY_SCHEMA_VERSION),
                ],
            )

        expression = '"alpha" OR "needle"'
        with self.index._lock:
            original_rows = self.index.db.execute(
                """
                SELECT path, name, paragraph_index, level, level_rank, body,
                       bm25(content_paragraph_fts) AS bm25_rank
                FROM content_paragraph_fts
                WHERE content_paragraph_fts MATCH ?
                """,
                (expression,),
            ).fetchall()
            optimized_rows = self.index.db.execute(
                """
                SELECT path, paragraph_index, level, level_rank, body,
                       bm25(content_paragraph_fts) AS bm25_rank
                FROM content_paragraph_fts
                WHERE content_paragraph_fts MATCH ?
                  AND path IN (SELECT path FROM content_meta WHERE hierarchy_schema_version >= ?)
                ORDER BY path, CAST(paragraph_index AS INTEGER)
                """,
                (expression, CURRENT_HIERARCHY_SCHEMA_VERSION),
            ).fetchall()

        fields = ("path", "paragraph_index", "level", "level_rank", "body", "bm25_rank")
        expected = sorted(
            (tuple(row[key] for key in fields) for row in original_rows
             if row["path"] != "/synthetic/legacy.txt"),
            key=lambda row: (row[0], int(row[1])),
        )
        actual = sorted(
            (tuple(row[key] for key in fields) for row in optimized_rows),
            key=lambda row: (row[0], int(row[1])),
        )
        self.assertEqual(expected, actual)
        self.assertTrue(original_rows)
        self.assertTrue(all("name" not in row.keys() for row in optimized_rows))

    def test_keyword_proximity_bonus_changes_rank(self):
        near = self.root / "near.txt"
        far = self.root / "far.txt"
        near.write_text("근접검색 알파 베타", encoding="utf-8")
        far.write_text("근접검색 알파 " + "잡음 " * 30 + "베타", encoding="utf-8")
        self.index.index_file(near); self.index.index_file(far)
        self.assertEqual(self.index.keyword_search("알파 베타")[0]["name"], near.name)

    def test_keyword_phrase_bonus_changes_rank(self):
        phrase = self.root / "phrase.txt"
        scattered = self.root / "scattered.txt"
        phrase.write_text("구문가산검색 알파 베타", encoding="utf-8")
        scattered.write_text("구문가산검색 알파 " + "잡음 " * 40 + "베타", encoding="utf-8")
        self.index.index_file(phrase); self.index.index_file(scattered)
        self.assertEqual(self.index.keyword_search('"알파 베타"')[0]["name"], phrase.name)

    def test_keyword_two_character_search_3000_documents_and_record_timings(self):
        with self.index._lock, self.index.db:
            rows = [(f"{self.root}/short{i}.txt", f"short{i}.txt", 0, "body", 8, f"문서 {i} 공통어 검색 대상") for i in range(3000)]
            self.index.db.executemany(
                "INSERT INTO content_paragraph_fts(path,name,paragraph_index,level,level_rank,body,location_label,section) VALUES(?,?,?,?,?,?,?,?)",
                [(*row, f"문단 1", None) for row in rows])
            self.index.db.executemany("INSERT INTO content_fts(path,name,body) VALUES(?,?,?)",
                                      [(row[0], row[1], row[5]) for row in rows])
            self.index.db.executemany(
                "INSERT INTO content_meta(path,size,mtime,ctime,hierarchy_schema_version) VALUES(?,?,?,?,?)",
                [(row[0], 1, 1.0, 1.0, CURRENT_HIERARCHY_SCHEMA_VERSION) for row in rows])
            self.index.db.executemany("INSERT INTO content_short_grams(gram,path) VALUES(?,?)",
                                      [(gram, row[0]) for row in rows
                                       for gram in {row[5][i:i + 2].casefold() for i in range(len(row[5]) - 1)}])
        start = time.perf_counter()
        self.index.db.execute("SELECT DISTINCT path FROM content_paragraph_fts WHERE body LIKE ?", ("%검색%",)).fetchall()
        before_seconds = time.perf_counter() - start
        start = time.perf_counter()
        self.index.db.execute("SELECT path FROM content_short_grams WHERE gram=?", ("검색",)).fetchall()
        gram_seconds = time.perf_counter() - start
        start = time.perf_counter()
        result = self.index.keyword_search("검색", limit=100)
        after_seconds = time.perf_counter() - start
        print(f"\n[성능 측정] 2글자 3,000건 전체검색={after_seconds:.4f}초 / 전 LIKE 후보={before_seconds:.4f}초 / 후 그램 후보={gram_seconds:.4f}초", flush=True)
        self.assertEqual(len(result), 100)
        self.assertLess(gram_seconds, before_seconds)
        self.assertLess(after_seconds, 2.5)

    def _insert_keyword_documents(self, documents):
        """(이름, [문단…]) 목록을 색인 표에 직접 넣는다. 넣은 경로 목록을 돌려준다."""
        paths = []
        with self.index._lock, self.index.db:
            for name, paragraphs in documents:
                path = f"{self.root}/{name}"
                paths.append(path)
                body = "\n".join(paragraphs)
                self.index.db.executemany(
                    "INSERT INTO content_paragraph_fts(path,name,paragraph_index,level,level_rank,body,location_label,section) VALUES(?,?,?,?,?,?,?,?)",
                    [(path, name, i, "body", 8, text, f"문단 {i + 1}", None) for i, text in enumerate(paragraphs)])
                self.index.db.execute("INSERT INTO content_fts(path,name,body) VALUES(?,?,?)", (path, name, body))
                self.index.db.execute(
                    "INSERT INTO content_meta(path,size,mtime,ctime,name,extension,hierarchy_schema_version) VALUES(?,?,?,?,?,?,?)",
                    (path, 1, 1.0, 1.0, name, "txt", CURRENT_HIERARCHY_SCHEMA_VERSION))
                self.index.db.executemany(
                    "INSERT OR IGNORE INTO content_short_grams(gram,path) VALUES(?,?)",
                    [(gram, path) for gram in {body[i:i + 2].casefold() for i in range(len(body) - 1)}])
        return paths

    def test_keyword_two_character_term_found_when_only_in_first_paragraph(self):
        # 2글자 검색어가 마지막 문단이 아닌 곳에만 있어도 문서가 빠지면 안 된다.
        self._insert_keyword_documents(
            [(f"first{i}.txt", [f"감사 결과 보고 {i}", "둘째 문단 내용", "마지막 문단 내용"]) for i in range(30)])
        found = self.index.keyword_search("감사")
        self.assertEqual(len(found), 30)
        self.assertTrue(all(row["locations"] and row["locations"][0]["paragraph_index"] == 0 for row in found))
        # 두 검색어가 서로 다른 문단에 있으면 예전처럼 결과에 넣지 않는다(같은 문단 조건 유지).
        self.assertEqual(self.index.keyword_search("감사 둘째"), [])

    def test_keyword_search_survives_more_candidates_than_sql_variable_limit(self):
        # 후보 문서가 SQL 변수 한도(기본 32,766)보다 많아도 실패하지 않는다.
        count = 40000
        self._insert_keyword_documents([(f"many{i}.txt", ["한도초과검색어 문단"]) for i in range(count)])
        found = self.index.keyword_search("한도초과검색어", limit=5)
        self.assertEqual(len(found), 5)
        short = self.index.keyword_search("문단", limit=5)
        self.assertEqual(len(short), 5)

    def test_keyword_locations_use_each_documents_own_matching_paragraphs(self):
        # 문서마다 검색어가 있는 문단 번호가 달라도 각 문서의 위치(발췌문)가 나와야 한다.
        self._insert_keyword_documents([
            ("loc_a.txt", ["머리말", "둘째 문단", "위치시험낱말 셋째"]),
            ("loc_b.txt", ["위치시험낱말 첫째", "둘째 문단"]),
            ("loc_c.txt", ["머리말", "위치시험낱말 둘째", "꼬리말"]),
        ])
        found = {row["name"]: row for row in self.index.keyword_search("위치시험낱말")}
        self.assertEqual(sorted(found), ["loc_a.txt", "loc_b.txt", "loc_c.txt"])
        self.assertEqual({name: [loc["paragraph_index"] for loc in row["locations"]] for name, row in found.items()},
                         {"loc_a.txt": [2], "loc_b.txt": [0], "loc_c.txt": [1]})
        self.assertTrue(all("위치시험낱말" in row["snippet"] for row in found.values()))

    def test_keyword_paging_reuses_ranking_and_drops_it_when_index_changes(self):
        self._insert_keyword_documents([(f"page{i:03d}.txt", [f"쪽넘김시험낱말 본문 {i}"]) for i in range(30)])
        whole = self.index.keyword_search("쪽넘김시험낱말", limit=30)
        first = self.index.keyword_search("쪽넘김시험낱말", limit=10)
        ranked_object = self.index._keyword_rank_cache[3]
        second = self.index.keyword_search("쪽넘김시험낱말", limit=10, offset=10)
        third = self.index.keyword_search("쪽넘김시험낱말", limit=10, offset=20)
        # 다음 쪽은 보관한 순위 목록을 그대로 쓴다(같은 객체).
        self.assertIs(self.index._keyword_rank_cache[3], ranked_object)
        strip = lambda rows: [(row["name"], row["snippet"], row["score"]) for row in rows]
        self.assertEqual(strip(first + second + third), strip(whole))
        self.assertEqual([row["relevance_rank"] for row in second], list(range(11, 21)))
        # 다른 검색어·범위는 섞이지 않는다.
        self.assertEqual(self.index.keyword_search("없는낱말없는낱말"), [])
        self.assertEqual(self.index.keyword_search("쪽넘김시험낱말", roots=[str(self.root / "elsewhere")]), [])
        # 색인이 바뀌면 보관한 목록을 버리고 새 문서가 보인다.
        self._insert_keyword_documents([("page_new.txt", ["쪽넘김시험낱말 새 문서"])])
        self.assertEqual(len(self.index.keyword_search("쪽넘김시험낱말", limit=100)), 31)
        extra = self.root / "real.txt"
        extra.write_text("쪽넘김시험낱말 실제 파일", encoding="utf-8")
        self.index.index_file(extra)
        self.assertEqual(len(self.index.keyword_search("쪽넘김시험낱말", limit=100)), 32)
        self.index.remove_file(extra)
        self.assertEqual(len(self.index.keyword_search("쪽넘김시험낱말", limit=100)), 31)

    def test_keyword_exclusion_terms_of_any_length_remove_documents(self):
        self._insert_keyword_documents([
            ("keep.txt", ["제외시험낱말 본문", "다른 문단"]),
            ("drop_long.txt", ["제외시험낱말 본문", "빼야할긴말 포함"]),
            ("drop_short.txt", ["제외시험낱말 본문", "여기 감사 포함"]),
        ])
        names = lambda query: sorted(row["name"] for row in self.index.keyword_search(query))
        self.assertEqual(names("제외시험낱말"), ["drop_long.txt", "drop_short.txt", "keep.txt"])
        self.assertEqual(names("제외시험낱말 -빼야할긴말"), ["drop_short.txt", "keep.txt"])
        self.assertEqual(names("제외시험낱말 -감사"), ["drop_long.txt", "keep.txt"])
        self.assertEqual(names("제외시험낱말 -빼야할긴말 -감사"), ["keep.txt"])

    def test_hierarchy_two_character_search_same_with_and_without_short_grams(self):
        # 2글자 위계검색은 조각 표로 후보를 좁혀도 결과·발췌문이 LIKE 전체 훑기와 같아야 한다.
        documents = [(f"gram{i}.txt", [f"머리말 {i}", f"올해 예산 편성 {i}" if i % 3 else "해당 없음", "예산 끝" if i % 5 == 0 else "끝"])
                     for i in range(40)]
        self._insert_keyword_documents(documents)

        def summary(rows):
            return [(row["name"], row["snippet"], row["total_matches"], row["best_level"], row["relevance_rank"]) for row in rows]

        with_grams = summary(self.index.search("예산", limit=500))
        expected_names = {name for name, paragraphs in documents if any("예산" in text for text in paragraphs)}
        self.assertEqual({row[0] for row in with_grams}, expected_names)
        self.assertTrue(all("예산" in row[1] for row in with_grams))
        # 조각 표가 비어 있는 옛 색인(조각 표 도입 전)에서는 LIKE 경로로 같은 결과를 낸다.
        with self.index._lock, self.index.db:
            self.index.db.execute("DELETE FROM content_short_grams")
        self.assertEqual(summary(self.index.search("예산", limit=500)), with_grams)

    def test_keyword_query_syntax_filters_and_extension_aliases(self):
        root = self.root / "폴더"
        root.mkdir()
        first = root / "first.hwpx"
        second = root / "second.txt"
        first.write_text('2026-10-20 "정확 구문" 한국어조사 처리 제외어', encoding="utf-8")
        second.write_text('2026-10-21 "정확 구문" 한국어조사 처리 다른말', encoding="utf-8")
        self.index.index_file(first); self.index.index_file(second)
        self.assertEqual([x["name"] for x in self.index.keyword_search('"정확 구문" -제외어')], ["second.txt"])
        self.assertEqual(len(self.index.keyword_search("한국어조사")), 1)
        self.assertEqual([x["name"] for x in self.index.keyword_search('정확 ext:txt')], ["second.txt"])
        self.assertEqual(len(self.index.keyword_search('"정확 구문" path:폴더')), 1)
        self.assertEqual(len(self.index.keyword_search('"정확 구문" after:2026-10-01 before:2026-10-30')), 1)
        self.assertEqual(len(self.index.keyword_search('정확 처리 ~5')), 1)
        self.assertEqual(len(self.index.keyword_search('"정확 구문" ext:txt')), 1)

    def test_keyword_results_keep_sheet_row_location_badge(self):
        path = self.root / "sheet.xlsx"
        with ZipFile(path, "w") as archive:
            archive.writestr("xl/workbook.xml", '<workbook xmlns:r="urn:r"><sheets><sheet name="매출" r:id="rId1"/></sheets></workbook>')
            archive.writestr("xl/_rels/workbook.xml.rels", '<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>')
            archive.writestr("xl/worksheets/sheet1.xml", '<worksheet><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>제목</t></is></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>시트행검색 배지</t></is></c></row></sheetData></worksheet>')
        self.index.index_file(path)
        result = self.index.keyword_search("시트행검색")[0]
        self.assertEqual(result["locations"][0]["position"]["sheet"], "매출")
        self.assertEqual(result["locations"][0]["position"]["row"], 2)

    def test_keyword_results_keep_pdf_page_badge(self):
        path = self.root / "pages.pdf"
        from unittest.mock import patch
        from docfit_core.timemachine.extractors import ExtractionRecord
        records = [ExtractionRecord("둘째쪽 페이지검색 배지", "page", "2쪽", 2)]
        with patch("docfit_core.timemachine.content_search.extract_document", return_value=records):
            path.write_bytes(b"pdf stub")
            self.index.index_file(path)
        result = self.index.keyword_search("페이지검색")[0]
        self.assertEqual(result["locations"][0]["position"]["page"], 2)

    def test_match_counts_phrases_exclusions_casefold_and_mentions_sort(self):
        low = self.root / "low.txt"
        high = self.root / "high.txt"
        low.write_text('예산이 집행되고 Annual Plan annual plan', encoding="utf-8")
        high.write_text('예산 예산을 집행 집행했다 annual plan Annual Plan', encoding="utf-8")
        self.index.index_file(low); self.index.index_file(high)
        found = self.index.search('예산 집행 "Annual Plan" -없는말', sort="mentions")
        self.assertEqual(found[0]["name"], high.name)
        item = next(row for row in found if row["name"] == low.name)
        self.assertEqual(item["match_counts"], {"예산": 1, "집행": 1, "Annual Plan": 2})
        self.assertEqual(item["total_matches"], 4)

    def test_legacy_schema_migrates_without_losing_documents(self):
        path = self.root / "legacy.sqlite"
        db = sqlite3.connect(path)
        db.executescript("""
            CREATE TABLE content_meta(path TEXT PRIMARY KEY,size INTEGER,mtime REAL);
            CREATE VIRTUAL TABLE content_fts USING fts5(path UNINDEXED,name UNINDEXED,body,tokenize='unicode61');
        """)
        old_file = self.root / "old.txt"
        old_file.write_text("한글 검색 이전", encoding="utf-8")
        db.execute("INSERT INTO content_meta VALUES(?,1,1)", (str(old_file),))
        db.execute("INSERT INTO content_fts VALUES(?,'old.txt','한글 검색 이전')", (str(old_file),))
        db.commit()
        db.close()
        migrated = ContentSearchIndex(path)
        # 구형 색인 상태이므로 재색인 필요 카운트 1, 검색 결과에는 미노출
        self.assertEqual(migrated.needs_reindex_count(), 1)
        self.assertEqual(migrated.search("검색"), [])
        # 재색인 수행 후 정상 검색
        self.assertTrue(migrated.index_file(old_file)["indexed"])
        self.assertEqual(migrated.needs_reindex_count(), 0)
        self.assertEqual(migrated.search("검색")[0]["name"], "old.txt")
        migrated.close()

    def test_hierarchy_order_title_once_beats_square_five_and_body_ten(self):
        # 문서A.hwpx: 제목 표식 문단에 query 1회
        doc_a = self.root / "A_title.hwpx"
        write_hwpx_paragraphs(doc_a, [
            {"text": "핵심과제 추진계획", "style_name": "제목", "outline_level": 0},
            {"text": "일반 설명 문장입니다.", "style_name": "바탕글"},
        ])
        # 문서B.hwpx: ㅁ 문단에 query 5회
        doc_b = self.root / "B_square.hwpx"
        write_hwpx_paragraphs(doc_b, [
            {"text": "ㅁ 핵심과제 핵심과제 핵심과제 핵심과제 핵심과제 추진", "style_name": "바탕글"},
        ])
        # 문서C.txt: 일반 본문에 query 10회
        doc_c = self.root / "C_body.txt"
        doc_c.write_text(" ".join(["핵심과제"] * 10), encoding="utf-8")

        self.index.index_file(doc_a)
        self.index.index_file(doc_b)
        self.index.index_file(doc_c)

        found = self.index.search("핵심과제")
        self.assertEqual(len(found), 3)
        self.assertEqual(found[0]["name"], doc_a.name)
        self.assertEqual(found[0]["best_level"], "문서 제목")
        self.assertEqual(found[0]["total_matches"], 1)

        self.assertEqual(found[1]["name"], doc_b.name)
        self.assertEqual(found[1]["best_level"], "ㅁ")
        self.assertEqual(found[1]["total_matches"], 5)

        self.assertEqual(found[2]["name"], doc_c.name)
        self.assertEqual(found[2]["best_level"], "본문")
        self.assertEqual(found[2]["total_matches"], 10)

    def test_hierarchy_order_same_level_more_mentions_first(self):
        doc1 = self.root / "m1.txt"
        doc2 = self.root / "m2.txt"
        doc1.write_text("동일위계 검색어 단독 출현", encoding="utf-8")
        doc2.write_text("동일위계 검색어 검색어 2회 출현", encoding="utf-8")
        self.index.index_file(doc1)
        self.index.index_file(doc2)
        found = self.index.search("검색어")
        self.assertEqual(found[0]["name"], doc2.name)
        self.assertEqual(found[1]["name"], doc1.name)

    def test_table_cell_two_mentions_beats_body_one_at_same_lowest_level(self):
        table_doc = self.root / "table.hwpx"
        body_doc = self.root / "body.hwpx"
        write_hwpx_paragraphs(table_doc, [
            {"text": "표순위검색 표순위검색", "is_table_cell": True},
        ])
        write_hwpx_paragraphs(body_doc, [
            {"text": "표순위검색", "style_name": "바탕글"},
        ])
        self.index.index_file(table_doc)
        self.index.index_file(body_doc)

        found = self.index.search("표순위검색")
        self.assertEqual([item["name"] for item in found], [table_doc.name, body_doc.name])
        self.assertEqual([item["best_level"] for item in found], ["표 칸", "본문"])
        self.assertEqual([item["total_matches"] for item in found], [2, 1])

    def test_hierarchy_order_same_level_same_mentions_newer_document_first(self):
        p_old = self.root / "z_older.txt"
        p_new = self.root / "a_newer.txt"
        p_old.write_text("날짜 동률 검사 키워드", encoding="utf-8")
        p_new.write_text("날짜 동률 검사 키워드", encoding="utf-8")
        self.index.index_file(p_old)
        self.index.index_file(p_new)

        # DB fixture로 안정된 timestamp 설정
        # p_old: ctime=10.0, mtime=20.0 -> max=20.0
        # p_new: ctime=30.0, mtime=15.0 -> max=30.0 (생성일이 미래인 경우도 검증)
        with self.index._lock, self.index.db:
            self.index.db.execute("UPDATE content_meta SET ctime=10.0, mtime=20.0 WHERE path=?", (str(p_old.resolve()),))
            self.index.db.execute("UPDATE content_meta SET ctime=30.0, mtime=15.0 WHERE path=?", (str(p_new.resolve()),))

        found = self.index.search("키워드")
        self.assertEqual(len(found), 2)
        self.assertEqual(found[0]["name"], p_new.name)
        self.assertEqual(found[1]["name"], p_old.name)

    def test_hierarchy_order_bm25_is_fourth_tiebreaker(self):
        # 위계, 횟수, 날짜가 완전히 동률인 두 문서
        # 한 문서는 검색어가 집중된 짧은 문맥, 다른 문서는 다량의 잡음 단어 포함 문맥
        p_dense = self.root / "z_dense.txt"
        p_sparse = self.root / "a_sparse.txt"
        p_dense.write_text("알고리즘분석", encoding="utf-8")
        p_sparse.write_text("알고리즘분석 " + " ".join([f"잡음단어{i}" for i in range(50)]), encoding="utf-8")
        self.index.index_file(p_dense)
        self.index.index_file(p_sparse)

        with self.index._lock, self.index.db:
            self.index.db.execute("UPDATE content_meta SET ctime=100.0, mtime=100.0 WHERE path=?", (str(p_dense.resolve()),))
            self.index.db.execute("UPDATE content_meta SET ctime=100.0, mtime=100.0 WHERE path=?", (str(p_sparse.resolve()),))

        found = self.index.search("알고리즘분석")
        self.assertEqual(len(found), 2)
        # z_dense가 파일명 알파벳 순서상 뒤이지만 BM25 밀도가 높아 우선해야 함
        self.assertEqual(found[0]["name"], p_dense.name)

    def test_best_level_total_matches_and_relevance_rank_are_returned(self):
        path = self.root / "doc.hwpx"
        write_hwpx_paragraphs(path, [
            {"text": "제1장 총칙", "style_name": "개요 1", "outline_level": 1},
            {"text": "총칙 세부 내용 총칙", "style_name": "바탕글"},
        ])
        self.index.index_file(path)
        found = self.index.search("총칙")
        self.assertEqual(len(found), 1)
        item = found[0]
        self.assertEqual(item["best_level"], "장")
        self.assertEqual(item["total_matches"], 3)
        self.assertEqual(item["relevance_rank"], 1)

    def test_heading_markers_reuse_existing_classifier(self):
        path = self.root / "headings.hwpx"
        paras = [
            {"text": "Ⅰ. 장", "outline_level": 1},
            {"text": "1. 절", "outline_level": 2},
            {"text": "가. 주제", "outline_level": 3},
            {"text": "ㅁ 항목", "style_name": "바탕글"},
            {"text": "ㅇ 항목", "style_name": "바탕글"},
            {"text": "- 항목", "style_name": "바탕글"},
            {"text": "* 참고", "style_name": "바탕글"},
            {"text": "일반 본문 문장입니다.", "style_name": "바탕글"},
            {"text": "표 내부 텍스트", "is_table_cell": True},
        ]
        write_hwpx_paragraphs(path, paras)
        records = extract_paragraphs(path)
        levels = [r.level for r in records]
        expected = ["chapter", "section", "topic", "square", "circle", "dash", "asterisk", "body", "table_cell"]
        self.assertEqual(levels, expected)

    def test_default_hierarchy_sort_and_explicit_mentions_sort(self):
        doc_sq = self.root / "square.hwpx"
        write_hwpx_paragraphs(doc_sq, [{"text": "ㅁ 연관도키워드"}])
        doc_body = self.root / "body.txt"
        doc_body.write_text("연관도키워드 연관도키워드 연관도키워드", encoding="utf-8")
        self.index.index_file(doc_sq)
        self.index.index_file(doc_body)

        default_res = self.index.search("연관도키워드")
        self.assertEqual(default_res[0]["name"], doc_sq.name)

        mentions_res = self.index.search("연관도키워드", sort="mentions")
        self.assertEqual(mentions_res[0]["name"], doc_body.name)

    def test_hwpx_outline_level_wins_over_plain_text_role(self):
        path = self.root / "outline_win.hwpx"
        write_hwpx_paragraphs(path, [
            {"text": "기호 없는 문서 대제목", "style_name": "제목", "outline_level": 0},
            {"text": "일반 본문", "style_name": "바탕글"},
        ])
        records = extract_paragraphs(path)
        self.assertEqual(records[0].level, "document_title")

    def test_hwpx_chapter_and_section_markers_map_to_distinct_levels(self):
        path = self.root / "chap_sec.hwpx"
        write_hwpx_paragraphs(path, [
            {"text": "제1장 운영 지침"},
            {"text": "1. 절 세부 사항", "outline_level": 2},
        ])
        records = extract_paragraphs(path)
        self.assertEqual(records[0].level, "chapter")
        self.assertEqual(records[1].level, "section")

    def test_square_marker_aliases_share_square_level(self):
        path = self.root / "squares.txt"
        path.write_text("ㅁ 첫째 항목\n□ 둘째 항목", encoding="utf-8")
        records = extract_paragraphs(path)
        self.assertEqual(records[0].level, "square")
        self.assertEqual(records[1].level, "square")

    def test_circle_marker_aliases_share_circle_level(self):
        path = self.root / "circles.txt"
        path.write_text("ㅇ 첫째 원\n○ 둘째 원", encoding="utf-8")
        records = extract_paragraphs(path)
        self.assertEqual(records[0].level, "circle")
        self.assertEqual(records[1].level, "circle")

    def test_dash_and_asterisk_are_below_circle(self):
        c_rank = LEVEL_ORDER.index("circle")
        d_rank = LEVEL_ORDER.index("dash")
        a_rank = LEVEL_ORDER.index("asterisk")
        self.assertLess(c_rank, d_rank)
        self.assertLess(d_rank, a_rank)

    def test_unknown_or_unmarked_paragraph_is_body(self):
        path = self.root / "plain.txt"
        path.write_text("2026. 10. 20. 날짜로 시작하는 일반 문장\n1. 문맥 없는 단독 번호 문장", encoding="utf-8")
        records = extract_paragraphs(path)
        self.assertEqual(records[0].level, "body")
        self.assertEqual(records[0].level_rank, records[1].level_rank)
        self.assertEqual(records[1].level, "body")

    def test_markdown_and_txt_heading_hierarchy_uses_document_context(self):
        cases = {
            "one.md": ("# 단일 제목\n## 절\n### 주제\n", ["document_title", "chapter", "section"]),
            "many.md": ("# 계획 제목\n# Ⅰ. 시설물 정비\n## 관광안내판 철거\n### 대상 위치\n",
                        ["document_title", "chapter", "section", "topic"]),
            "same.txt": ("# 시설물 정비\n## 관광안내판 철거\n### 대상 위치\n",
                         ["document_title", "chapter", "section"]),
        }
        for name, (content, expected) in cases.items():
            with self.subTest(name=name):
                path = self.root / name
                path.write_text(content, encoding="utf-8")
                self.assertEqual([item.level for item in extract_paragraphs(path)], expected)

    def test_docx_heading_styles_are_resolved_from_styles_part(self):
        path = self.root / "styled.docx"
        with ZipFile(path, "w") as archive:
            archive.writestr("word/document.xml", """<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
              <w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>시설물 정비</w:t></w:r></w:p>
              <w:p><w:pPr><w:pStyle w:val="Heading2"/></w:pPr><w:r><w:t>관광안내판 철거</w:t></w:r></w:p>
              <w:p><w:pPr><w:pStyle w:val="Heading3"/></w:pPr><w:r><w:t>대상 위치</w:t></w:r></w:p>
              <w:sectPr/></w:body></w:document>""")
            archive.writestr("word/styles.xml", """<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
              <w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/></w:style>
              <w:style w:type="paragraph" w:styleId="Heading2"><w:name w:val="heading 2"/></w:style>
              <w:style w:type="paragraph" w:styleId="Heading3"><w:name w:val="heading 3"/></w:style>
              </w:styles>""")
        records = extract_paragraphs(path)
        self.assertEqual([r.style_name for r in records], ["Heading 1", "Heading 2", "Heading 3"])
        self.assertEqual([r.level for r in records], ["chapter", "section", "topic"])

    def test_numbered_hierarchy_requires_consistent_document_sequence(self):
        path = self.root / "numbered.txt"
        path.write_text("Ⅰ. 총칙\nⅡ. 운영\n1. 적용 범위\n2. 실행 방법\n가. 대상\n나. 담당 부서\n(1) 세부 기준\n(2) 일정\n① 예외\n② 추가 예외\n"
                        "일반 문장 본문\nⅡ. 운영\n  1. 실행 방법\n    가. 담당 부서\n      (1) 일정\n        ② 참고\n",
                        encoding="utf-8")
        records = extract_paragraphs(path)
        levels = [r.level for r in records]
        self.assertEqual(levels[0], "chapter")
        self.assertIn("section", levels)
        self.assertIn("topic", levels)
        self.assertEqual(levels[6:10], ["topic", "topic", "body", "body"])
        self.assertEqual(levels[10], "body")
        self.assertEqual(levels[11], "chapter")
        isolated = self.root / "isolated.txt"
        isolated.write_text("가. 일반 문장처럼 보이는 번호", encoding="utf-8")
        self.assertEqual(extract_paragraphs(isolated)[0].level, "body")

    def test_table_cell_records_keep_table_level_and_order(self):
        path = self.root / "table.hwpx"
        write_hwpx_paragraphs(path, [
            {"text": "본문 첫 문단"},
            {"text": "1. 표 첫째 칸 번호 문단", "is_table_cell": True},
            {"text": "표 내부 제목 스타일 문단", "is_table_cell": True, "style_name": "제목", "outline_level": 0},
            {"text": "본문 둘째 문단"},
        ])
        records = extract_paragraphs(path)
        self.assertEqual(records[0].level, "body")
        self.assertFalse(records[0].is_table_cell)
        self.assertEqual(records[1].level, "table_cell")
        self.assertTrue(records[1].is_table_cell)
        self.assertEqual(records[2].level, "table_cell")
        self.assertTrue(records[2].is_table_cell)
        self.assertEqual(records[3].level, "body")

    def test_new_index_stores_paragraph_level_and_schema_version(self):
        path = self.root / "store_test.txt"
        path.write_text("본문 내용", encoding="utf-8")
        self.index.index_file(path)
        row = self.index.db.execute("SELECT hierarchy_schema_version, ctime FROM content_meta WHERE path=?", (str(path.resolve()),)).fetchone()
        self.assertEqual(row["hierarchy_schema_version"], CURRENT_HIERARCHY_SCHEMA_VERSION)
        self.assertIsNotNone(row["ctime"])

        p_row = self.index.db.execute("SELECT level, level_rank FROM content_paragraph_fts WHERE path=?", (str(path.resolve()),)).fetchone()
        self.assertEqual(p_row["level"], "body")
        self.assertEqual(int(p_row["level_rank"]), LEVEL_ORDER.index("body"))

    def test_index_unchanged_current_schema_is_noop(self):
        path = self.root / "noop.txt"
        path.write_text("고정 내용", encoding="utf-8")
        self.assertTrue(self.index.index_file(path)["indexed"])
        self.assertFalse(self.index.index_file(path)["indexed"])

        with self.index._lock, self.index.db:
            self.index.db.execute("UPDATE content_meta SET hierarchy_schema_version=0 WHERE path=?", (str(path.resolve()),))

        self.assertTrue(self.index.index_file(path)["indexed"])

    def test_reindex_replaces_all_rows_atomically(self):
        path = self.root / "atomic.txt"
        path.write_text("줄 1\n줄 2\n줄 3", encoding="utf-8")
        self.index.index_file(path)
        count1 = self.index.db.execute("SELECT COUNT(*) FROM content_paragraph_fts WHERE path=?", (str(path.resolve()),)).fetchone()[0]
        self.assertEqual(count1, 3)

        path.write_text("단일 줄", encoding="utf-8")
        self.index.index_file(path)
        count2 = self.index.db.execute("SELECT COUNT(*) FROM content_paragraph_fts WHERE path=?", (str(path.resolve()),)).fetchone()[0]
        self.assertEqual(count2, 1)

    def test_remove_file_clears_paragraph_and_legacy_rows(self):
        path = self.root / "remove_me.txt"
        path.write_text("지울 내용", encoding="utf-8")
        self.index.index_file(path)
        self.index.remove_file(path)
        c_p = self.index.db.execute("SELECT COUNT(*) FROM content_paragraph_fts WHERE path=?", (str(path.resolve()),)).fetchone()[0]
        c_f = self.index.db.execute("SELECT COUNT(*) FROM content_fts WHERE path=?", (str(path.resolve()),)).fetchone()[0]
        c_m = self.index.db.execute("SELECT COUNT(*) FROM content_meta WHERE path=?", (str(path.resolve()),)).fetchone()[0]
        self.assertEqual(c_p, 0)
        self.assertEqual(c_f, 0)
        self.assertEqual(c_m, 0)

    def test_full_body_counts_not_limited_to_snippet(self):
        path = self.root / "long.txt"
        # 1번째 줄에 1회, 100번째 줄에 2회 -> 총 3회
        lines = ["키워드 첫 문맥"] + ["일반 줄"] * 98 + ["키워드 반복 키워드 끝 문맥"]
        path.write_text("\n".join(lines), encoding="utf-8")
        self.index.index_file(path)
        found = self.index.search("키워드")
        self.assertEqual(found[0]["total_matches"], 3)
        self.assertIn("첫 문맥", found[0]["snippet"])

    def test_multiple_terms_count_by_term_and_total(self):
        path = self.root / "multi.txt"
        # 예산 2회, 집행 3회
        lines = [
            "예산 예산 관련 문장",
            "집행 집행 집행 관련 문장",
        ]
        path.write_text("\n".join(lines), encoding="utf-8")
        self.index.index_file(path)
        found = self.index.search("예산 집행")
        self.assertEqual(len(found), 1)
        self.assertEqual(found[0]["match_counts"], {"예산": 2, "집행": 3})
        self.assertEqual(found[0]["total_matches"], 5)

    def test_excluded_terms_do_not_contribute_to_count_or_best_level(self):
        path = self.root / "exclude.hwpx"
        write_hwpx_paragraphs(path, [
            {"text": "제1장 제외키워드 포함", "style_name": "개요 1", "outline_level": 1},
            {"text": "본문에만 양성키워드 존재", "style_name": "바탕글"},
        ])
        self.index.index_file(path)
        found = self.index.search("양성키워드 -다른제외어")
        self.assertEqual(len(found), 1)
        self.assertEqual(found[0]["best_level"], "본문")
        self.assertEqual(found[0]["total_matches"], 1)

    def test_highest_level_is_used_when_one_document_matches_multiple_levels(self):
        path = self.root / "mixed.hwpx"
        write_hwpx_paragraphs(path, [
            {"text": "복합문서 제목검색어", "style_name": "제목", "outline_level": 0},
            {"text": " ".join(["제목검색어"] * 8), "style_name": "바탕글"},
        ])
        self.index.index_file(path)
        found = self.index.search("제목검색어")
        self.assertEqual(len(found), 1)
        self.assertEqual(found[0]["best_level"], "문서 제목")
        self.assertEqual(found[0]["total_matches"], 9)

    def test_phrase_counts_and_highlights_preserve_existing_contract(self):
        path = self.root / "phrase.txt"
        path.write_text('First Line "Exact Match Target" And More "Exact Match Target"', encoding="utf-8")
        self.index.index_file(path)
        found = self.index.search('"Exact Match Target"')
        self.assertEqual(len(found), 1)
        self.assertEqual(found[0]["match_counts"], {'Exact Match Target': 2})
        self.assertEqual(found[0]["total_matches"], 2)
        self.assertTrue(found[0]["highlights"])

    def test_operator_filters_do_not_change_hierarchy_ordering(self):
        p_hwpx = self.root / "target.hwpx"
        p_txt = self.root / "target.txt"
        write_hwpx_paragraphs(p_hwpx, [{"text": "제1장 필터검색어", "outline_level": 1}])
        p_txt.write_text("필터검색어 필터검색어 필터검색어", encoding="utf-8")
        self.index.index_file(p_hwpx)
        self.index.index_file(p_txt)

        found = self.index.search("ext:hwpx 필터검색어")
        self.assertEqual(len(found), 1)
        self.assertEqual(found[0]["best_level"], "장")

    def test_sort_alias_compatibility(self):
        p1 = self.root / "p1.txt"
        p2 = self.root / "p2.txt"
        p1.write_text("별칭검색 1회", encoding="utf-8")
        p2.write_text("별칭검색 별칭검색 2회", encoding="utf-8")
        self.index.index_file(p1)
        self.index.index_file(p2)
        found = self.index.search("별칭검색", sort="relevance")
        self.assertEqual(found[0]["name"], p2.name)

    def test_3000_document_search_under_one_second(self):
        with self.index._lock, self.index.db:
            self.index.db.executemany(
                "INSERT INTO content_paragraph_fts(path,name,paragraph_index,level,level_rank,body) VALUES(?,?,?,?,?,?)",
                [(f"{self.root}/d{i}.txt", f"d{i}.txt", 0, "body", 8, f"문서 {i} 공통어 행정문서 대한민국 검색") for i in range(3000)]
            )
            self.index.db.executemany(
                "INSERT INTO content_fts(path,name,body) VALUES(?,?,?)",
                [(f"{self.root}/d{i}.txt", f"d{i}.txt", f"문서 {i} 공통어 행정문서 대한민국 검색") for i in range(3000)]
            )
            self.index.db.executemany(
                "INSERT INTO content_meta(path,size,mtime,ctime,hierarchy_schema_version) VALUES(?,?,?,?,?)",
                [(f"{self.root}/d{i}.txt", 1, 1.0, 1.0, CURRENT_HIERARCHY_SCHEMA_VERSION) for i in range(3000)]
            )
        start = time.perf_counter()
        result = self.index.search("행정문서 대한민국", limit=100)
        elapsed = time.perf_counter() - start
        print(f"\n[성능 측정] 3000건 본문검색: {elapsed:.4f}초", flush=True)
        self.assertEqual(len(result), 100)
        self.assertLess(elapsed, 1.0, f"3000건 검색: {elapsed:.3f}s")

    def _assert_map_matches_fts(self):
        db = self.index.db
        fts = [tuple(r) for r in db.execute(
            "SELECT path, CAST(paragraph_index AS INTEGER), rowid FROM content_paragraph_fts ORDER BY rowid")]
        mapped = [tuple(r) for r in db.execute(
            "SELECT path, paragraph_index, para_rowid FROM content_paragraph_map ORDER BY para_rowid")]
        self.assertEqual(mapped, fts)
        state = tuple(db.execute("SELECT row_count, max_rowid FROM content_paragraph_map_state WHERE id=1").fetchone())
        self.assertEqual(state, (len(fts), max((r[2] for r in fts), default=0)))

    def test_paragraph_map_self_heals_when_empty_or_stale(self):
        first = self.root / "heal_a.txt"
        second = self.root / "heal_b.txt"
        first.write_text("청사 방호 계획\n둘째 줄", encoding="utf-8")
        second.write_text("도서관 운영 계획", encoding="utf-8")
        self.index.index_file(first)
        self.index.index_file(second)
        self._assert_map_matches_fts()
        expected = [r["path"] for r in self.index.keyword_search("계획")]
        self.assertEqual(len(expected), 2)

        # 대응 표가 빈 옛 색인
        with self.index._lock, self.index.db:
            self.index.db.execute("DELETE FROM content_paragraph_map")
            self.index.db.execute("UPDATE content_paragraph_map_state SET row_count=-1, max_rowid=-1")
        self.assertEqual([r["path"] for r in self.index.keyword_search("계획")], expected)
        self._assert_map_matches_fts()

        # 문단 FTS에 직접 넣은 행(시험·측정 스크립트 방식)
        raw = str(first.resolve())
        with self.index._lock, self.index.db:
            self.index.db.execute(
                "INSERT INTO content_paragraph_fts(path,name,paragraph_index,level,level_rank,body,location_label,section)"
                " VALUES(?,?,?,?,?,?,?,?)", (raw, first.name, 2, "body", 8, "직접 넣은 수영장 문단", "문단 3", None))
        found = self.index.keyword_search("수영장")
        self.assertEqual([r["path"] for r in found], [raw])
        self.assertEqual(found[0]["locations"][0]["paragraph_index"], 2)
        self.assertEqual([r["path"] for r in self.index.search("수영장")], [raw])
        self._assert_map_matches_fts()

        # 문단 FTS에서 직접 지운 행, 그리고 행 수는 같지만 내용이 어긋난 대응 표
        with self.index._lock, self.index.db:
            self.index.db.execute("DELETE FROM content_paragraph_fts WHERE body LIKE '%수영장%'")
        self.assertEqual(self.index.keyword_search("수영장"), [])
        self._assert_map_matches_fts()
        with self.index._lock, self.index.db:
            self.index.db.execute("UPDATE content_paragraph_map SET path='없는 경로'"
                                  " WHERE para_rowid=(SELECT MAX(para_rowid) FROM content_paragraph_map)")
        self.assertEqual([r["path"] for r in self.index.keyword_search("계획")], expected)
        self._assert_map_matches_fts()

    def test_paragraph_map_self_heals_for_writes_from_another_connection(self):
        path = self.root / "other_conn.txt"
        path.write_text("공원 정비 계획", encoding="utf-8")
        self.index.index_file(path)
        self.assertEqual(len(self.index.keyword_search("정비")), 1)
        other = sqlite3.connect(self.root / "content.sqlite")
        try:
            with other:
                other.execute(
                    "INSERT INTO content_paragraph_fts(path,name,paragraph_index,level,level_rank,body,location_label,section)"
                    " VALUES(?,?,?,?,?,?,?,?)",
                    (str(path.resolve()), path.name, 1, "body", 8, "다른 연결이 넣은 놀이터 문단", "문단 2", None))
        finally:
            other.close()
        self.assertEqual(len(self.index.keyword_search("놀이터")), 1)
        self._assert_map_matches_fts()

    def test_paragraph_map_stays_one_to_one_through_index_lifecycle(self):
        keep = self.root / "keep.txt"
        path = self.root / "life.txt"
        keep.write_text("남는 문서\n둘째 줄", encoding="utf-8")
        path.write_text("줄 1\n줄 2\n줄 3", encoding="utf-8")
        self.index.index_file(keep)
        self.index.index_file(path)
        self._assert_map_matches_fts()
        raw = str(path.resolve())
        count = lambda: self.index.db.execute(
            "SELECT COUNT(*) FROM content_paragraph_map WHERE path=?", (raw,)).fetchone()[0]
        self.assertEqual(count(), 3)

        path.write_text("바뀐 줄 1\n바뀐 줄 2\n바뀐 줄 3\n바뀐 줄 4\n바뀐 줄 5", encoding="utf-8")
        self.assertTrue(self.index.index_file(path)["indexed"])
        self._assert_map_matches_fts()
        self.assertEqual(count(), 5)

        path.write_text("한 줄만", encoding="utf-8")
        self.assertTrue(self.index.index_file(path)["indexed"])
        self._assert_map_matches_fts()
        self.assertEqual(count(), 1)

        self.index.remove_file(path)
        self._assert_map_matches_fts()
        self.assertEqual(count(), 0)
        self.assertEqual(self.index.db.execute("SELECT COUNT(*) FROM content_paragraph_map").fetchone()[0], 2)
        # 일치가 유지됐으므로 다음 확인은 다시 채우지 않는다.
        self.assertFalse(self.index._ensure_paragraph_map())

    def test_unchanged_file_reindex_does_not_scan_paragraph_table(self):
        import re
        path = self.root / "unchanged.txt"
        path.write_text("고정 내용\n둘째 줄", encoding="utf-8")
        self.assertTrue(self.index.index_file(path)["indexed"])
        statements = []
        self.index.db.set_trace_callback(statements.append)
        try:
            self.assertFalse(self.index.index_file(path)["indexed"])
            # 다른 연결이 쓴 뒤(일치 확인을 다시 해야 하는 경우)에도 전체 훑기는 없어야 한다.
            other = sqlite3.connect(self.root / "content.sqlite")
            try:
                with other:
                    other.execute("CREATE TABLE IF NOT EXISTS unrelated_probe(x)")
            finally:
                other.close()
            self.assertFalse(self.index.index_file(path)["indexed"])
        finally:
            self.index.db.set_trace_callback(None)
        self.assertTrue(statements)
        for sql in statements:
            # 그림자 표(_docsize, _data, _content)의 키 조회와 rowid 한 건 조회만 허용한다.
            if re.search(r"content_paragraph_fts(?!_)", sql):
                self.assertRegex(sql, r"WHERE rowid=", sql)
            self.assertNotRegex(sql, r"COUNT\(\*\) FROM", sql)
        plan = " ".join(row[3] for row in self.index.db.execute(
            "EXPLAIN QUERY PLAN SELECT 1 FROM content_paragraph_map WHERE path=? LIMIT 1", ("x",)))
        self.assertIn("SEARCH", plan)
        self.assertNotIn("SCAN", plan)

    def test_paragraph_lookup_by_rowid_and_by_scan_give_same_results(self):
        for doc in range(6):
            lines = [f"문서{doc} 머리말 2024-0{doc + 1}-15 예산 편성", f"주민센터 운영 계획 {doc}",
                     f"예산 집행 결과와 주민센터 민원 {doc}", f"맺음말 {doc}"]
            path = self.root / f"both{doc}.txt"
            path.write_text("\n".join(lines[:2 + doc % 3]), encoding="utf-8")
            self.index.index_file(path)
        queries = [("예산", {}), ("주민센터 예산", {}), ("주민센터", {"limit": 2, "offset": 1}), ('"운영 계획"', {}),
                   ("예산 -민원", {}), ("주민센터 예산 near:3", {}), ("예산 after:2024-03", {})]

        def run():
            keyword = [self.index.keyword_search(q, **kw) for q, kw in queries]
            hierarchy = [self.index.search(q) for q, _ in queries[:2]]
            return keyword, hierarchy

        with patch.object(ContentSearchIndex, "_PARAGRAPH_SCAN_RATIO", 0):
            by_rowid = run()
        with patch.object(ContentSearchIndex, "_PARAGRAPH_SCAN_RATIO", 10 ** 9):
            by_scan = run()
        self.assertEqual(by_rowid, by_scan)
        self.assertEqual(by_rowid, run())
        self.assertTrue(by_rowid[0][0])

    def test_combined_candidate_text_matches_group_concat_order(self):
        path = self.root / "concat.txt"
        path.write_text("첫 줄\n둘째 줄\n셋째 줄", encoding="utf-8")
        self.index.index_file(path)
        raw = str(path.resolve())
        with self.index._lock, self.index.db:
            # 문단 번호와 저장 순서가 다른 행, 본문이 NULL인 행
            self.index.db.executemany(
                "INSERT INTO content_paragraph_fts(path,name,paragraph_index,level,level_rank,body,location_label,section)"
                " VALUES(?,?,?,?,?,?,?,?)",
                [(raw, path.name, 9, "body", 8, "아홉째 줄", None, None), (raw, path.name, 4, "body", 8, None, None, None),
                 (raw, path.name, 3, "body", 8, "넷째 줄", None, None)])
        self.index._ensure_paragraph_map()
        grouped = self.index.db.execute(
            "SELECT group_concat(body, char(10)) FROM content_paragraph_fts WHERE path=? GROUP BY path", (raw,)).fetchone()[0]
        for ratio in (0, 10 ** 9):
            with patch.object(ContentSearchIndex, "_PARAGRAPH_SCAN_RATIO", ratio):
                rows = self.index._select_paragraphs_for_paths("path,paragraph_index,body,level_rank", [raw])
            self.assertEqual("\n".join(r["body"] for r in rows if r["body"] is not None), grouped)
        self.assertEqual(grouped, "첫 줄\n둘째 줄\n셋째 줄\n아홉째 줄\n넷째 줄")




class MinPositionSpanTests(unittest.TestCase):
    def test_matches_brute_force_product(self):
        import itertools
        import random
        from docfit_core.timemachine.content_search import _min_position_span
        rnd = random.Random(20261111)
        for _ in range(2000):
            lists = [[rnd.randrange(40) for _ in range(rnd.randrange(0, 6))] for _ in range(rnd.randrange(1, 5))]
            expected = (min(max(c) - min(c) for c in itertools.product(*lists))
                        if all(lists) else None)
            self.assertEqual(_min_position_span(lists), expected, lists)

    def test_many_positions_do_not_explode(self):
        from docfit_core.timemachine.content_search import _min_position_span
        lists = [list(range(i, 60000, 4)) for i in range(4)]   # 조합으로는 15,000^4가지
        start = time.perf_counter()
        self.assertEqual(_min_position_span(lists), 3)
        self.assertLess(time.perf_counter() - start, 2.0)


class BatchIndexTests(unittest.TestCase):
    TABLES = ("content_meta", "content_fts", "content_short_grams", "content_paragraph_fts",
              "content_paragraph_map", "content_errors")
    COLUMNS = {
        "content_paragraph_map": "path, paragraph_index",
        "content_paragraph_fts": "path, paragraph_index, level, body",
        "content_fts": "path, name, body",
        "content_meta": "path, name, extension, size",
    }

    def _write_files(self, folder, count, prefix="doc"):
        paths = []
        for i in range(count):
            path = Path(folder) / f"{prefix}_{i:03d}.txt"
            path.write_text(f"공통낱말 문서{i} alpha{i}\n둘째 문단 beta{i} 예산", encoding="utf-8")
            paths.append(path)
        return paths

    def _dump(self, index):
        return {
            table: sorted(tuple(row) for row in index.db.execute(
                f"SELECT {self.COLUMNS.get(table, '*')} FROM {table}"))
            for table in self.TABLES
        }

    def _assert_one_to_one(self, index):
        db = index.db
        self.assertFalse(db.in_transaction)
        para = {row[0] for row in db.execute("SELECT rowid FROM content_paragraph_fts")}
        mapped = [row[0] for row in db.execute("SELECT para_rowid FROM content_paragraph_map")]
        self.assertEqual(len(mapped), len(set(mapped)))
        self.assertEqual(set(mapped), para)
        state = tuple(db.execute("SELECT row_count, max_rowid FROM content_paragraph_map_state WHERE id=1").fetchone())
        self.assertEqual(state, (len(para), max(para, default=0)))
        meta = {row[0] for row in db.execute("SELECT path FROM content_meta")}
        for table in ("content_fts", "content_short_grams", "content_paragraph_fts"):
            self.assertEqual({row[0] for row in db.execute(f"SELECT DISTINCT path FROM {table}")}, meta, table)
        self.assertFalse(index._ensure_paragraph_map())

    def test_batch_and_rollback(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "sample.txt"
            path.write_text("alpha beta", encoding="utf-8")
            index = ContentSearchIndex(Path(tmp) / "index.sqlite")
            with index.batch():
                self.assertTrue(index.index_file(path)["indexed"])
            self.assertEqual(len(index.search("alpha")), 1)
            other = Path(tmp) / "other.txt"
            other.write_text("gamma delta", encoding="utf-8")
            with self.assertRaises(RuntimeError):
                with index.batch():
                    index.index_file(other)
                    raise RuntimeError("abort")
            self.assertEqual(len(index.search("gamma")), 0)
            self.assertEqual(index.db.execute("SELECT count(*) FROM content_paragraph_map").fetchone()[0],
                             index.db.execute("SELECT count(*) FROM content_paragraph_fts").fetchone()[0])
            self._assert_one_to_one(index)
            index.close()

    def test_batch_result_equals_per_file_indexing(self):
        with tempfile.TemporaryDirectory() as tmp:
            paths = self._write_files(tmp, 12)
            (Path(tmp) / "empty.txt").write_text("", encoding="utf-8")
            paths.append(Path(tmp) / "empty.txt")
            single = ContentSearchIndex(Path(tmp) / "single.sqlite")
            for path in paths:
                single.index_file(path)
            batched = ContentSearchIndex(Path(tmp) / "batched.sqlite")
            with batched.batch(max_files=5):
                results = [batched.index_file(path) for path in paths]
            self.assertEqual(sum(bool(r["indexed"]) for r in results), 12)
            self.assertEqual(sum(bool(r["error"]) for r in results), 1)
            self.assertEqual(self._dump(batched), self._dump(single))
            self._assert_one_to_one(batched)
            for query in ("공통낱말", "예산", "alpha3", "둘째"):
                self.assertEqual([r["path"] for r in batched.keyword_search(query)],
                                 [r["path"] for r in single.keyword_search(query)], query)
                self.assertEqual([r["path"] for r in batched.search(query)],
                                 [r["path"] for r in single.search(query)], query)
            self.assertEqual(len(batched.keyword_search("공통낱말")), 12)
            # 묶음 밖에서는 파일마다 커밋한다. 다른 연결이 바로 볼 수 있어야 한다.
            extra = self._write_files(tmp, 1, prefix="extra")[0]
            batched.index_file(extra)
            other = sqlite3.connect(Path(tmp) / "batched.sqlite")
            try:
                self.assertEqual(other.execute("SELECT COUNT(*) FROM content_meta").fetchone()[0], 13)
            finally:
                other.close()
            single.close()
            batched.close()

    def test_batch_commits_in_bounded_chunks(self):
        with tempfile.TemporaryDirectory() as tmp:
            paths = self._write_files(tmp, 7)
            index = ContentSearchIndex(Path(tmp) / "index.sqlite")
            other = sqlite3.connect(Path(tmp) / "index.sqlite")
            visible = []
            try:
                with index.batch(max_files=3, max_seconds=60):
                    for path in paths:
                        index.index_file(path)
                        visible.append(other.execute("SELECT COUNT(*) FROM content_meta").fetchone()[0])
                self.assertEqual(visible, [0, 0, 3, 3, 3, 6, 6])
                self.assertEqual(other.execute("SELECT COUNT(*) FROM content_meta").fetchone()[0], 7)
            finally:
                other.close()
            self._assert_one_to_one(index)
            index.close()

    def test_exception_mid_batch_keeps_tables_one_to_one(self):
        with tempfile.TemporaryDirectory() as tmp:
            paths = self._write_files(tmp, 8)
            index = ContentSearchIndex(Path(tmp) / "index.sqlite")
            index.index_file(paths[0])
            paths[0].write_text("고쳐 쓴 문서 omega 입니다", encoding="utf-8")
            with self.assertRaises(RuntimeError):
                with index.batch(max_files=3, max_seconds=60):
                    for number, path in enumerate(paths):
                        if number == 5:
                            raise RuntimeError("abort")
                        index.index_file(path)
            # 3개째에서 커밋된 조각은 남고, 그 뒤 2개(미커밋)는 되돌려진다.
            self.assertEqual(index.db.execute("SELECT COUNT(*) FROM content_meta").fetchone()[0], 3)
            self._assert_one_to_one(index)
            self.assertEqual(len(index.keyword_search("omega")), 1)
            self.assertEqual(len(index.keyword_search("alpha4")), 0)
            # 되돌려진 파일은 다시 색인된다.
            with index.batch():
                results = [index.index_file(path) for path in paths]
            self.assertEqual(sum(bool(r["indexed"]) for r in results), 5)
            self.assertEqual(index.db.execute("SELECT COUNT(*) FROM content_meta").fetchone()[0], 8)
            self._assert_one_to_one(index)
            index.close()

    def test_write_failure_inside_batch_drops_only_that_file(self):
        import docfit_core.timemachine.content_search as module
        with tempfile.TemporaryDirectory() as tmp:
            paths = self._write_files(tmp, 4)
            bad = os.path.abspath(paths[1])
            index = ContentSearchIndex(Path(tmp) / "index.sqlite")
            index.index_file(paths[1])
            original = module.extract_paragraphs

            def broken(path):
                records = original(path)
                if os.path.abspath(path) == bad:
                    # 마지막 문단에서 SQLite 바인딩이 실패한다(짝 없는 대리 문자).
                    records.append(ParagraphRecord(len(records), "깨진 문단 \ud800", "body", 8, False))
                return records

            paths[1].write_text("고친 문서 zeta\n둘째 문단", encoding="utf-8")
            with patch.object(module, "extract_paragraphs", broken):
                with index.batch(max_files=100, max_seconds=60):
                    results = [index.index_file(path) for path in paths]
                    # 실패한 파일의 쓰다 만 행이 묶음 안에 남지 않는다.
                    self.assertTrue(index.db.in_transaction)
                    self.assertEqual(index.db.execute(
                        "SELECT COUNT(*) FROM content_paragraph_fts WHERE path=?", (bad,)).fetchone()[0], 0)
            self.assertEqual([bool(r["indexed"]) for r in results], [True, False, True, True])
            self.assertIn("UnicodeEncodeError", results[1]["error"])
            self.assertEqual(index.db.execute("SELECT COUNT(*) FROM content_meta").fetchone()[0], 3)
            self.assertEqual([row[0] for row in index.db.execute("SELECT path FROM content_errors")], [bad])
            self._assert_one_to_one(index)
            self.assertEqual(len(index.keyword_search("공통낱말")), 3)
            self.assertEqual(len(index.keyword_search("zeta")), 0)
            index.close()

    def test_failed_write_block_inside_batch_keeps_earlier_files(self):
        with tempfile.TemporaryDirectory() as tmp:
            paths = self._write_files(tmp, 3)
            first = os.path.abspath(paths[0])
            index = ContentSearchIndex(Path(tmp) / "index.sqlite")
            with index.batch():
                index.index_file(paths[0])
                index.index_file(paths[1])
                with self.assertRaises(RuntimeError):
                    with index._lock, index._map_kept(), index._index_transaction(first):
                        index._delete_paragraphs(first)
                        index.db.execute(
                            "INSERT INTO content_paragraph_fts(path, name, paragraph_index, level, level_rank, body)"
                            " VALUES(?,?,?,?,?,?)", (first, "doc_000.txt", 0, "body", 8, "쓰다 만 문단"))
                        raise RuntimeError("write failed")
                index.index_file(paths[2])
            self.assertEqual(sorted(row[0] for row in index.db.execute("SELECT path FROM content_meta")),
                             sorted(os.path.abspath(p) for p in paths[1:]))
            self._assert_one_to_one(index)
            index.close()

    def test_keyword_search_from_other_thread_returns_during_batch(self):
        import threading
        with tempfile.TemporaryDirectory() as tmp:
            paths = self._write_files(tmp, 6)
            index = ContentSearchIndex(Path(tmp) / "index.sqlite")
            index.index_file(paths[0])
            midway, resume = threading.Event(), threading.Event()
            failures = []

            def worker():
                try:
                    with index.batch(max_files=100, max_seconds=60):
                        for number, path in enumerate(paths):
                            index.index_file(path)
                            if number == 2:
                                midway.set()
                                if not resume.wait(timeout=20):
                                    failures.append("resume timeout")
                except BaseException as exc:  # noqa: BLE001 - 시험 스레드의 실패를 본 스레드로 옮긴다
                    failures.append(repr(exc))

            thread = threading.Thread(target=worker)
            thread.start()
            try:
                self.assertTrue(midway.wait(timeout=20))
                self.assertTrue(index.db.in_transaction)
                started = time.perf_counter()
                keyword_rows = index.keyword_search("공통낱말")
                keyword_elapsed = time.perf_counter() - started
                started = time.perf_counter()
                hierarchy_rows = index.search("공통낱말")
                count = index.needs_reindex_count()
                other_elapsed = time.perf_counter() - started
                # 검색이 묶음의 미커밋 변경을 대신 커밋하지 않는다.
                still_open = index.db.in_transaction
            finally:
                resume.set()
                thread.join(timeout=30)
            self.assertFalse(thread.is_alive())
            self.assertEqual(failures, [])
            self.assertTrue(still_open)
            self.assertLess(keyword_elapsed, 2.0)
            self.assertLess(other_elapsed, 2.0)
            self.assertEqual(len(keyword_rows), 3)
            self.assertEqual(len(hierarchy_rows), 3)
            self.assertEqual(count, 0)
            self.assertEqual(len(index.keyword_search("공통낱말")), 6)
            self._assert_one_to_one(index)
            index.close()


if __name__ == "__main__":
    unittest.main()
