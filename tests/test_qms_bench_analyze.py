import json
import tempfile
import unittest
from pathlib import Path
from scripts import qms_bench_analyze as qa

FIX = Path(__file__).parent / "fixtures" / "qms_bench_analyze"
def read(name): return qa.load_result(FIX / name)[0]

class BenchAnalyzeTests(unittest.TestCase):
    def test_01_percentile_median(self): self.assertEqual(qa.pct([1,2,3],.5),2)
    def test_02_percentile_interpolates(self): self.assertEqual(qa.pct([0,10],.95),9.5)
    def test_03_empty_percentile(self): self.assertIsNone(qa.pct([],.5))
    def test_04_result_band_zero(self): self.assertEqual(qa.result_band(0),"0건")
    def test_05_result_band_ten(self): self.assertEqual(qa.result_band(10),"1–10건")
    def test_06_result_band_boundary(self): self.assertEqual(qa.result_band(100),"11–100건")
    def test_07_native_stats(self): self.assertEqual(qa.query_metrics(read("base.json")["queries"][0])[1]["p50"],10)
    def test_08_ident_hash(self): self.assertEqual(qa.ident(read("base.json")["queries"][0])["hash"],"sha256:aaa")
    def test_09_privacy_label(self): self.assertNotIn("query_text",qa.qlabel(qa.ident(read("base.json")["queries"][0])))
    def test_10_schema_one(self): self.assertEqual(read("base.json")["schema_version"],1)
    def test_11_unknown_schema_rejected(self):
        x=read("base.json"); x["schema_version"]=9
        with self.assertRaises(qa.InputError): qa.load_result(self._temp_json(x))
    def test_12_missing_required_rejected(self):
        x=read("base.json"); del x["queries"]
        with self.assertRaises(qa.InputError): qa.load_result(self._temp_json(x))
    def test_13_current_writer_emits_warnings_for_metadata(self): self.assertTrue(qa.load_result(FIX/"base.json")[1])
    def test_14_fast_win(self):
        q=read("everything.json")["queries"][0]
        self.assertEqual(qa.stats(q,"kfind_names")["p50"],4)
        self.assertEqual(qa.stats(q,"everything")["p50"],2)
    def test_15_discrepancy_count_and_match(self): self.assertEqual(len(qa.analyze([read("everything.json")])[1]),1)
    def test_16_slow_sort(self):
        slow=qa.analyze([read("everything.json")])[2]
        self.assertGreater(slow[0][0],slow[1][0])
    def test_17_error_timeout_list(self): self.assertEqual(len(qa.analyze([read("current.json")])[3]),2)
    def test_18_regression_sample_hold(self):
        reg=qa.regression([read("base.json"),read("current.json")]); self.assertTrue(reg); self.assertTrue(all(x[2]=="판정 보류" for x in reg))
    def test_19_resource_reason(self): self.assertEqual(qa.resource_info(read("everything.json"))[0]["unavailable_reason"],"access_denied")
    def test_20_report_omits_query_text(self):
        report=qa.render_report([read("base.json"),read("current.json")]); self.assertNotIn("비밀검색어",report); self.assertIn("TODO 후보",report)
    def test_21_cli_writes_report(self):
        with tempfile.TemporaryDirectory() as td:
            out=Path(td)/"r.md"; rc=qa.main([str(FIX/"base.json"),"--out",str(out)])
            self.assertIn(rc,(0,1)); self.assertTrue(out.is_file())
    def test_22_malformed_input_message_korean(self):
        with tempfile.TemporaryDirectory() as td:
            p=Path(td)/"bad.json"; p.write_text("{")
            with self.assertRaises(qa.InputError): qa.load_result(p)
    def test_23_paired_synthetic_adapters_different_latencies_and_correct_verdict(self):
        kfind_run = {
            "schema_version": 1,
            "run": {"run_id": "run-kfind", "adapter": "kfind", "query_set_version": "set-synth", "repeats": 5, "started_at": "2026-11-21T00:00:00Z", "machine": {}, "indexes": {}},
            "queries": [
                {"digest": {"id": "q1", "kind": "korean_2char", "query_length": 2, "query_hash": "sha256:q1", "query_text": None}, "statistics": {"count": 5, "p50": 5.0, "p95": 8.0}, "result_count": 0, "timeouts": 0, "errors": 0},
                {"digest": {"id": "q2", "kind": "korean_3char", "query_length": 3, "query_hash": "sha256:q2", "query_text": None}, "statistics": {"count": 5, "p50": 5.0, "p95": 9.0}, "result_count": 5, "timeouts": 0, "errors": 0},
                {"digest": {"id": "q3", "kind": "korean_4char", "query_length": 4, "query_hash": "sha256:q3", "query_text": None}, "statistics": {"count": 5, "p50": 5.0, "p95": 10.0}, "result_count": 50, "timeouts": 0, "errors": 0},
                {"digest": {"id": "q4", "kind": "korean_5char", "query_length": 5, "query_hash": "sha256:q4", "query_text": None}, "statistics": {"count": 5, "p50": 5.0, "p95": 11.0}, "result_count": 500, "timeouts": 0, "errors": 0},
            ],
            "resources": {"status": "available"},
            "summary": {"completed": True}
        }
        ev_run = {
            "schema_version": 1,
            "run": {"run_id": "run-ev", "adapter": "everything", "query_set_version": "set-synth", "repeats": 5, "started_at": "2026-11-21T00:00:00Z", "machine": {}, "indexes": {}},
            "queries": [
                {"digest": {"id": "q1", "kind": "korean_2char", "query_length": 2, "query_hash": "sha256:q1", "query_text": None}, "statistics": {"count": 5, "p50": 50.0, "p95": 80.0}, "result_count": 0, "timeouts": 0, "errors": 0},
                {"digest": {"id": "q2", "kind": "korean_3char", "query_length": 3, "query_hash": "sha256:q2", "query_text": None}, "statistics": {"count": 5, "p50": 50.0, "p95": 90.0}, "result_count": 5, "timeouts": 0, "errors": 0},
                {"digest": {"id": "q3", "kind": "korean_4char", "query_length": 4, "query_hash": "sha256:q3", "query_text": None}, "statistics": {"count": 5, "p50": 50.0, "p95": 100.0}, "result_count": 50, "timeouts": 0, "errors": 0},
                {"digest": {"id": "q4", "kind": "korean_5char", "query_length": 5, "query_hash": "sha256:q4", "query_text": None}, "statistics": {"count": 5, "p50": 50.0, "p95": 110.0}, "result_count": 500, "timeouts": 0, "errors": 0},
            ],
            "resources": {"status": "available"},
            "summary": {"completed": True}
        }
        report = qa.render_report([kfind_run, ev_run])
        # 1. 두 엔진의 p50이 분리되어 보고서에 5.0과 50.0이 각각 표시됨
        self.assertIn("5.0", report)
        self.assertIn("50.0", report)
        # 2. 비율이 1.00x가 아니라 약 10배(0.10x)로 계산됨
        self.assertIn("0.10x", report)
        self.assertNotIn("1.00x", report)
        # 3. '무승부'가 아니라 몽땅검색이 10배 빠르므로 올바른 판정 '승'
        self.assertNotIn("| 무승부 |", report)
        self.assertIn("| 승 |", report)
        # 4. 보고서 끝 판정 기준 및 임계값(1.20) 명시
        self.assertIn("## 판정 기준", report)
        self.assertIn(f"{qa.REGRESSION_RATIO:.2f}x", report)

    def test_24_paired_synthetic_adapters_reverse_order(self):
        kfind_run = {
            "schema_version": 1,
            "run": {"run_id": "run-kfind", "adapter": "kfind", "query_set_version": "set-synth", "repeats": 5},
            "queries": [
                {"digest": {"id": "q1", "kind": "korean_2char", "query_length": 2, "query_hash": "sha256:q1"}, "statistics": {"count": 5, "p50": 5.0, "p95": 8.0}, "result_count": 10, "timeouts": 0, "errors": 0},
            ],
            "resources": {"status": "available"},
            "summary": {"completed": True}
        }
        ev_run = {
            "schema_version": 1,
            "run": {"run_id": "run-ev", "adapter": "everything", "query_set_version": "set-synth", "repeats": 5},
            "queries": [
                {"digest": {"id": "q1", "kind": "korean_2char", "query_length": 2, "query_hash": "sha256:q1"}, "statistics": {"count": 5, "p50": 50.0, "p95": 80.0}, "result_count": 10, "timeouts": 0, "errors": 0},
            ],
            "resources": {"status": "available"},
            "summary": {"completed": True}
        }
        report = qa.render_report([ev_run, kfind_run])
        self.assertIn("0.10x", report)
        self.assertNotIn("1.00x", report)
        self.assertIn("| 승 |", report)

    def test_25_single_adapter_no_fake_tie(self):
        kfind_run = {
            "schema_version": 1,
            "run": {"run_id": "run-kfind", "adapter": "kfind", "query_set_version": "set-synth", "repeats": 5},
            "queries": [
                {"digest": {"id": "q1", "kind": "korean_2char", "query_length": 2, "query_hash": "sha256:q1"}, "statistics": {"count": 5, "p50": 5.0, "p95": 8.0}, "result_count": 10, "timeouts": 0, "errors": 0},
            ],
            "resources": {"status": "available"},
            "summary": {"completed": True}
        }
        report = qa.render_report([kfind_run])
        self.assertNotIn("| 무승부 |", report)
        self.assertIn("비교 불가", report)

    def _temp_json(self,obj):
        f=tempfile.NamedTemporaryFile(mode="w",encoding="utf-8",suffix=".json",delete=False)
        json.dump(obj,f); f.close(); return f.name

if __name__=="__main__": unittest.main()
