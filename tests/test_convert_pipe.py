import json
import tempfile
import threading
import unittest
from pathlib import Path

from docfit_app.convert_pipe import ConvertPipeReceiver, decode_request


class FakeClient:
    def __init__(self, request):
        self.request = request
        self.response = None
        self.closed = False
    def read(self): return self.request
    def write(self, data): self.response = json.loads(data)
    def close(self): self.closed = True


class ConvertPipeTests(unittest.TestCase):
    def test_json_paths_are_validated_and_added(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / '한글 파일.pdf'
            path.write_bytes(b'pdf')
            calls = []
            result = decode_request(json.dumps({'cmd': 'convert_open', 'target': 'pdf', 'paths': [str(path), 'bad.exe']}).encode(), lambda paths, target: calls.append((paths, target)))
        self.assertTrue(result['ok'])
        self.assertEqual(len(calls[0][0]), 1)
        self.assertEqual(calls[0][1], 'pdf')

    def test_limits_and_invalid_messages(self):
        calls = []
        self.assertFalse(decode_request(b'x' * (1_048_577), lambda *args: calls.append(args))['ok'])
        self.assertFalse(decode_request(json.dumps({'cmd': 'convert_open', 'paths': ['a.exe']}).encode(), lambda *args: calls.append(args))['ok'])
        self.assertFalse(calls)

    def test_receiver_dispatches_and_stops_thread(self):
        ready = threading.Event()
        client = FakeClient(b'{"cmd":"convert_open","target":"pdf","paths":[]}\n')
        def factory(stop):
            if ready.is_set():
                stop.wait(.01)
                return None
            ready.set()
            return client
        receiver = ConvertPipeReceiver(lambda *_: None, server_factory=factory).start()
        for _ in range(100):
            if client.response is not None: break
            threading.Event().wait(.01)
        self.assertEqual(client.response['error'], 'invalid_paths')
        self.assertTrue(receiver.stop())
        self.assertFalse(receiver._thread.is_alive())


if __name__ == '__main__':
    unittest.main()
