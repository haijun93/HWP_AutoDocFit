from __future__ import annotations
import json, os, time
from pathlib import Path

class SessionEconomizer:
    def __init__(self, root: Path | None = None, repeat_threshold: int = 3):
        self.root = root or Path(os.environ.get('LOCALAPPDATA', Path.home())) / 'HWP_AutoDocFit' / 'token_economizer'
        self.path = self.root / 'state.json'; self.repeat_threshold = repeat_threshold
        try: self.state = json.loads(self.path.read_text(encoding='utf-8'))
        except (OSError, ValueError): self.state = {}
    def _save(self):
        self.root.mkdir(parents=True, exist_ok=True); tmp = self.path.with_suffix('.tmp')
        tmp.write_text(json.dumps(self.state, ensure_ascii=False, indent=2), encoding='utf-8'); os.replace(tmp, self.path)
    def record_fact(self, key, value, source='session'):
        facts = self.state.setdefault('facts', {}); item = facts.setdefault(key, {'value': value, 'count': 0, 'sources': []})
        if item['value'] != value: item.update(value=value, count=0, sources=[])
        item['count'] += 1
        if source not in item['sources']: item['sources'].append(source)
        item['active'] = item['count'] >= self.repeat_threshold; self._save(); return item['active']
    def active_facts(self): return {k:v['value'] for k,v in self.state.get('facts', {}).items() if v.get('active')}
    def record_tool(self, name, used):
        item = self.state.setdefault('tools', {}).setdefault(name, {'used':0, 'unused':0}); item['used' if used else 'unused'] += 1; item['last'] = int(time.time()); self._save()
    def loop_guard(self, signature):
        item = self.state.setdefault('failures', {}).setdefault(signature, {'count':0, 'redirected':False}); item['count'] += 1
        redirect = item['count'] >= self.repeat_threshold and not item['redirected']; item['redirected'] |= redirect; self._save(); return redirect
    def reset_failure(self, signature): self.state.get('failures', {}).pop(signature, None); self._save()
