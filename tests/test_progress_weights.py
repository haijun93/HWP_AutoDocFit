import unittest

from docfit_core.progress_guide import progress_view
from docfit_core import progress_weights as pw


class WeightTests(unittest.TestCase):
    def test_baseline_shares(self):
        keys = ["open", "pre_format", "space", "unify_survey", "standard", "body_spacing", "control_spacing",
                "short_line", "indent", "page_fit", "page_group", "save"]
        w = pw.step_weights(keys)
        total = sum(w.values())
        self.assertAlmostEqual((w["body_spacing"] + w["control_spacing"]) / total, 0.42, delta=0.03)
        self.assertAlmostEqual(w["indent"] / total, 0.23, delta=0.03)
        self.assertAlmostEqual((w["page_group"] + w["page_fit"]) / total, 0.12, delta=0.03)

    def test_scale_affects_only_non_fixed(self):
        w = pw.step_weights(["open", "body_spacing"], scale=2)
        self.assertEqual(w["open"], pw.STEP_WEIGHTS["open"])
        self.assertEqual(w["body_spacing"], pw.STEP_WEIGHTS["body_spacing"] * 2)
        self.assertEqual(pw.step_weights(["body_spacing"], scale=999)["body_spacing"],
                         pw.STEP_WEIGHTS["body_spacing"] * pw.SCALE_RANGE[1])

    def test_weighted_sum_and_bounds(self):
        keys = ["open", "body_spacing", "save"]
        self.assertEqual(pw.weighted_ratio(keys, []), 0.0)
        self.assertAlmostEqual(pw.weighted_ratio(keys, ["open"], "body_spacing"), 3 / 45)
        self.assertAlmostEqual(pw.weighted_ratio(keys, ["open", "body_spacing"], "save"), 43 / 45)
        self.assertEqual(pw.weighted_ratio(keys, keys), 1.0)
        self.assertEqual(pw.weighted_ratio([], []), 0.0)

    def test_stage_inner_progress(self):
        keys = ["open", "body_spacing", "save"]
        base = pw.weighted_ratio(keys, ["open"], "body_spacing")
        later = pw.weighted_ratio(keys, ["open"], "body_spacing", stage_elapsed=30)
        self.assertGreater(later, base)
        self.assertLess(pw.weighted_ratio(keys, ["open"], "body_spacing", stage_elapsed=1e9), 43 / 45)
        half = pw.weighted_ratio(keys, ["open"], "body_spacing", reported=(50, 100))
        self.assertAlmostEqual(half, (3 + 20) / 45)

    def test_stage_fraction_edges(self):
        self.assertEqual(pw.stage_fraction(None, 10), 0.0)
        self.assertEqual(pw.stage_fraction(-1, 10), 0.0)
        self.assertEqual(pw.stage_fraction(5, 10, (0, 0)) , pw.stage_fraction(5, 10))
        self.assertEqual(pw.stage_fraction(0, 10, (500, 100)), pw.MAX_STAGE_FRACTION)

    def test_eta_calculating(self):
        for e, r in ((None, .5), (2, .5), (30, .0), (30, .01)):
            self.assertEqual(pw.estimate_remaining(e, r)["state"], "calculating")

    def test_eta_estimate_and_done(self):
        eta = pw.estimate_remaining(60, 0.5)
        self.assertEqual((eta["state"], eta["seconds"]), ("estimate", 60))
        self.assertEqual(pw.estimate_remaining(60, 1.0)["seconds"], 0)

    def test_eta_steady_history_is_point(self):
        hist = [(10, .1), (20, .2), (30, .3)]
        self.assertEqual(pw.estimate_remaining(40, .4, hist)["state"], "estimate")

    def test_eta_unstable_history_is_range(self):
        hist = [(10, .30), (20, .31), (30, .32)]   # 초반에 빨랐다가 느려짐
        eta = pw.estimate_remaining(40, .33, hist)
        self.assertEqual(eta["state"], "range")
        self.assertLess(eta["low"], eta["high"])

    def test_view_ratio_is_weighted(self):
        view = progress_view("all", "body_spacing", ["open", "pre_format", "space", "unify_survey", "standard"],
                             running=True, files=["a.hwpx"], doc=(1, 1), elapsed=60)
        self.assertGreater(view["ratio"], 0.12)
        self.assertEqual(view["eta"]["state"], "estimate")
        self.assertEqual([s["name"] for s in view["steps"]][:2], ["문서 열기", "구조 정리"])

    def test_view_eta_none_when_finished_and_calculating_at_start(self):
        v = progress_view("all", "open", running=True, files=["a.hwpx"], doc=(1, 1), elapsed=1)
        self.assertEqual(v["eta"]["state"], "calculating")
        v = progress_view("all", "done", running=False, files=["a.hwpx"], doc=(1, 1),
                          results=[{"source": "a.hwpx", "name": "b.hwpx"}])
        self.assertEqual(v["eta"]["state"], "none")


if __name__ == "__main__":
    unittest.main()
