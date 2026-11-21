"""Flutter(WebView)가 부르는 모든 API 이름이 DesktopWebBridge에 있는지 검사한다.

ApplicationService.dispatch에만 있고 DesktopWebBridge에 없으면 실제 앱 화면이
"앱 연결을 기다리고 있습니다" 오류를 낸다(문서뷰·아웃라이너·PDF 편집·Dock 사고, 2026-11-19)."""
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
# 웹 화면이 부르지 않는(죽은 코드이거나 네이티브 전용) 이름은 이유와 함께 여기에 적는다.
NOT_A_WEB_CALL = set()


def flutter_api_names():
    names = set()
    for path in (ROOT / "flutter_ui" / "lib").rglob("*.dart"):
        text = path.read_text(encoding="utf-8")
        names |= set(re.findall(r"api\(\s*'([a-z_0-9]+)'", text))
        names |= set(re.findall(r"callApi\(\s*'([a-z_0-9]+)'", text))
    return names


def bridge_method_names():
    text = (ROOT / "desktop_web_ui.py").read_text(encoding="utf-8")
    return set(re.findall(r"^    def ([a-z_0-9]+)\(", text, re.M))


class BridgeMethodsTests(unittest.TestCase):
    def test_every_flutter_api_name_exists_on_the_web_bridge(self):
        missing = sorted(n for n in flutter_api_names() - NOT_A_WEB_CALL if n not in bridge_method_names())
        self.assertEqual(missing, [], f"DesktopWebBridge에 없는 API 이름: {missing}")


def browser_api_names():
    """실제 WebView가 JS로 부를 수 있는 이름(_BrowserApi 허용 목록)."""
    text = (ROOT / "desktop_web_ui.py").read_text(encoding="utf-8")
    start = text.index("class _BrowserApi:")
    end = text.index("setattr(self, name, getattr(bridge, name))", start)
    return set(re.findall(r'"([a-z_0-9]+)"', text[start:end]))


class BrowserApiAllowlistTests(unittest.TestCase):
    def test_every_flutter_api_name_is_exposed_to_the_webview(self):
        """브리지에 메서드가 있어도 _BrowserApi 허용 목록에 없으면 화면이 '앱 연결을 기다리고 있습니다'를 낸다(v2.2.81 문서뷰 사고)."""
        missing = sorted(n for n in flutter_api_names() - NOT_A_WEB_CALL if n not in browser_api_names())
        self.assertEqual(missing, [], f"_BrowserApi 허용 목록에 없는 API 이름: {missing}")


class BridgeWrapperBehaviorTests(unittest.TestCase):
    """새 브리지 메서드가 실제로 서비스를 부르는지(임시 설정 폴더, 앱·Tk 없이)."""

    def _bridge(self, folder):
        import desktop_web_ui
        from docfit_app.settings_service import SettingsService
        bridge = desktop_web_ui.DesktopWebBridge.__new__(desktop_web_ui.DesktopWebBridge)
        store = SettingsService(Path(folder) / "settings.json")
        bridge._settings_store = lambda: store
        return bridge, store

    def test_menu_display_mode_is_saved_and_validated(self):
        import tempfile
        with tempfile.TemporaryDirectory() as folder:
            bridge, store = self._bridge(folder)
            self.assertEqual(bridge.set_menu_display_mode("dock"), {"menu_display_mode": "dock"})
            self.assertEqual(store.read().get("menu_display_mode"), "dock")
            with self.assertRaises(ValueError):
                bridge.set_menu_display_mode("floating")

    def test_kclip_settings_mode_defaults_to_dialog_and_accepts_native(self):
        import tempfile
        with tempfile.TemporaryDirectory() as folder:
            bridge, store = self._bridge(folder)
            self.assertEqual(bridge.kclip_window("settings_mode"), {"ok": True, "mode": "dialog"})
            self.assertEqual(bridge.kclip_window("settings_mode", {"value": "native"}), {"ok": True, "mode": "native"})
            self.assertEqual(store.read().get("kclip_settings_mode"), "native")
            self.assertFalse(bridge.kclip_window("settings_mode", {"value": "other"})["ok"])

    def test_outliner_ui_setting_goes_through_the_existing_outliner_bridge(self):
        """새 브리지 메서드 없이 기존 `outliner` 로 화면 방식을 읽고 쓴다(허용 목록·dispatch 개수 변경 없음)."""
        import tempfile
        with tempfile.TemporaryDirectory() as folder:
            bridge, store = self._bridge(folder)
            first = bridge.outliner({"action": "ui_mode"})
            self.assertTrue(first["ok"])
            self.assertEqual(first["data"], {"mode": "flutter"})
            self.assertEqual(bridge.outliner({"action": "ui_mode", "value": "native"})["data"], {"mode": "native"})
            self.assertEqual(store.read().get("outliner_ui"), "native")
            self.assertFalse(bridge.outliner({"action": "ui_mode", "value": "x"})["ok"])
        self.assertIn("outliner", browser_api_names())

    def test_settings_window_bridge_reads_and_writes_settings_ui(self):
        """새 브리지 메서드 `settings_window`: 화면 방식 읽기·쓰기. 기본 'flutter' 에서는 창을 열지 않는다."""
        import tempfile
        from docfit_app.api_contract import ServiceError
        with tempfile.TemporaryDirectory() as folder:
            bridge, store = self._bridge(folder)
            first = bridge.settings_window({"action": "ui_mode"})
            self.assertTrue(first["ok"])
            self.assertEqual(first["data"], {"mode": "flutter"})
            self.assertEqual(bridge.settings_window({"action": "open_if_native"})["data"], {"mode": "flutter", "opened": False})
            self.assertEqual(bridge.settings_window({"action": "ui_mode", "value": "native"})["data"], {"mode": "native"})
            self.assertEqual(store.read().get("settings_ui"), "native")
            self.assertFalse(bridge.settings_window({"action": "ui_mode", "value": "x"})["ok"])
            with self.assertRaises(ServiceError):
                bridge.settings_window("not a dict")
        self.assertIn("settings_window", browser_api_names())

    def test_docview_window_bridge_reads_and_writes_docview_ui(self):
        """새 브리지 메서드 `docview_window`: 문서뷰 화면 방식 읽기·쓰기. 기본 'flutter' 에서는 창을 열지 않는다."""
        import tempfile
        from docfit_app.api_contract import ServiceError
        with tempfile.TemporaryDirectory() as folder:
            bridge, store = self._bridge(folder)
            first = bridge.docview_window({"action": "ui_mode"})
            self.assertTrue(first["ok"])
            self.assertEqual(first["data"], {"mode": "flutter"})
            self.assertEqual(bridge.docview_window({"action": "open_if_native"})["data"], {"mode": "flutter", "opened": False})
            self.assertEqual(bridge.docview_window({"action": "ui_mode", "value": "native"})["data"], {"mode": "native"})
            self.assertEqual(store.read().get("docview_ui"), "native")
            self.assertFalse(bridge.docview_window({"action": "ui_mode", "value": "x"})["ok"])
            with self.assertRaises(ServiceError):
                bridge.docview_window("not a dict")
        self.assertIn("docview_window", browser_api_names())

    def test_photoview_window_bridge_reads_and_writes_photoview_ui(self):
        """새 브리지 메서드 `photoview_window`: 사진뷰 화면 방식 읽기·쓰기. 기본 'flutter' 에서는 창을 열지 않는다."""
        import tempfile
        from docfit_app.api_contract import ServiceError
        with tempfile.TemporaryDirectory() as folder:
            bridge, store = self._bridge(folder)
            first = bridge.photoview_window({"action": "ui_mode"})
            self.assertTrue(first["ok"])
            self.assertEqual(first["data"], {"mode": "flutter"})
            self.assertEqual(bridge.photoview_window({"action": "open_if_native"})["data"], {"mode": "flutter", "opened": False})
            self.assertEqual(bridge.photoview_window({"action": "ui_mode", "value": "native"})["data"], {"mode": "native"})
            self.assertEqual(store.read().get("photoview_ui"), "native")
            self.assertFalse(bridge.photoview_window({"action": "ui_mode", "value": "x"})["ok"])
            with self.assertRaises(ServiceError):
                bridge.photoview_window("not a dict")
        self.assertIn("photoview_window", browser_api_names())

    def test_outliner_and_pdf_edit_reject_non_dict_payloads(self):
        import tempfile
        from docfit_app.api_contract import ServiceError
        with tempfile.TemporaryDirectory() as folder:
            bridge, _ = self._bridge(folder)
            with self.assertRaises(ServiceError):
                bridge.outliner("not a dict")
            with self.assertRaises(ServiceError):
                bridge.pdf_edit(None)

    def test_qms_bench_requires_developer_mode(self):
        import tempfile
        with tempfile.TemporaryDirectory() as folder:
            bridge, _ = self._bridge(folder)
            result = bridge.qms_bench({"action": "start"})
            self.assertIsInstance(result, dict)
            self.assertFalse(result.get("ok"))
            self.assertEqual((result.get("error") or {}).get("code"), "DEVELOPER_MODE_REQUIRED")

    def test_doc_grid_returns_a_dict_for_an_unknown_action(self):
        import tempfile
        with tempfile.TemporaryDirectory() as folder:
            bridge, _ = self._bridge(folder)
            result = bridge.doc_grid("no_such_action", {})
            self.assertIsInstance(result, dict)
            self.assertFalse(result.get("ok", False))


if __name__ == "__main__":
    unittest.main()
