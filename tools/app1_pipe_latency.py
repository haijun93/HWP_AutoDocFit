"""APP1-E 1회 실행 도구: 가짜 KFS1 바이트 파이프, IPC 전역 잠금 포함.

실제 서비스/Flutter/디스크 속도 측정이 아니다. 사용자 문서를 읽지 않는다.
"""
import json
import socket
import statistics
import threading
import time
from types import SimpleNamespace

from docfit_app.ipc_server import IpcServer
from docfit_app.kfind_client import KfindClient
from tests.test_kfind_client import FakeServer


def measure():
    fake = FakeServer()
    entered, release = threading.Event(), threading.Event()

    def handler(request, result):
        if request["command"] == "query" and request["id"] == 900:
            entered.set()
            release.wait(1)
        if request["command"] == "cancel":
            release.set()
        return result

    fake.handler = handler
    client = KfindClient(opener=fake.open)
    client.connect()

    class App:
        lock = threading.RLock()
        data = SimpleNamespace(closing=False)

        def dispatch(self, method, args):
            return getattr(client, method)("예산", request_id=args[0])

    server = IpcServer(App(), "measurement")
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()

    def rpc(method, number):
        with socket.create_connection(server.server_address) as conn, conn.makefile("rwb") as stream:
            request = dict(protocol_version=1, request_id=str(number), token="measurement",
                           method=method, params=dict(args=[number]))
            start = time.perf_counter()
            stream.write(json.dumps(request).encode() + b"\n")
            stream.flush()
            response = json.loads(stream.readline())
            assert response["success"], response
            return (time.perf_counter() - start) * 1000

    try:
        measurements = {}
        for method in ("count", "query"):
            values = [rpc(method, number) for number in range(40)]
            measurements[method] = dict(n=len(values), median_ms=statistics.median(values),
                p95_ms=sorted(values)[37], max_ms=max(values))
        slow = threading.Thread(target=lambda: client.query("예산", request_id=900))
        slow.start()
        assert entered.wait(2)
        start = time.perf_counter()
        client.cancel(900)
        measurements["cancel_ms"] = (time.perf_counter() - start) * 1000
        measurements["next_query_ms"] = rpc("query", 901)
        slow.join(2)
        assert not slow.is_alive()
        return measurements
    finally:
        release.set()
        server.shutdown()
        server.server_close()
        thread.join(2)
        client.close()
        fake.finish()


if __name__ == "__main__":
    print(json.dumps(measure(), ensure_ascii=False, indent=2))
