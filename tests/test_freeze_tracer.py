import json
from pathlib import Path
import tempfile
import threading
import time
import unittest

from docfit_app.freeze_tracer import FreezeTracer


class FreezeTracerTests(unittest.TestCase):
    def test_fake_stall_writes_threads_tk_and_recent_api(self):
        with tempfile.TemporaryDirectory() as directory:
            tracer = FreezeTracer(directory, threshold=0.02)
            token = tracer.begin_tk('blocked-test')
            tracer.record_api('sample_api', 0.4, 'timeout')
            time.sleep(0.03)
            path = tracer.check_now()
            self.assertTrue(path.is_file())
            data = json.loads(path.read_text(encoding='utf-8'))
            self.assertIn('tk:blocked-test', data['causes'])
            self.assertEqual(data['tk_pending'][0]['name'], 'blocked-test')
            self.assertEqual(data['recent_api_requests'][-1]['name'], 'sample_api')
            self.assertTrue(any(name.startswith('MainThread') for name in data['threads']))
            tracer.end_tk(token)

    def test_keeps_only_latest_twenty(self):
        with tempfile.TemporaryDirectory() as directory:
            tracer = FreezeTracer(directory, keep=20)
            for index in range(23):
                tracer._write_snapshot((f'test-{index}',), [])
                time.sleep(0.002)
            self.assertEqual(len(list(Path(directory).glob('freeze-*.json'))), 20)

    def test_disabled_has_no_monitor_thread_and_stop_joins(self):
        tracer = FreezeTracer(interval=0.005)
        self.assertFalse(tracer.running)
        self.assertFalse(any(t.name == 'freeze-tracer' for t in threading.enumerate()))
        tracer.start()
        self.assertTrue(tracer.running)
        tracer.stop()
        self.assertFalse(tracer.running)
        self.assertFalse(any(t.name == 'freeze-tracer' for t in threading.enumerate()))

    def test_hung_window_detection_is_sampled(self):
        with tempfile.TemporaryDirectory() as directory:
            tracer = FreezeTracer(directory, hung_checker=lambda window: window == 'hung')
            tracer._window = 'hung'
            path = tracer.check_now()
            self.assertIn('IsHungAppWindow', json.loads(path.read_text(encoding='utf-8'))['causes'])


if __name__ == '__main__':
    unittest.main()
