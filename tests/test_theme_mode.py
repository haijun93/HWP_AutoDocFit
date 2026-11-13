import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace

from docfit_app.application_service import ApplicationService
from docfit_app.kagent_client import KAgentError
from docfit_app.kclip_service import push_theme
from docfit_app.settings_schema import DEFAULTS, normalize_theme_mode
from docfit_app.settings_service import SettingsService


class FakeClient:
    def __init__(self, running, fail=False):
        self._running = running
        self.fail = fail
        self.requests = []

    def running(self):
        return self._running

    def request(self, cmd, **payload):
        if self.fail:
            raise KAgentError("연결 실패")
        self.requests.append((cmd, payload))
        return {"ok": True}


class ThemeModeServiceTest(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory()
        self.path = Path(self.folder.name) / "settings.json"
        self.service = ApplicationService(self.path)
        self.client = FakeClient(running=False)
        self.service._kagent_client = self.client

    def tearDown(self):
        self.folder.cleanup()

    def test_default_is_system(self):
        self.assertEqual(DEFAULTS["theme_mode"], "system")
        self.assertEqual(self.service.dispatch("get_state", [])["theme_mode"], "system")

    def test_three_values_saved_and_survive_restart(self):
        for mode in ("light", "dark", "system"):
            self.assertEqual(self.service.dispatch("set_theme_mode", [mode]), {"theme_mode": mode})
            self.assertEqual(self.service.dispatch("get_state", [])["theme_mode"], mode)
            self.assertEqual(SettingsService(self.path).read()["theme_mode"], mode)
        self.service.dispatch("set_theme_mode", ["dark"])
        reloaded = ApplicationService(self.path)
        self.assertEqual(reloaded.dispatch("get_state", [])["theme_mode"], "dark")

    def test_invalid_value_rejected_and_not_saved(self):
        self.service.dispatch("set_theme_mode", ["light"])
        for bad in ("hc", "", None, 1, "DARK"):
            with self.assertRaises(ValueError):
                self.service.dispatch("set_theme_mode", [bad])
        self.assertEqual(self.service.dispatch("get_state", [])["theme_mode"], "light")

    def test_corrupt_saved_value_reads_as_system(self):
        SettingsService(self.path).update(lambda current: current.update({"theme_mode": "purple"}))
        self.assertEqual(self.service.dispatch("get_state", [])["theme_mode"], "system")
        self.assertEqual(normalize_theme_mode(None), "system")

    def test_kagent_off_is_skipped(self):
        self.service.dispatch("set_theme_mode", ["dark"])
        self.assertEqual(self.client.requests, [])

    def test_kagent_on_receives_theme(self):
        self.client._running = True
        self.service.dispatch("set_theme_mode", ["dark"])
        self.assertEqual(self.client.requests, [("settings_set", {"settings": {"theme": "dark"}})])

    def test_kagent_error_is_swallowed(self):
        self.service._kagent_client = FakeClient(running=True, fail=True)
        self.assertEqual(self.service.dispatch("set_theme_mode", ["light"]), {"theme_mode": "light"})
        self.assertFalse(push_theme("light", FakeClient(running=True, fail=True)))


class ThemeModeDesktopBridgeTest(unittest.TestCase):
    def test_bridge_saves_and_forwards(self):
        from desktop_web_ui import DesktopWebBridge, _BrowserApi

        with tempfile.TemporaryDirectory() as folder:
            store = SettingsService(Path(folder) / "settings.json")
            gui = SimpleNamespace(_준말_서비스=lambda: SimpleNamespace(settings=store))
            bridge = DesktopWebBridge.__new__(DesktopWebBridge)
            bridge.gui = gui
            client = FakeClient(running=True)
            bridge._kagent_client = client
            self.assertEqual(bridge._theme_mode_value(), "system")
            self.assertEqual(bridge.set_theme_mode("dark"), {"theme_mode": "dark"})
            self.assertEqual(bridge._theme_mode_value(), "dark")
            self.assertEqual(store.read()["theme_mode"], "dark")
            self.assertEqual(client.requests, [("settings_set", {"settings": {"theme": "dark"}})])
            with self.assertRaises(ValueError):
                bridge.set_theme_mode("blue")
            self.assertEqual(store.read()["theme_mode"], "dark")
            self.assertIs(_BrowserApi(bridge).set_theme_mode.__func__, DesktopWebBridge.set_theme_mode)


if __name__ == "__main__":
    unittest.main()
