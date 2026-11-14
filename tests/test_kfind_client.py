import json
import socket
import struct
import threading
import time
import unittest
from types import SimpleNamespace

from docfit_app.kfind_client import (
    KfindClient, KfindError, KfindTimeout, MAX_REQUEST, TRANSITIONS, _read,
)
from docfit_core.timemachine.kfind_records import doc_from_records


class FakeServer:
    """실제 바이트 스트림·길이 프레임을 쓰는 가짜 KFS1 서버."""
    def __init__(self):
        self.requests = []
        self.status = dict(state="ready", format=3, fold_rule=1, sentence_rule=1,
                           generation=7)
        self.error = None
        self.handler = None
        self.threads = []
        self.errors = []

    def open(self, name):
        if self.error:
            raise self.error
        client, server = socket.socketpair()

        def serve():
            try:
                with server, server.makefile("rwb", buffering=0) as pipe:
                    size = struct.unpack("<I", _read(pipe, 4))[0]
                    assert size <= MAX_REQUEST
                    request = json.loads(_read(pipe, size))
                    self.requests.append(request)
                    result = (self.status if request["command"] == "status" else
                              dict(id=request.get("id"), generation=8))
                    if self.handler:
                        result = self.handler(request, result)
                    if result is None:
                        return
                    data = json.dumps(result if "ok" in result else
                                      dict(ok=True, result=result)).encode()
                    # 응답을 잘게 나눠 짧은 read도 검증한다.
                    frame = struct.pack("<I", len(data)) + data
                    for start in range(0, len(frame), 11):
                        pipe.write(frame[start:start + 11])
            except (BrokenPipeError, ConnectionResetError):
                pass
            except Exception as exc:
                self.errors.append(exc)

        thread = threading.Thread(target=serve, daemon=True)
        self.threads.append(thread)
        thread.start()
        stream = client.makefile("rwb", buffering=0)
        client.close()
        return stream

    def finish(self):
        for thread in self.threads:
            thread.join(2)
        if self.errors:
            raise self.errors[0]


class KfindClientTests(unittest.TestCase):
    def setUp(self):
        self.server = FakeServer()
        self.now = 100.0
        self.client = KfindClient(opener=self.server.open, clock=lambda: self.now)
        self.addCleanup(self.server.finish)

    def test_every_allowed_and_impossible_transition(self):
        # 공개한 상태 표의 36개 조합 모두 판정한다.
        for source in TRANSITIONS:
            for target in TRANSITIONS:
                with self.subTest(source=source, target=target):
                    self.client._phase = source
                    if target in TRANSITIONS[source]:
                        self.client._transition(target)
                        self.assertEqual(self.client.state()["state"], target)
                    else:
                        with self.assertRaises(ValueError):
                            self.client._transition(target)

    def test_handshake_and_all_commands(self):
        self.assertEqual(self.client.state()["state"], "none")
        self.client.query("예산", "tm", top=12, refs=2, filters={"ext": ["hwp"]},
                          request_id=41, split="off", sources=None)
        self.assertEqual([r["command"] for r in self.server.requests], ["status", "query"])
        query = self.server.requests[-1]
        self.assertEqual((query["text"], query["id"], query["scope"]), ("예산", 41, "tm"))
        self.assertNotIn("q", query)
        self.client.count("예산", "nb:1", request_id=42, sources=["2"])
        self.client.cancel(41)
        self.client.remove(["a"])
        self.client.stale("tm", 4, after="a", limit=1)
        self.client.status()
        self.assertEqual(self.client.state()["state"], "connected")
        self.client.close()
        self.assertEqual(self.client.state()["state"], "none")

    def test_connection_failures_and_retry_schedule(self):
        for error, phase, reason in ((FileNotFoundError("missing"), "disconnected", "not_installed"),
                                     (PermissionError("denied"), "refused", "denied"),
                                     (OSError("stopped"), "disconnected", "stopped")):
            with self.subTest(reason=reason):
                self.client.close()
                self.server.error = error
                with self.assertRaises(KfindError):
                    self.client.connect()
                self.assertEqual(self.client.state()["state"], phase)
                self.assertEqual(self.client.state()["reason"], reason)
                self.assertEqual(self.client.state()["retry_at"], self.now + 5)
                with self.assertRaises(KfindError):
                    self.client.query("a", request_id=1)
                self.now += 5
                with self.assertRaises(KfindError):
                    self.client.connect()
                self.assertEqual(self.client.state()["retry_at"], self.now + 30)
                self.now += 30
                self.server.error = None
                self.client.connect()
                self.assertEqual(self.client.state()["state"], "connected")

    def test_version_and_service_state_refused_before_query(self):
        for key in ("format", "fold_rule", "sentence_rule"):
            with self.subTest(key=key):
                self.client.close()
                original = self.server.status[key]
                self.server.status[key] = 999
                with self.assertRaises(KfindError):
                    self.client.query("a", request_id=1)
                self.assertEqual(self.client.state()["reason"], "version_mismatch")
                self.server.status[key] = original
        self.client.close()
        self.server.status["state"] = "rebuilding"
        with self.assertRaises(KfindError):
            self.client.connect()
        self.assertTrue(all(r["command"] == "status" for r in self.server.requests))

    def test_disconnect_server_error_and_wrong_id(self):
        for response in (None, dict(ok=False, error="corrupt"), dict(id=999)):
            with self.subTest(response=response):
                self.client.close()
                self.server.handler = lambda req, result: result if req["command"] == "status" else response
                with self.assertRaises(KfindError):
                    self.client.query("a", request_id=1)
                self.assertEqual(self.client.state()["state"], "disconnected")

    def test_timeout_and_late_response_do_not_restore_connection(self):
        release = threading.Event()
        self.addCleanup(release.set)
        self.client = KfindClient(opener=self.server.open, timeout=0.04)
        self.client.connect()
        def handler(req, result):
            if req["command"] == "query":
                release.wait(1)
            return result
        self.server.handler = handler
        started = time.monotonic()
        with self.assertRaises(KfindTimeout):
            self.client.query("a", request_id=1)
        self.assertLess(time.monotonic() - started, 0.5)
        self.assertEqual(self.client.state()["reason"], "timeout")
        release.set()
        self.server.finish()
        self.assertEqual(self.client.state()["state"], "disconnected")

    def test_cancel_reaches_server_while_query_waits(self):
        entered, release = threading.Event(), threading.Event()
        self.addCleanup(release.set)
        def handler(req, result):
            if req["command"] == "query":
                entered.set()
                release.wait(1)
            if req["command"] == "cancel":
                release.set()
                result["cancelled"] = True
            return result
        self.server.handler = handler
        results = []
        worker = threading.Thread(target=lambda: results.append(self.client.query("a", request_id=1)))
        worker.start()
        self.assertTrue(entered.wait(1))
        self.assertTrue(self.client.cancel(1)["cancelled"])
        worker.join(1)
        self.assertFalse(worker.is_alive())
        self.assertEqual(results[0]["id"], 1)

    def test_ingest_large_unicode_paragraphs_reassemble_exactly(self):
        paras = [dict(p=0, lv=0, t=""), dict(p=1, lv=8, t=('가😀"\\\n' * 20000)),
                 dict(p=2, lv=8, t="끝")]
        doc = dict(doc_key="a", path="a", size=123, mtime=1, date=2, ref_date=None,
                   scopes=["tm"], level_src="py", level_rule=4, paras=paras)
        self.client.ingest(doc)
        parts = [r for r in self.server.requests if r["command"] == "ingest"]
        self.assertGreater(len(parts), 1)
        restored = {}
        for i, part in enumerate(parts):
            self.assertEqual((part["part"], part["parts"]), (i, len(parts)))
            self.assertLessEqual(len(json.dumps(part, ensure_ascii=False, separators=(",", ":")).encode()), MAX_REQUEST)
            for para in part["paras"]:
                restored[para["p"]] = restored.get(para["p"], "") + para["t"]
        self.assertEqual(restored, {p["p"]: p["t"] for p in paras})
        self.client.ingest(dict(doc, paras=[]))
        self.assertEqual(self.server.requests[-1]["paras"], [])

    def test_oversized_request_rejected_before_connect(self):
        with self.assertRaises(ValueError):
            self.client.query("가" * MAX_REQUEST, request_id=1)
        self.assertEqual(self.server.requests, [])

    def test_expired_deadline_never_sends_request(self):
        with self.assertRaises(KfindTimeout):
            self.client._exchange(dict(command="remove", doc_keys=["a"]), self.now)
        self.assertEqual(self.server.requests, [])

    def test_old_connection_failure_cannot_break_new_connection(self):
        self.client.connect()
        old_epoch = self.client._epoch
        self.client._failed(OSError("끊김"), old_epoch)
        self.now += 5
        self.client.connect()
        self.client._failed(OSError("이전 응답"), old_epoch)
        self.assertEqual(self.client.state()["state"], "connected")

    def test_close_during_handshake_discards_late_status(self):
        entered, release = threading.Event(), threading.Event()
        self.addCleanup(release.set)
        errors = []
        def handler(req, result):
            entered.set()
            release.wait(1)
            return result
        self.server.handler = handler
        def connect():
            try:
                self.client.connect()
            except KfindError as exc:
                errors.append(exc)
        worker = threading.Thread(target=connect)
        worker.start()
        self.assertTrue(entered.wait(1))
        self.client.close()
        release.set()
        worker.join(1)
        self.assertEqual(self.client.state()["state"], "none")
        self.assertEqual(len(errors), 1)

    def test_handshake_timeout_is_typed(self):
        release = threading.Event()
        self.addCleanup(release.set)
        self.client = KfindClient(opener=self.server.open, timeout=0.03)
        def handler(req, result):
            release.wait(1)
            return result
        self.server.handler = handler
        with self.assertRaises(KfindTimeout):
            self.client.connect()
        self.assertEqual(self.client.state()["reason"], "timeout")
        release.set()

    def test_record_adapter_has_kfs1_required_metadata(self):
        doc = doc_from_records("a", [], SimpleNamespace(st_size=99, st_ctime=2,
                                                      st_mtime=1.234), "2026-11-13", ["tm"])
        self.assertEqual((doc["size"], doc["mtime"], doc["ref_date"]), (99, 1234, "2026-11-13"))
        self.client.ingest(doc)

    def test_public_operations_observe_retry_and_connection_states(self):
        observed = []
        transition = self.client._transition
        def observe(phase):
            transition(phase)
            observed.append(phase)
        self.client._transition = observe
        self.server.error = PermissionError("denied")
        with self.assertRaises(KfindError):
            self.client.connect()
        self.now += 5
        self.server.error = None
        self.client.connect()
        self.server.error = OSError("broken")
        with self.assertRaises(KfindError):
            self.client.count("a", request_id=10)
        self.now += 5
        self.server.error = None
        self.client.connect()
        self.client.close()
        self.assertEqual(observed, ["connecting", "refused", "retry", "connecting",
                                   "connected", "disconnected", "retry", "connecting",
                                   "connected", "none"])

    def test_status_refresh_detects_version_change(self):
        self.client.connect()
        self.server.status["format"] = 999
        with self.assertRaises(KfindError):
            self.client.status()
        self.assertEqual(self.client.state()["state"], "refused")

    def test_failed_ingest_stops_before_next_part(self):
        self.server.handler = lambda req, result: (
            dict(ok=False, error="upload rejected") if req["command"] == "ingest" else result)
        doc = dict(doc_key="a", path="a", size=1, mtime=1, date=1,
                   scopes=["tm"], level_src="py", level_rule=4,
                   paras=[dict(p=0, lv=8, t="가" * 50000)])
        with self.assertRaises(KfindError):
            self.client.ingest(doc)
        self.assertEqual(len([r for r in self.server.requests if r["command"] == "ingest"]), 1)

    def test_permission_loss_after_connection_is_refused(self):
        self.client.connect()
        self.server.error = PermissionError("denied")
        with self.assertRaises(KfindError):
            self.client.query("a", request_id=1)
        self.assertEqual(self.client.state()["state"], "refused")

    def test_invalid_timeout_rejected(self):
        for timeout in (0, -1, float("inf"), float("nan")):
            with self.subTest(timeout=timeout), self.assertRaises(ValueError):
                KfindClient(timeout=timeout)
