# OSS Scanner build image for HWP AutoDocFit.
# The product is a Windows desktop app (Hancom Office COM, WebView2). On Linux we install the
# pure-Python parts and their dependencies so the code can be imported, analysed and unit-tested.
# Windows-only modules are replaced by stubs at test time (scripts/linux_stub_test_runner.py).
FROM python:3.14-bookworm
ENV DEBIAN_FRONTEND=noninteractive PYTHONDONTWRITEBYTECODE=1 PIP_DISABLE_PIP_VERSION_CHECK=1
RUN apt-get update && apt-get install -y --no-install-recommends \
        tk xvfb xauth libgl1 libglib2.0-0 ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY . /src
WORKDIR /src
# pywin32 has no Linux build; everything else installs normally (pinned by constraints.txt).
RUN grep -viE '^(pywin32)\b' requirements.txt > /tmp/requirements-linux.txt \
    && python -m pip install --no-cache-dir -r /tmp/requirements-linux.txt -c constraints.txt \
    && python -m pip install --no-cache-dir pytest
# Byte-compile to catch syntax errors during the build.
RUN python -m compileall -q docfit_core docfit_app docfit_engine desktop_web_ui.py
# Run the test suite offline with:  python scripts/linux_stub_test_runner.py discover -s tests
# (tests that need Tk/Windows/Hancom are expected to fail or skip on Linux).
# Run as an unprivileged user by default (the scanner switches to root for its own layer).
RUN useradd --create-home --uid 10001 scanner && chown -R scanner /src
USER scanner
