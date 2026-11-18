"""Sensitive-data masking for application log messages."""
from __future__ import annotations

import logging
import re


_EMAIL = re.compile(r"\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b", re.IGNORECASE)
_TOKEN = re.compile(r"(?i)\b(?:sk|api|key|token|secret|bearer)[-_ ]?[A-Za-z0-9_-]{20,}\b|\b[A-Za-z0-9_-]{40,}\b")
_WINDOWS_USER_PATH = re.compile(r"(?i)([A-Z]:\\Users\\)([^\\/\s]+)")
_UNIX_USER_PATH = re.compile(r"(/home/)([^/\s]+)")
_QUOTED_TEXT = re.compile(r"(?<![\w])(?:'([^'\r\n]{12,})'|\"([^\"\r\n]{12,})\")")


def mask_log_text(value: object) -> str:
    """Mask likely personal identifiers and content in a log string."""
    text = str(value)
    text = _EMAIL.sub("[이메일]", text)
    text = _TOKEN.sub("[토큰]", text)
    text = _WINDOWS_USER_PATH.sub(lambda m: m.group(1) + "[사용자]", text)
    text = _UNIX_USER_PATH.sub(lambda m: m.group(1) + "[사용자]", text)
    text = _QUOTED_TEXT.sub("'[본문]'", text)
    return text


def _sanitize_record(record: logging.LogRecord) -> None:
    record.msg = mask_log_text(record.msg)
    if record.args:
        if isinstance(record.args, dict):
            record.args = {key: mask_log_text(value) for key, value in record.args.items()}
        else:
            record.args = tuple(mask_log_text(value) for value in record.args)
    if record.exc_info:
        exc_type, exc_value, _ = record.exc_info
        if exc_value is not None:
            record.exc_text = f"{exc_type.__name__}: {mask_log_text(exc_value)}"
            record.exc_info = None


class _MaskingFilter(logging.Filter):
    def filter(self, record: logging.LogRecord) -> bool:
        _sanitize_record(record)
        return True


_previous_factory = logging.getLogRecordFactory()


def _masking_record_factory(*args, **kwargs):
    record = _previous_factory(*args, **kwargs)
    _sanitize_record(record)
    return record


logging.setLogRecordFactory(_masking_record_factory)
