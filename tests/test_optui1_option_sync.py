"""OPTUI1: 한 번에 적용 옵션의 설정 키·기본값·세부 작업 연동 규칙이 화면 정리 뒤에도 그대로인지 고정한다.

화면(Flutter)만 정리했으므로 이 값들은 바뀌면 안 된다. 옵션 조합마다 '옵션 → 세부 작업 → 옵션'이 제자리로 돌아오는지(단일 원천)도 본다.
"""
import itertools
import unittest

from docfit_app.settings_schema import DEFAULTS
from docfit_core import stage_selection as ss

ALL_OPTIONS = ("all_include_spacing", "all_exclude_tables", "all_exclude_pagefit",
               "all_exclude_linespacing", "all_cell_image_to_bg")


class OptionDefaultsTest(unittest.TestCase):
    def test_setting_keys_and_defaults_are_unchanged(self):
        self.assertEqual(
            {key: DEFAULTS[key] for key in ALL_OPTIONS},
            {"all_include_spacing": True, "all_exclude_tables": False, "all_exclude_pagefit": False,
             "all_exclude_linespacing": False, "all_cell_image_to_bg": False})

    def test_stage_counts_per_mode_are_unchanged(self):
        self.assertEqual({mode: len(ss.stages_for_mode(mode)) for mode in ("spacing", "unify", "format", "all")},
                         {"spacing": 11, "unify": 7, "format": 21, "all": 28})

    def test_option_to_stage_links_are_unchanged(self):
        self.assertEqual(ss.CARD_OPTION_STAGES["all_include_spacing"], (("all",), ss.SPACING_STAGE_KEYS, False))
        self.assertEqual(ss.CARD_OPTION_STAGES["all_exclude_tables"], (("all", "format"), ss.TABLE_STAGE_KEYS, True))
        self.assertEqual(ss.CARD_OPTION_STAGES["all_exclude_pagefit"], (("all", "format"), ss.PAGE_FIT_STAGE_KEYS, True))
        self.assertEqual(ss.CARD_OPTION_STAGES["all_exclude_linespacing"], (("all", "format"), (), True))
        self.assertEqual(ss.CARD_OPTION_STAGES["all_cell_image_to_bg"], (("all", "format"), ("cell_image_to_bg",), False))


class OptionRoundTripTest(unittest.TestCase):
    def test_every_combination_round_trips_through_stages(self):
        for mode in ("all", "format"):
            for values in itertools.product((False, True), repeat=len(ALL_OPTIONS)):
                options = dict(zip(ALL_OPTIONS, values))
                selection = {key: ss.default_choice(key, mode) for key, _ in ss.stages_for_mode(mode)}
                for option in ALL_OPTIONS:
                    selection.update(ss.card_option_stage_values(option, options[option], options, mode))
                for option in ALL_OPTIONS:
                    keys = ss.card_option_keys(option, mode)
                    if not keys:
                        continue
                    # '제외'를 켠 옵션은 관련 작업이 모두 꺼져 있고, 꺼진 옵션은 하나라도 켜져 있다.
                    # (같은 모드의 다른 '제외' 옵션이 끈 작업은 옵션이 꺼져도 켤 수 없으므로 그 경우는 제외한다.)
                    blocked = ss.card_option_off_keys({k: v for k, v in options.items() if k != option}, mode)
                    free = [key for key in keys if key not in blocked]
                    if not free:
                        continue
                    restored = ss.card_option_from_stages(option, selection, mode)
                    self.assertEqual(restored, options[option], f"{mode} {options} {option}")


if __name__ == "__main__":
    unittest.main()
