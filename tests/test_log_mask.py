import logging
import unittest

from docfit_core.log_mask import mask_log_text


class LogMaskTests(unittest.TestCase):
    def test_masks_sensitive_patterns(self):
        source = r"C:\Users\sensitive.user\Docs\report.hwp user@example.com sk-abcdefghijklmnopqrstuvwxyz123456 '회의 내용과 개인 정보 문장입니다'"
        masked = mask_log_text(source)
        self.assertNotIn("sensitive.user", masked)
        self.assertNotIn("user@example.com", masked)
        self.assertNotIn("sk-abcdefghijklmnopqrstuvwxyz123456", masked)
        self.assertNotIn("회의 내용과 개인 정보", masked)

    def test_regular_string_is_unchanged(self):
        source = "문서 열기 실패 (code 17): 파일을 찾을 수 없습니다"
        self.assertEqual(mask_log_text(source), source)

    def test_log_record_is_masked(self):
        logger = logging.getLogger("test.log_mask")
        with self.assertLogs(logger, level="ERROR") as captured:
            logger.error("문서 오류 %s", r"C:\Users\private\Docs\a.hwp")
        self.assertNotIn("private", captured.output[0])


if __name__ == "__main__":
    unittest.main()
