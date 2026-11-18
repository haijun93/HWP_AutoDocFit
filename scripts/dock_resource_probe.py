"""Sample Dock CPU time delta and private working set for 60 seconds (not run during DOCK2)."""
from __future__ import annotations

import argparse
import time
from datetime import datetime, timezone

import psutil


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("pid", type=int)
    parser.add_argument("--seconds", type=int, default=60)
    parser.add_argument("--interval", type=float, default=1.0)
    args = parser.parse_args()
    process = psutil.Process(args.pid)
    previous = process.cpu_times()
    start = time.monotonic()
    print("utc,elapsed_s,cpu_delta_s,private_working_set_bytes")
    while time.monotonic() - start < args.seconds:
        time.sleep(args.interval)
        current = process.cpu_times()
        memory = process.memory_info()
        private = getattr(memory, "private", getattr(memory, "wset", 0))
        elapsed = time.monotonic() - start
        print(f"{datetime.now(timezone.utc).isoformat()},{elapsed:.3f},"
              f"{current.user + current.system - previous.user - previous.system:.6f},{private}")
        previous = current
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
