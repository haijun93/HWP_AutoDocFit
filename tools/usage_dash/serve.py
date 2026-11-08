"""Start the local usage collector and a loopback-only HTTP server."""
from __future__ import annotations

import subprocess
import sys
import threading
import webbrowser
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

HERE = Path(__file__).resolve().parent


class QuietHandler(SimpleHTTPRequestHandler):
    def log_message(self, fmt: str, *args: object) -> None:
        pass


def main() -> None:
    subprocess.Popen([sys.executable, str(HERE / "collect.py")], cwd=HERE,
                     stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                     creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
    server = ThreadingHTTPServer(("127.0.0.1", 8765), partial(QuietHandler, directory=str(HERE)))
    print("Usage dashboard: http://127.0.0.1:8765")
    threading.Timer(1.0, lambda: webbrowser.open("http://127.0.0.1:8765")).start()
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()


if __name__ == "__main__": main()
