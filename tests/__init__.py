"""Keep user profile data isolated while the unittest suite is running."""
from __future__ import annotations

import os
import tempfile

_sandbox = tempfile.TemporaryDirectory(prefix="docfit-tests-")
_root = _sandbox.name
_real_localappdata = os.environ.get("LOCALAPPDATA") or os.path.join(
    os.path.expanduser("~"), "AppData", "Local"
)
os.environ["DOCFIT_REAL_LOCALAPPDATA"] = _real_localappdata
for _name in ("LOCALAPPDATA", "APPDATA", "USERPROFILE", "HOME"):
    os.environ[_name] = os.path.join(_root, _name.lower())
os.environ["DOCFIT_FORBID_REAL_DATA"] = "1"


def _cleanup_sandbox():
    _sandbox.cleanup()


import atexit
atexit.register(_cleanup_sandbox)


