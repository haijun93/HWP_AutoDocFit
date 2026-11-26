"""Rust 창 통합의 실행 선택·작업 버전·배포 누락 회귀 검사."""
import ast
from pathlib import Path
import unittest

from docfit_app import sidebar_launch


ROOT = Path(__file__).resolve().parents[1]


class MergeRustTest(unittest.TestCase):
    def test_worker_and_app_share_the_release_version_before_early_exit(self):
        source = (ROOT / 'hwp-auto-docfit.py').read_bytes()
        tree = ast.parse(source.decode('utf-8'))
        versions = [node for node in tree.body if isinstance(node, ast.Assign)
                    and any(isinstance(t, ast.Name) and t.id == 'APP_VERSION' for t in node.targets)]
        self.assertEqual(len(versions), 1)
        version = ast.literal_eval(versions[0].value)
        self.assertRegex(version, r'^\d+\.\d+\.\d+$')
        self.assertTrue((ROOT / 'releases' / f'v{version}.md').is_file())
        self.assertLess(versions[0].lineno,
                        next(i for i, line in enumerate(source.decode('utf-8').splitlines(), 1)
                             if '--native-task' in line))
        self.assertEqual(source.count(b'\n'), source.count(b'\r\n'))

    def test_empty_settings_keep_the_normal_app(self):
        import tempfile
        with tempfile.TemporaryDirectory() as folder:
            self.assertFalse(sidebar_launch.wanted([], Path(folder), {}))
            self.assertTrue(sidebar_launch.wanted(['--sidebar'], Path(folder), {}))
            self.assertFalse(sidebar_launch.wanted(['--full-app'], Path(folder), {}))

    def test_sidebar_and_native_task_tests_are_in_distribution_pipeline(self):
        workflow = (ROOT / '.github/workflows/build-windows-exe.yml').read_text(encoding='utf-8')
        self.assertIn('cargo test --release --locked --manifest-path native_tasks/Cargo.toml', workflow)
        self.assertIn('resources/sidebar.exe.sha256', workflow)
        runtime_check = workflow.split('$files = @(', 1)[1].split('ForEach-Object', 1)[0]
        bundle_check = workflow.split("$folder = 'dist/HWP_AutoDocFit/_internal/resources'", 1)[1]
        for name in ('sidebar.exe', 'dock.exe', 'rename_win.exe', 'convert_win.exe',
                     'settings_win.exe', 'docview_win.exe', 'photoview_win.exe', 'outliner_win.exe'):
            self.assertIn("'" + name + "'", runtime_check)
            self.assertIn("'" + name + "'", bundle_check)
            self.assertIn(name, (ROOT / 'HWP_AutoDocFit.spec').read_text(encoding='utf-8'))

    def test_installer_keeps_user_confirmed_index_service_restart_notice(self):
        installer = (ROOT / 'installer/setup.iss').read_text(encoding='utf-8')
        self.assertIn('색인 서비스(HwpAutoDocFit.Index)를 지금 다시 시작하시겠습니까?', installer)
        self.assertIn('사용자가 서비스 재시작을 보류함', installer)
        self.assertIn('if not WizardSilent then', installer)


if __name__ == '__main__':
    unittest.main()
