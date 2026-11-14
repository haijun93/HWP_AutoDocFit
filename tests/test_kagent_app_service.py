"""몽땅클립보드(kagent) → 앱 서비스 파이프: 엑셀 표 '목표 결과' 변환 시험.

kagent 쪽 분기·폴백은 `kagent/src/xl_target.rs` 단위 시험, 여기서는 앱이 돌려주는 값과 두 쪽의 약속(파이프 이름·
설정 키·요청 형식)을 확인한다. 실제 한/글 붙여넣기 결과는 PC 확인이 필요하다.
"""
import base64
import json
import os
import re
import unittest
import uuid
from pathlib import Path

from docfit_app import kagent_app_service as svc
from docfit_core.excel_paste_target import to_target_html_report
from docfit_core.excel_table import parse_clipboard

ROOT = Path(__file__).resolve().parents[1]
SRC = (ROOT / "tests" / "fixtures" / "xlpaste_accounting.cf_html").read_bytes()
RUST = (ROOT / "kagent" / "src" / "xl_target.rs").read_text(encoding="utf-8")
STORE = (ROOT / "kagent" / "src" / "store.rs").read_text(encoding="utf-8")
DIALOG = (ROOT / "flutter_ui" / "lib" / "features" / "kclip" / "kclip_dialog.dart").read_text(encoding="utf-8")


def request(html=SRC, text="부서명\t목\r\n관광정책과\t소계\t                    88,180 \r\n", **extra):
    return dict(cmd="xl_target", html=base64.b64encode(html).decode("ascii"), text=text, **extra)


class HandleTests(unittest.TestCase):
    def test_converts_excel_html_like_the_core(self):
        out = svc.handle(request(SRC + b"\0\0"))  # 클립보드 메모리 끝 NUL
        self.assertTrue(out["ok"])
        self.assertTrue(out["changed"])
        html = base64.b64decode(out["html"])
        expected, _ = to_target_html_report(SRC)
        self.assertEqual(html, expected, "변환 코어(excel_paste_target)와 같은 결과")
        self.assertFalse(html.endswith(b"\0"))
        rows = [[c["text"] for c in r] for r in parse_clipboard(html, clean=False)["rows"]]
        self.assertIn("88,180", [c.strip() for r in rows for c in r])
        self.assertFalse(any(c.startswith(" ") for r in rows for c in r), "숫자 앞 공백 없음")
        self.assertEqual(out["text"], "부서명\t목\r\n관광정책과\t소계\t88,180 \r\n", "글 형식도 같은 공백 규칙")
        self.assertEqual(out["report"]["numeric"], 5)

    def test_shading_option_is_passed(self):
        keep = base64.b64decode(svc.handle(request())["html"])
        strip = base64.b64decode(svc.handle(request(keep_cell_background=False))["html"])
        self.assertNotEqual(keep, strip)

    def test_non_excel_or_bad_input_is_left_alone(self):
        web = b"<html><body><table><tr><td>  1 </td></tr></table></body></html>"
        self.assertEqual(svc.handle(request(web)), dict(ok=True, changed=False, reason="엑셀 표 HTML이 아님"))
        self.assertFalse(svc.handle(request(b"\0"))["changed"])
        self.assertFalse(svc.handle(dict(cmd="xl_target", html="@@not base64@@"))["ok"])
        self.assertFalse(svc.handle(dict(cmd="other"))["ok"])
        self.assertFalse(svc.handle(["xl_target"])["ok"])
        self.assertEqual(svc.handle(dict(cmd="ping")), dict(ok=True, protocol=svc.PROTOCOL))

    def test_open_photo_edit_command_invokes_handler(self):
        called = []
        svc.set_photo_edit_handler(lambda: called.append(True))
        try:
            out = svc.handle(dict(cmd="open_photo_edit"))
            self.assertTrue(out.get("ok"))
            self.assertEqual(called, [True])

            called.clear()
            out_paste = svc.handle(dict(cmd="open_photo_edit_paste"))
            self.assertTrue(out_paste.get("ok"))
            self.assertEqual(called, [True])
        finally:
            svc.set_photo_edit_handler(None)

    def test_handle_line_never_raises(self):
        for line in (b"", b"{", b"\xff\xfe", b"[]", json.dumps(request()).encode()):
            out = json.loads(svc.handle_line(line))
            self.assertIn("ok", out)
        original = svc.handle
        try:
            svc.handle = lambda _r: 1 / 0
            self.assertFalse(json.loads(svc.handle_line(b'{"cmd":"ping"}'))["ok"])
        finally:
            svc.handle = original


class ContractTests(unittest.TestCase):
    def test_pipe_name_matches_kagent(self):
        self.assertIn(f'"\\\\\\\\.\\\\pipe\\\\{svc.PIPE_PREFIX}{{suffix}}.{{}}"', RUST)
        self.assertEqual(svc.pipe_name("S-1-5-21-1"), r"\\.\pipe\HwpAutoDocFit.appsvc.S-1-5-21-1")

    def test_request_keys_match_kagent(self):
        for key in ('("cmd", "xl_target".into())', '("html", base64(html).into())', '("text", text.into())'):
            self.assertIn(key, RUST)
        for key in ('v.bool("ok")', 'v.bool("changed")', 'v.str("html")', 'v.str("text")'):
            self.assertIn(key, RUST)

    def test_setting_key_is_shared_and_defaults_on(self):
        self.assertIn('excel_target_paste: true', STORE)
        self.assertIn("'excel_target_paste'", DIALOG)
        self.assertRegex(DIALOG, r"settings\['excel_target_paste'\]\s*!=\s*false", "관리창도 값이 없으면 켬")


@unittest.skipUnless(os.name == "nt", "Windows 이름 있는 파이프")
class PipeRoundTripTests(unittest.TestCase):
    def test_real_pipe_round_trip(self):
        name = svc.pipe_name(svc._current_sid(), suffix=f".test.{uuid.uuid4().hex}")
        server = svc.AppServicePipe(name).start()
        self.assertTrue(server.ready.wait(5))
        self.assertIsNone(server.failed)
        try:
            for _ in range(2):  # 연결을 끊고 다시 받는지도 본다
                with open(name, "r+b", buffering=0) as stream:
                    stream.write(json.dumps(request()).encode() + b"\n")
                    data = b""
                    while not data.endswith(b"\n"):
                        chunk = stream.read(1 << 16)
                        if not chunk:
                            break
                        data += chunk
                out = json.loads(data)
                self.assertTrue(out["changed"])
                self.assertEqual(base64.b64decode(out["html"]), to_target_html_report(SRC)[0])
            # 같은 이름을 다른 서버가 또 잡지 못한다(첫 인스턴스 표시).
            second = svc.AppServicePipe(name).start()
            self.assertTrue(second.ready.wait(5))
            self.assertIsNotNone(second.failed)
        finally:
            server.stop()


if __name__ == "__main__":
    unittest.main()
