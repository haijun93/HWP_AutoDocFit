import json
import subprocess
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch

from docfit_app.kvf_client import KvfClient


class KvfClientTests(unittest.TestCase):
    def client(self, response=None):
        runner = Mock(return_value=SimpleNamespace(returncode=0, stdout=json.dumps(
            response if response is not None else dict(ok=True, running=False))))
        return KvfClient(bundled=lambda: Path('bundle/kvf.exe'), runner=runner), runner

    def test_missing_bundle_stays_off(self):
        runner = Mock()
        client = KvfClient(bundled=lambda: None, runner=runner)
        for action in ('status', 'upgrade_if_stale'):
            self.assertEqual(client.command(action)['running'], False)
        self.assertFalse(client.command('enable')['ok'])
        runner.assert_not_called()

    def test_status_does_not_enable(self):
        client, runner = self.client()
        self.assertTrue(client.command('status')['ok'])
        self.assertEqual(runner.call_args.args[0], ['bundle\\kvf.exe', '--status'])

    def test_enable_default_autostart_then_refresh(self):
        client, runner = self.client()
        client.command('enable')
        self.assertEqual(runner.call_args_list[0].args[0][1:], ['--enable', '--autostart'])
        self.assertEqual(runner.call_args_list[1].args[0][1:], ['--status'])

    def test_enable_without_autostart(self):
        client, runner = self.client()
        client.command('enable', {'autostart': False})
        self.assertEqual(runner.call_args_list[0].args[0][1:], ['--enable'])

    def test_disable_then_refresh(self):
        client, runner = self.client()
        client.command('disable')
        self.assertEqual([c.args[0][1] for c in runner.call_args_list], ['--disable', '--status'])

    def test_upgrade_never_enables(self):
        client, runner = self.client()
        client.command('upgrade_if_stale')
        self.assertEqual(runner.call_args.args[0][1:], ['--upgrade-if-stale'])

    def test_failed_mutation_is_not_retried(self):
        client, runner = self.client(dict(ok=False, reason='integrity'))
        self.assertEqual(client.command('enable')['message'], 'integrity')
        self.assertEqual(runner.call_count, 1)

    def test_invalid_responses(self):
        for stdout in ('', '[]', '{', '{"running":false}'):
            client, runner = self.client()
            runner.return_value.stdout = stdout
            self.assertFalse(client.command('status')['ok'])

    def test_process_failures(self):
        for failure in (OSError('missing'), subprocess.TimeoutExpired('kvf', 60)):
            client, runner = self.client()
            runner.side_effect = failure
            self.assertFalse(client.command('enable')['ok'])
            self.assertEqual(runner.call_count, 1)

    def test_exit_failure(self):
        client, runner = self.client()
        runner.return_value.returncode = 1
        self.assertFalse(client.command('status')['ok'])

    def test_unknown_command(self):
        client, runner = self.client()
        self.assertFalse(client.command('launch')['ok'])
        runner.assert_not_called()

    def test_ci_and_package_contract(self):
        root = Path(__file__).resolve().parents[1]
        ci = (root / '.github/workflows/build-windows-exe.yml').read_text('utf-8')
        self.assertIn('cargo test --release --locked --manifest-path kvf/Cargo.toml', ci)
        self.assertIn('Copy-Item kvf/target/release/kvf.exe resources/kvf.exe', ci)
        self.assertIn('resources/kvf.exe.sha256', ci)
        self.assertIn('dist/HWP_AutoDocFit/_internal/resources', ci)
        self.assertIn('kvf bundle hash mismatch', ci)
        spec = (root / 'HWP_AutoDocFit.spec').read_text('utf-8')
        self.assertIn("(str(root / 'resources'), 'resources')", spec)
        self.assertIn('recursesubdirs', (root / 'installer/setup.iss').read_text('utf-8'))

    def test_browser_bridge_exposes_independent_command(self):
        from desktop_web_ui import DesktopWebBridge, _BrowserApi
        with patch('docfit_app.kvf_client.KvfClient') as factory:
            factory.return_value.command.return_value = dict(ok=True, running=False)
            bridge = DesktopWebBridge.__new__(DesktopWebBridge)
            self.assertTrue(bridge.kvf('disable')['ok'])
            factory.return_value.command.assert_called_once_with('disable', None)
            self.assertTrue(hasattr(_BrowserApi(bridge), 'kvf'))


if __name__ == '__main__':
    unittest.main()
