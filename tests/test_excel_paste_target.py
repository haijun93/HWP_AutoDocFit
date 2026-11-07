"""엑셀 CF_HTML → 몽땅클립보드 '목표 결과' 변환 시험 (합성 자료: tests/fixtures/xlpaste_accounting.cf_html).

합성 자료는 TODO.md 첨부 분석(오류1.xlsx)을 흉내 낸 것이다: 회계 서식 공백, 전각·nbsp 빈 칸,
맨 위 빈 행, 열 너비 18.625/25.75/22.75자. 실제 한/글 붙여넣기는 PC에서 따로 확인해야 한다.
"""
import re
import unittest
from pathlib import Path

from docfit_core.excel_paste_target import clean_tsv, to_target_html, to_target_html_report
from docfit_core.excel_table import parse_clipboard
from docfit_app.excel_table_service import ExcelTableService

FIX = Path(__file__).parent / "fixtures"
SRC = (FIX / "xlpaste_accounting.cf_html").read_bytes()


def grid(model):
    return [[c["text"] for c in row] for row in model["rows"]]


def header(data):
    return {k.decode(): int(v) for k, v in re.findall(rb"(StartHTML|EndHTML|StartFragment|EndFragment):(\d+)", data)}


class TargetHtmlTests(unittest.TestCase):
    def setUp(self):
        self.out, self.report = to_target_html_report(SRC)

    def test_changed_and_counts(self):
        self.assertTrue(self.report["changed"])
        self.assertEqual(self.report["numeric"], 5)
        self.assertEqual(self.report["blank"], 8)

    def test_before_has_problems(self):
        before = grid(parse_clipboard(SRC, clean=False))
        self.assertTrue(before[2][2].startswith(" " * 20))      # 숫자 앞 공백 ~20칸
        self.assertIn("　", before[0][2] + before[3][0])    # 전각 공백 빈 칸

    def test_after_numbers_and_blanks(self):
        after = grid(parse_clipboard(self.out, clean=False))
        self.assertEqual([r[2] for r in after[2:]], ["88,180 ", "2,000 ", "", "1,300 ", "60,000 ", "24,880 "])
        for row in after:
            for text in row:
                self.assertFalse(text.strip("  　") == "" and text != "", repr(text))
        self.assertEqual(after[0], ["", "", ""])               # 맨 위 빈 행 보존
        self.assertEqual(len(after), 8)
        self.assertEqual(after[1], ["부서명", "목", "2026년 예산(A)"])
        self.assertEqual([r[1] for r in after[2:]], ["소계", "과징금", "그외수입", "기타과태료", "기타사업수입", "기타사용료"])
        self.assertEqual([r[0] for r in after[2:]], ["관광정책과", "", "", "", "", ""])

    def test_text_otherwise_unchanged(self):
        strip = lambda m: [[re.sub(r"[\s 　]+", "", t) for t in r] for r in grid(m)]
        self.assertEqual(strip(parse_clipboard(SRC, clean=False)), strip(parse_clipboard(self.out, clean=False)))

    def test_column_widths_consistent(self):
        html = self.out.decode()
        cols = re.findall(r"<col width=([\d.]+)", html)
        self.assertEqual(cols, ["135", "185", "164"])
        rows = re.findall(r"<tr\b.*?</tr>", html, re.S)
        self.assertEqual(len(rows), 8)
        for row in rows:
            self.assertEqual(re.findall(r"<td\b[^>]*?\bwidth=([\d.]+)", row), cols)
        self.assertRegex(html, r"<table[^>]*width=484")
        # 엑셀 열 너비 비례(약 612 한/글 단위/문자)와 일치
        ratios = [int(c) / 484 for c in cols]
        chars = [18.625, 25.75, 22.75]
        for r, ch in zip(ratios, chars):
            self.assertAlmostEqual(r, ch / sum(chars), delta=0.01)

    def test_padding_normal(self):
        html = self.out.decode()
        td = re.search(r"\btd\s*\{([^}]*)\}", html).group(1)
        self.assertNotIn("mso-ignore:padding", td)
        self.assertIn("padding-left:5.1pt", td)
        self.assertIn("padding-top:1.4pt", td)
        self.assertNotIn("padding-left:1px", td)

    def test_shading_kept_by_default_and_strip_option(self):
        self.assertIn(b"background:yellow", self.out)
        stripped, report = to_target_html_report(SRC, shading="strip")
        self.assertTrue(report["changed"])
        self.assertGreater(report["shading_removed"], 0)
        self.assertIsNone(re.search(rb"background|mso-pattern", stripped))
        self.assertEqual(grid(parse_clipboard(stripped, clean=False)), grid(parse_clipboard(self.out, clean=False)))

    def test_cf_html_offsets(self):
        h = header(self.out)
        self.assertEqual(h["EndHTML"], len(self.out))
        self.assertEqual(self.out[h["StartFragment"] - 20:h["StartFragment"]], b"<!--StartFragment-->")
        self.assertEqual(self.out[h["EndFragment"]:h["EndFragment"] + 18], b"<!--EndFragment-->")
        self.assertTrue(self.out[h["StartHTML"]:].lstrip().startswith(b"<html"))

    def test_idempotent(self):
        again, report = to_target_html_report(self.out)
        self.assertFalse(report["changed"])
        self.assertIs(again, self.out)

    def test_str_input_returns_str(self):
        out = to_target_html(SRC.decode("utf-8"))
        self.assertIsInstance(out, str)
        self.assertNotIn(" " * 10, out)

    def test_merged_fixture_structure_kept(self):
        src = (FIX / "excel_clipboard.cf_html").read_bytes()
        out = to_target_html(src)
        a, b = parse_clipboard(src, clean=False), parse_clipboard(out, clean=False)
        key = lambda m: [(c["row"], c["col"], c["rowspan"], c["colspan"]) for r in m["rows"] for c in r]
        self.assertEqual(key(a), key(b))
        self.assertEqual(len(re.findall(rb"<tr\b", src)), len(re.findall(rb"<tr\b", out)))

    def test_non_table_and_bad_input_fallback(self):
        for bad in [None, b"", "", "plain\ttext", "<p>x</p>", b"\xff\xfe\x00bad", 123, "<td>x", b"<table><tr><td>1"]:
            with self.subTest(bad=bad):
                out, report = to_target_html_report(bad)
                self.assertEqual(out, bad)
                self.assertFalse(report["changed"])

    def test_non_numeric_leading_spaces_untouched(self):
        src = "<table><col width=80><tr><td>  들여쓴 글</td><td>   1,000 </td></tr></table>"
        out = to_target_html(src)
        self.assertIn("<td", out)
        self.assertIn("  들여쓴 글", out)
        self.assertIn(">1,000 </td>", out)

    def test_no_widths_still_cleans_text(self):
        out, report = to_target_html_report("<table><tr><td>&nbsp;</td><td>   5 </td></tr></table>")
        self.assertTrue(report["changed"])
        self.assertIn("<td></td>", out)
        self.assertIn(">5 </td>", out)


class TsvTests(unittest.TestCase):
    def test_clean_tsv(self):
        tsv = "부서명\t목\t예산\r\n관광\t소계\t         88,180 \r\n　\t과징금\t   2,000 \r\n"
        self.assertEqual(clean_tsv(tsv), "부서명\t목\t예산\r\n관광\t소계\t88,180 \r\n\t과징금\t2,000 \r\n")
        self.assertEqual(clean_tsv(None), None)


class ServiceTests(unittest.TestCase):
    def test_target_paste_writes_clipboard(self):
        written = []
        svc = ExcelTableService(clipboard=lambda: (SRC, "a\t   1,000 "), html_writer=lambda h, t: written.append((h, t)))
        res = svc.command("target_paste")
        self.assertTrue(res["ok"] and res["changed"], res)
        self.assertEqual(len(written), 1)
        self.assertEqual(written[0][1], "a\t1,000 ")
        self.assertIsNone(re.search(rb"mso-ignore:padding", written[0][0]))

    def test_strip_option(self):
        written = []
        svc = ExcelTableService(clipboard=lambda: (SRC, None), html_writer=lambda h, t: written.append(h))
        self.assertTrue(svc.command("target_paste", {"keep_cell_background": False})["ok"])
        self.assertIsNone(re.search(rb"background", written[0]))

    def test_no_html_or_nothing_to_do_leaves_clipboard(self):
        written = []
        svc = ExcelTableService(clipboard=lambda: (None, "x"), html_writer=lambda h, t: written.append(h))
        self.assertFalse(svc.command("target_paste")["ok"])
        done, _ = to_target_html_report(SRC)
        svc = ExcelTableService(clipboard=lambda: (done, None), html_writer=lambda h, t: written.append(h))
        res = svc.command("target_paste")
        self.assertTrue(res["ok"])
        self.assertFalse(res["changed"])
        self.assertEqual(written, [])

    def test_writer_failure_reported(self):
        def boom(h, t): raise OSError("busy")
        svc = ExcelTableService(clipboard=lambda: (SRC, None), html_writer=boom)
        res = svc.command("target_paste")
        self.assertFalse(res["ok"])


if __name__ == "__main__":
    unittest.main()
