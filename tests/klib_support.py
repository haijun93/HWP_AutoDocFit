"""klib 실행 파일을 쓰는 시험의 준비 도구."""
import subprocess
import tempfile

from docfit_app.rust_scan import find_klib_executable


def warm_up_klib() -> None:
    """시간 기한을 재기 전에 klib 을 빈 폴더에 한 번 실행한다.

    새로 만든 실행 파일은 첫 실행만 몇 초 걸려(실행 파일 검사) 검색 시험의 기한을 넘길 수 있다.
    실행 실패는 여기서 판정하지 않는다(각 시험이 그대로 판정한다).
    """
    executable = find_klib_executable()
    if executable is None:
        return
    with tempfile.TemporaryDirectory() as empty:
        try:
            subprocess.run([str(executable), "scan", "--roots", empty, "--ext", "pdf", "--json-lines"],
                           capture_output=True, timeout=60, check=False)
        except (OSError, subprocess.TimeoutExpired):
            pass
