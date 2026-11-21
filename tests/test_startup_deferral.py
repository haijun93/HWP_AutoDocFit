import threading
import unittest

from docfit_app import startup_deferral as sd


class DeferredStartupTests(unittest.TestCase):
    def test_not_run_before_trigger_and_run_after(self):
        done = threading.Event()
        d = sd.DeferredStartup([done.set])
        self.assertFalse(done.wait(0.1))
        self.assertTrue(d.trigger())
        self.assertTrue(done.wait(2))

    def test_trigger_once(self):
        count = []
        d = sd.DeferredStartup([lambda: count.append(1)])
        self.assertTrue(d.trigger())
        self.assertFalse(d.trigger())

    def test_exception_does_not_propagate(self):
        ok = threading.Event()

        def boom():
            raise RuntimeError("x")
        d = sd.DeferredStartup([boom, ok.set])
        d.trigger()
        self.assertTrue(ok.wait(2))

    def test_summary_has_no_paths(self):
        sd.mark("unit_test_mark")
        self.assertIn("unit_test_mark=", sd.summary())


class OrderingSourceTests(unittest.TestCase):
    def test_main_no_longer_starts_threads_before_window(self):
        src = open("hwp-auto-docfit.py", encoding="utf-8").read()
        self.assertNotIn('name="kfind-ensure"', src)
        self.assertNotIn('name="kordoc-deploy"', src)
        self.assertIn("desktop_web_ui.DEFERRED_STARTUP", src)

    def test_loaded_handler_triggers_deferred(self):
        src = open("desktop_web_ui.py", encoding="utf-8").read()
        self.assertIn("window.events.loaded += _on_loaded", src)
        self.assertIn("DEFERRED_STARTUP.trigger()", src)


if __name__ == "__main__":
    unittest.main()
