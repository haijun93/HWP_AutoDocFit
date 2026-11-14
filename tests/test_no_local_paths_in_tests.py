"""시험 코드에 이 개발 PC 전용 경로를 박으면 CI에서 실패한다(2026-11-12, 같은 실수 두 번). 미리 막는다."""
import pathlib
import unittest

FORBIDDEN = tuple(f"{d}:{sep}docfit-tmp" for d in "EFG" for sep in ("/", "\\", "\\\\"))  # 작업용 임시 드라이브. 기존 시험의 C:/HWP_AUTODOCFIT 참조는 없을 때 건너뛰는 선택 자료라 대상에서 뺀다.


class NoLocalPathsInTests(unittest.TestCase):
    def test_tests_do_not_hardcode_this_pc_paths(self):
        here = pathlib.Path(__file__).resolve()
        bad = []
        for path in here.parent.glob("test_*.py"):
            if path == here:
                continue
            text = path.read_text(encoding="utf-8", errors="replace")
            for token in FORBIDDEN:
                if token in text:
                    bad.append(f"{path.name}: {token}")
        self.assertEqual([], bad)

    def test_rust_sources_do_not_hardcode_this_pc_paths(self):
        """Rust 시험도 같은 실수를 했다(2026-11-13 kvf, v2.2.64 CI 실패). 크레이트 src·tests 전부 검사한다."""
        root = pathlib.Path(__file__).resolve().parent.parent
        bad = []
        for path in root.rglob("*.rs"):
            parts = set(path.parts)
            if "target" in parts or "node_modules" in parts:
                continue
            text = path.read_text(encoding="utf-8", errors="replace")
            for token in FORBIDDEN:
                if token in text:
                    bad.append(f"{path.relative_to(root)}: {token}")
        self.assertEqual([], bad)


if __name__ == "__main__":
    unittest.main()
