"""목록 키(hidden_tabs·library_excluded_folders): 앱 스키마·골든·Flutter 설정 화면·서식 엔진과의 관계(키 개수는 스키마에서 계산)."""
import json
import unittest
from pathlib import Path

from docfit_app.configuration_service import EXCLUDED, fields
from docfit_app.settings_schema import ALL_DEFAULTS, DEFAULTS, LIST_DEFAULTS

GOLDEN = Path(__file__).resolve().parents[1] / "docs" / "settings_defaults.json"


class SettingsListKeysTest(unittest.TestCase):
    def test_schema_has_list_keys_with_empty_defaults(self):
        self.assertTrue(LIST_DEFAULTS, "앱 스키마에 목록 키가 없다")
        for key, value in LIST_DEFAULTS.items():
            self.assertEqual(value, [], key)
            self.assertIsInstance(value, list)

    def test_golden_matches_python_defaults_exactly(self):
        self.assertEqual(json.loads(GOLDEN.read_text(encoding="utf-8")), ALL_DEFAULTS)

    def test_list_keys_stay_out_of_engine_defaults_and_flutter_form(self):
        # DEFAULTS 는 서식 엔진(설정_불러오기·설정_저장)이 통째로 읽고 되쓰므로 목록 키를 넣지 않는다.
        shown = {f["key"] for f in fields()}
        for key in LIST_DEFAULTS:
            self.assertNotIn(key, DEFAULTS, key)
            self.assertNotIn(key, shown, key)

    def test_every_default_key_is_a_form_field_or_excluded(self):
        covered = {f["key"].split(".")[0] for f in fields()} | EXCLUDED
        self.assertEqual(set(DEFAULTS) - covered, set())


if __name__ == "__main__":
    unittest.main()
