"""시험 코드에 이 개발 PC 전용 경로를 박으면 CI에서 실패한다(2026-11-12, 같은 실수 두 번). 미리 막는다."""
import pathlib
import unittest

FORBIDDEN = ("E:/docfit-tmp", "E:\docfit-tmp", "E:\\docfit-tmp")  # 작업용 임시 드라이브. 기존 시험의 C:/HWP_AUTODOCFIT 참조는 없을 때 건너뛰는 선택 자료라 대상에서 뺀다.


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


if __name__ == "__main__":
    unittest.main()
