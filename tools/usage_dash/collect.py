"""Collect local usage snapshots. No collected data leaves this machine."""
from __future__ import annotations

import json
import os
import re
import subprocess
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Iterable

HERE = Path(__file__).resolve().parent
HOME = Path.home()
REPORTS = Path(os.environ.get("USAGEDASH_REPORTS", r"C:\HWP_AUTODOCFIT\v1803\reports"))
CODEX_DIRS = [("PC · codex", HOME / ".codex"), ("PC · ngaytot9", HOME / ".codex-ngaytot9"), ("PC · 2be", HOME / ".codex-2be")]
CLAUDE_DIRS = [("PC · claude", HOME / ".claude"), ("PC · ngaytot9", HOME / ".claude-ngaytot9")]
MAC_CODEX = "hyeokjunkong@192.168.0.4"
AGY = [("PC · haijun2be", REPORTS, "haijun2be"), ("Mac Studio · haijun93", None, "haijun93"), ("MacBook Air · ngaytot9", None, "ngaytot9")]


def read_jsonl(path: Path) -> Iterable[dict[str, Any]]:
    try:
        with path.open("r", encoding="utf-8", errors="replace") as stream:
            for line in stream:
                try:
                    value = json.loads(line)
                    if isinstance(value, dict):
                        yield value
                except (ValueError, TypeError):
                    continue
    except OSError:
        return


def latest_codex_event(paths: Iterable[Path]) -> dict[str, Any] | None:
    events = []
    for path in paths:
        for row in read_jsonl(path):
            payload = row.get("payload", {})
            info = payload.get("info") or {} if isinstance(payload, dict) else {}
            limits = payload.get("rate_limits") if isinstance(payload, dict) else None
            if row.get("type") == "event_msg" and info.get("total_token_usage") and limits:
                events.append((row.get("timestamp", ""), limits))
    return max(events, key=lambda item: item[0])[1] if events else None


def codex_card(name: str, root: Path, remote: bool = False) -> dict[str, Any]:
    try:
        if remote:
            cmd = "python3 -c 'import pathlib,json; d=pathlib.Path.home()/\".codex/sessions\"; ev=[]; [ev.append((r.get(\"timestamp\",\"\"),r.get(\"payload\",{}).get(\"rate_limits\"))) for p in d.rglob(\"*.jsonl\") for l in p.open(errors=\"replace\") if l.strip() for r in [json.loads(l)] if r.get(\"type\")==\"event_msg\" and r.get(\"payload\",{}).get(\"info\",{}).get(\"total_token_usage\") and r.get(\"payload\",{}).get(\"rate_limits\")]; print(json.dumps(max(ev,key=lambda x:x[0])[1]) if ev else \"null\")'"
            result = subprocess.run(["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=3", MAC_CODEX, cmd], capture_output=True, text=True, timeout=20)
            event = json.loads(result.stdout.strip()) if result.returncode == 0 and result.stdout.strip() else None
        else:
            event = latest_codex_event(root.joinpath("sessions").rglob("*.jsonl")) if root.exists() else None
        if not event:
            return {"name": name, "status": "세션 또는 rate_limits 없음", "primary": None, "secondary": None}
        def window(key: str) -> dict[str, Any] | None:
            item = event.get(key)
            return {"used_percent": item.get("used_percent"), "resets_at": item.get("resets_at"), "window_minutes": item.get("window_minutes")} if item else None
        return {"name": name, "status": "정확 · 최신 rate_limits 이벤트", "primary": window("primary"), "secondary": window("secondary")}
    except (OSError, subprocess.SubprocessError):
        return {"name": name, "status": "읽기 실패/SSH 연결 불가", "primary": None, "secondary": None}


def remote_gemini_card(name: str, account: str) -> dict[str, Any]:
    # Search only the remote account's home reports directory; emit aggregate metadata, never log contents.
    script = "import pathlib,re,json,datetime; h=pathlib.Path.home(); roots=[h/\"reports\",h/\"work/hwp/reports\"]; fs=[p for r in roots if r.exists() for p in r.rglob(\"*.log\") if re.search(r\"agy|maccoder\",p.name,re.I)]; lim=[]; workers={}; runs=0; now=datetime.datetime.now().timestamp(); pat=re.compile(r\"\\b(429|quota|rate.?limit|usage.?limit|resource.?exhausted|too many requests)\\b\",re.I); run=re.compile(r\"\\b(agy|maccoder)\\b\",re.I);\nfor p in fs:\n t=p.stat().st_mtime; s=p.read_text(errors=\"replace\"); runs += (len(run.findall(s)) or 1) if t>=now-7*86400 else 0;\n for line in s.splitlines():\n  if pat.search(line): lim.append((t,p.name))\n if t>=now-2*86400: workers[p.stem]=dict(name=p.stem,log=p.name,modified=datetime.datetime.fromtimestamp(t).astimezone().isoformat())\nprint(json.dumps(dict(runs=runs,limit_at=datetime.datetime.fromtimestamp(max(lim)[0]).astimezone().isoformat() if lim else None,limit_log=max(lim)[1] if lim else None,alert=bool(lim),workers=sorted(workers.values(),key=lambda x:x[\"modified\"],reverse=True)[:8])))"
    try:
        result = subprocess.run(["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=3", MAC_CODEX, "python3 -c " + subprocess.list2cmdline([script])], capture_output=True, text=True, timeout=25)
        if result.returncode != 0: raise OSError(result.stderr.strip())
        data = json.loads(result.stdout.strip())
        return {"name": name, "status": "제공 안 됨", "quota_percent": None, **data}
    except (OSError, ValueError, subprocess.SubprocessError):
        return {"name": name, "status": "SSH 연결 불가 또는 reports 폴더 없음", "quota_percent": None, "limit_at": None, "runs": 0, "workers": []}


def latest_codex_event_from_rows(rows: Iterable[dict[str, Any]]) -> dict[str, Any] | None:
    events = []
    for row in rows:
        payload = row.get("payload", {})
        info = payload.get("info") or {} if isinstance(payload, dict) else {}
        limits = payload.get("rate_limits") if isinstance(payload, dict) else None
        if row.get("type") == "event_msg" and info.get("total_token_usage") and limits:
            events.append((row.get("timestamp", ""), limits))
    return max(events, key=lambda item: item[0])[1] if events else None


def claude_usage(root: Path, now: float) -> dict[str, Any]:
    cutoff5, cutoff7 = now - 5 * 3600, now - 7 * 86400
    total5 = total7 = 0
    alerted = False
    sessions = 0
    if root.exists():
        for path in root.rglob("*.jsonl"):
            try:
                if path.stat().st_mtime < cutoff7: continue
                session_tokens = 0
                for row in read_jsonl(path):
                    stamp = row.get("timestamp")
                    ts = parse_time(stamp)
                    payload = row.get("payload", {})
                    usage = row.get("message", {}).get("usage", {}) if isinstance(row.get("message"), dict) else {}
                    usage = usage or (payload.get("message", {}).get("usage", {}) if isinstance(payload, dict) else {})
                    amount = sum(int(usage.get(k, 0) or 0) for k in ("input_tokens", "output_tokens", "cache_read_input_tokens", "cache_creation_input_tokens"))
                    if amount:
                        session_tokens += amount
                        if ts and ts >= cutoff7: total7 += amount
                        if ts and ts >= cutoff5: total5 += amount
                    serialized = json.dumps(row, ensure_ascii=False).lower()
                    if re.search(r"rate.?limit|usage limit|limit reached|overloaded_error", serialized): alerted = True
                sessions += int(session_tokens > 0)
            except OSError: continue
    return {"name": root.name, "tokens_5h": total5, "tokens_7d": total7, "sessions_7d": sessions,
            "estimate": True, "alert": alerted, "status": "로그 토큰 합계 · 한도 % 추정" + (" · 제한 문구 감지" if alerted else "")}


def parse_time(value: Any) -> float | None:
    if not isinstance(value, str): return None
    try: return datetime.fromisoformat(value.replace("Z", "+00:00")).timestamp()
    except ValueError: return None


LIMIT_RE = re.compile(r"\b(429|quota|rate.?limit|usage.?limit|resource.?exhausted|too many requests)\b", re.I)
RUN_RE = re.compile(r"\b(agy|maccoder)\b", re.I)


def scan_logs(root: Path, account: str, now: float) -> dict[str, Any]:
    hits, runs, workers = [], 0, {}
    if not root or not root.exists():
        return {"name": account, "status": "로그 폴더 없음", "quota_percent": None, "limit_at": None, "runs": 0, "workers": []}
    try: paths = list(root.rglob("*.log"))
    except OSError: paths = []
    for path in paths:
        if not re.search(r"agy|maccoder", path.name, re.I): continue
        try:
            log_text = path.read_text(encoding="utf-8", errors="replace")
            if path.stat().st_mtime >= now - 7 * 86400: runs += len(RUN_RE.findall(log_text)) or 1
            for line in log_text.splitlines():
                if LIMIT_RE.search(line):
                    match = re.search(r"\d{4}-\d\d-\d\d[T ][0-2]\d:[0-5]\d:[0-5]\d(?:\.\d+)?(?:Z|[+-]\d\d:?\d\d)?", line)
                    stamp = parse_time(match.group(0).replace(" ", "T", 1)) if match else path.stat().st_mtime
                    hits.append((stamp or path.stat().st_mtime, str(path)))
            if path.stat().st_mtime >= now - 2 * 86400:
                key = path.stem
                workers[key] = {"name": key, "log": path.name, "modified": datetime.fromtimestamp(path.stat().st_mtime).astimezone().isoformat()}
        except OSError: continue
    recent = max(hits, default=(None, None), key=lambda x: x[0] or 0)
    return {"name": account, "status": "제공 안 됨", "quota_percent": None, "limit_at": datetime.fromtimestamp(recent[0]).astimezone().isoformat() if recent[0] else None,
            "alert": bool(recent[0]), "runs": runs, "workers": sorted(workers.values(), key=lambda x:x["modified"], reverse=True)[:8],
            "limit_log": Path(recent[1]).name if recent[1] else None}


def collect() -> dict[str, Any]:
    now = time.time()
    codex = [codex_card(label, root) for label, root in CODEX_DIRS]
    codex.append(codex_card("Mac · .codex", Path("~/.codex"), remote=True))
    claude = [claude_usage(root, now) | {"name": label} for label, root in CLAUDE_DIRS]
    agy = [scan_logs(root, account, now) if root else remote_gemini_card(account, _) for account, root, _ in AGY]
    return {"collected_at": datetime.now().astimezone().isoformat(timespec="seconds"), "interval_seconds": 30,
            "codex": codex, "claude": claude, "gemini": agy}


def main() -> None:
    while True:
        try:
            data = collect()
            tmp = HERE / "usage.json.tmp"
            tmp.write_text(json.dumps(data, ensure_ascii=False, indent=2), encoding="utf-8")
            tmp.replace(HERE / "usage.json")
        except Exception as exc:
            (HERE / "collector-error.txt").write_text(f"{datetime.now().isoformat()} {exc!r}\n", encoding="utf-8")
        time.sleep(30)


if __name__ == "__main__": main()
