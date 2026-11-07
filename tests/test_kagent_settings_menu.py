"""빠른 목록 '설정' 메뉴(kagent settings_menu.rs)와 앱(kagent_client·관리창)이 같은 값을 쓰는지 확인한다."""
import re
import unittest
from pathlib import Path

from docfit_app import kagent_client as kc

ROOT = Path(__file__).resolve().parents[1]
RUST = (ROOT / "kagent" / "src" / "settings_menu.rs").read_text(encoding="utf-8")
DIALOG = (ROOT / "flutter_ui" / "lib" / "features" / "kclip" / "kclip_dialog.dart").read_text(encoding="utf-8")


def rust_const(name):
    match = re.search(rf'pub const {name}: &str = "((?:[^"\\]|\\.)*)";', RUST)
    if not match:
        raise AssertionError(f"{name} 없음")
    return match.group(1).replace("\\\\", "\\")


def rust_choices(name):
    match = re.search(rf"pub const {name}: \[i64; \d+\] = \[([^\]]*)\];", RUST)
    return [int(v) for v in match.group(1).split(",")]


def dart_choices(key):
    match = re.search(rf"number\('{key}', '[^']*', const \[([^\]]*)\]\)", DIALOG)
    return [int(v) for v in match.group(1).split(",")]


class KAgentSettingsMenuTests(unittest.TestCase):
    def test_autostart_registration_matches_app(self):
        # 에이전트가 직접 등록해도 앱의 autostart_enabled()가 같은 등록으로 읽어야 한다.
        self.assertEqual(rust_const("RUN_KEY"), kc.RUN_KEY)
        self.assertEqual(rust_const("RUN_NAME"), kc.RUN_NAME)
        self.assertIn('format!("\\"{exe}\\" --delay 30")', RUST)

    def test_menu_choices_include_management_window_choices(self):
        for rust, dart in (("MAX_ITEMS", "max_items"), ("MAX_TOTAL_MB", "max_total_mb"), ("MAX_AGE_DAYS", "max_age_days")):
            self.assertEqual(rust_choices(rust), dart_choices(dart), rust)

    def test_menu_uses_same_settings_keys_as_management_window(self):
        for key in ("max_items", "max_total_mb", "max_age_days", "sticky_popup", "always_on_top", "auto_paste",
                    "excel_target_paste"):
            self.assertIn(f"'{key}'", DIALOG)
            self.assertIn(f'"{key}"', RUST)


if __name__ == "__main__":
    unittest.main()
