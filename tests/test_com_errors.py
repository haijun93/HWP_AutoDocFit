import unittest

from docfit_core.com_errors import hresult_of, user_message


class ComErrorsTest(unittest.TestCase):
    def test_server_fault_has_actionable_korean_message_and_context(self):
        error = RuntimeError(-2147417851, "서버에서 예외 오류가 발생했습니다.")
        message = user_message(error, stage="자간 조정", call="HAction.Run()")
        self.assertIn("0x80010105", message)
        self.assertIn("한/글을 모두 닫고 다시 시도", message)
        self.assertIn("표·그림·도형", message)
        self.assertIn("자간 조정", message)
        self.assertIn("HAction.Run()", message)

    def test_unsigned_hresult_is_normalized(self):
        self.assertEqual(-2147417851, hresult_of(RuntimeError(0x80010105)))

    def test_unknown_error_preserves_original_message(self):
        error = RuntimeError("ordinary failure")
        self.assertEqual("ordinary failure", user_message(error, stage="저장"))


if __name__ == "__main__":
    unittest.main()
