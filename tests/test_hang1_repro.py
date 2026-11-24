import hashlib
from pathlib import Path
import unittest

from scripts.hang1_repro_pc import taskkill_command, unfinished_stage, validate_timeout


ROOT = Path(__file__).resolve().parents[1]


class Hang1ReproTests(unittest.TestCase):
    def test_fixture_matches_spdmemo3_original_and_shape(self):
        source = ROOT / "tests" / "fixtures" / "allqa_report_synthetic.txt"
        data = source.read_bytes()
        text = data.decode("utf-8")
        self.assertEqual(
            hashlib.sha256(data).hexdigest(),
            "f6b123bacdbb66296dceb1123c3e4e228424d423770acd785437fa79aed471af",
        )
        self.assertEqual(len(text.splitlines()), 15)
        self.assertEqual(sum(line.startswith(tuple(f"{n}장." for n in range(1, 7)))
                             for line in text.splitlines()), 6)

    def test_second_spdmemo3_fixture_is_preserved(self):
        source = ROOT / "tests" / "fixtures" / "allqa_tables24_synthetic.txt"
        self.assertEqual(
            hashlib.sha256(source.read_bytes()).hexdigest(),
            "5ba5f8eecf1136f3b0bb7074beba3fa433ea88f65ece6b365cf6aecffee218ef",
        )
        self.assertEqual(len(source.read_text(encoding="utf-8").splitlines()), 97)

    def test_timeout_is_capped_at_fifteen_minutes(self):
        self.assertEqual(validate_timeout(1), 1)
        self.assertEqual(validate_timeout(900), 900)
        for invalid in (0, 901):
            with self.subTest(invalid=invalid), self.assertRaises(ValueError):
                validate_timeout(invalid)

    def test_trace_reports_only_unfinished_stage(self):
        records = [
            {"kind": "stage_start", "stage": "TXT → HWPX 변환"},
            {"kind": "stage_end", "stage": "TXT → HWPX 변환"},
            {"kind": "stage_start", "stage": "본문 자간·단어 분리 조정"},
        ]
        self.assertEqual(unfinished_stage(records), "본문 자간·단어 분리 조정")
        records.append({"kind": "stage_end", "stage": "본문 자간·단어 분리 조정"})
        self.assertIsNone(unfinished_stage(records))

    def test_timeout_cleanup_kills_only_selected_process_tree(self):
        self.assertEqual(taskkill_command(123, tree=True),
                         ["taskkill", "/PID", "123", "/T", "/F"])
        self.assertEqual(taskkill_command(123, tree=False),
                         ["taskkill", "/PID", "123", "/F"])


if __name__ == "__main__":
    unittest.main()
