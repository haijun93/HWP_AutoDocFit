"""몽땅파일변환 우클릭 메뉴(CTXMENU2): mdm-ctx 연동 명령·옛 키 마이그레이션·임시 레지스트리 키 시험.

실제 `HKCU\\Software\\Classes\\...\\HwpAutoDocFit`·`MongttangFileConvert` 키는 쓰거나 지우지 않는다.
실제 레지스트리 시험은 난수가 붙은 임시 메뉴 이름만 쓰고 끝나면 반드시 지운다.
"""
import os
import re
import sys
import unittest
import uuid
from pathlib import Path
from unittest.mock import patch

from docfit_app import shell_menu as menu
from docfit_core.file_conversion import CONVERSION_INPUTS
from tests.test_shell_menu import Registry

ROOT = Path(__file__).resolve().parents[1]


class CtxCommandTests(unittest.TestCase):
    def setUp(self):
        self.reg = Registry()
        self.app = r'C:\앱 폴더\HWP_AutoDocFit.exe'
        self.ctx = r'C:\앱 폴더\_internal\resources\mdm-ctx.exe'

    def commands(self):
        return {p: v[''] for p, v in self.reg.data.items() if p.endswith('\\command')}

    def test_rust_extension_set_matches_python_conversion_inputs(self):
        source = (ROOT / 'mdm-ctx' / 'src' / 'logic.rs').read_text(encoding='utf-8')
        block = re.search(r'INPUT_EXTENSIONS: \[&str; (\d+)\] = \[(.*?)\];', source, re.S)
        exts = re.findall(r'"(\.[a-z0-9]+)"', block.group(2))
        self.assertEqual(int(block.group(1)), len(exts))
        self.assertEqual(set(exts), set(CONVERSION_INPUTS))

    def test_convert_commands_go_through_ctx_without_collect(self):
        menu.register(self.app, registry=self.reg, ctx_exe=self.ctx)
        convert = [c for c in self.commands().values() if ' convert ' in c]
        self.assertEqual(len(convert), 16)
        for command in convert:
            self.assertTrue(command.startswith(f'"{self.ctx}" --app "{self.app}" convert "%1" --to '), command)
            self.assertNotIn('--collect', command)
            self.assertTrue(command.endswith(' --notify'))
        # '앱으로 열기'는 앱을 직접 부른다(앱의 단일 인스턴스 전달 사용).
        self.assertIn(f'"{self.app}" "%1"', self.commands().values())
        base = menu.ROOT + r'\.hwp\shell\MongttangFileConvert'
        self.assertEqual(self.reg.data[base + r'\shell\04_open']['MUIVerb'], '앱으로 열기')
        self.assertEqual(self.reg.data[base]['DocFitCtx'], self.ctx)

    def test_without_ctx_keeps_direct_collect_commands(self):
        menu.register(self.app, registry=self.reg)
        commands = self.commands().values()
        self.assertIn(f'"{self.app}" convert "%1" --to pdfm --collect pdfm --notify', commands)
        self.assertFalse(any('mdm-ctx' in c for c in commands))

    def test_submenus_per_extension(self):
        menu.register(self.app, registry=self.reg, ctx_exe=self.ctx)

        def labels(ext):
            base = menu.ROOT + f'\\{ext}\\shell\\MongttangFileConvert\\shell\\'
            return [v['MUIVerb'] for p, v in sorted(self.reg.data.items()) if p.startswith(base) and '\\' not in p[len(base):]]
        self.assertEqual(labels('.hwp'), ['PDF로 변환(여러 개)', 'HWPX로 변환(여러 개)', 'PDF 하나로 합치기', '그림(PNG)으로 변환', '앱으로 열기'])
        self.assertEqual(labels('.hwpx'), ['PDF로 변환', 'PDF 하나로 합치기', '그림(PNG)으로 변환', '앱으로 열기'])
        self.assertEqual(labels('.pdf'), ['PDF 하나로 합치기', 'PDF 압축', '그림(PNG)으로 변환'])
        for ext in ('.png', '.jpg', '.jpeg'):
            self.assertEqual(labels(ext), ['PDF로 변환', 'PDF 하나로 합치기'])

    def test_register_is_idempotent(self):
        menu.register(self.app, registry=self.reg, ctx_exe=self.ctx)
        before = {k: dict(v) for k, v in self.reg.data.items()}
        menu.register(self.app, registry=self.reg, ctx_exe=self.ctx)
        self.assertEqual(self.reg.data, before)


class MigrationTests(unittest.TestCase):
    def setUp(self):
        self.reg = Registry()
        self.app = r'C:\앱\HWP_AutoDocFit.exe'
        self.ctx = r'C:\앱\_internal\resources\mdm-ctx.exe'
        self.legacy = menu.ROOT + r'\.pdf\shell\HwpAutoDocFit'

    def legacy_install(self):
        for ext in menu.EXTENSIONS:
            base = menu.ROOT + '\\' + ext + r'\shell\HwpAutoDocFit'
            self.reg.CreateKey(1, base + r'\shell\00_pdf\command')
            self.reg.data[base] = {'MUIVerb': '한글편집 후처리', 'DocFitCommand': '"old.exe"'}

    def test_startup_replaces_legacy_key_when_setting_is_on(self):
        self.legacy_install()
        self.assertTrue(menu.status(registry=self.reg)['registered'])  # 옛 키만 있어도 설정은 켜짐으로 보인다
        with patch.object(menu, 'launch_command', return_value=[self.app]), patch.object(menu, 'ctx_exe_for', return_value=Path(self.ctx)):
            menu.refresh_registered(registry=self.reg)
        self.assertFalse(any('HwpAutoDocFit' in p for p in self.reg.data))
        state = menu.status(registry=self.reg)
        self.assertEqual((state['registered'], state['legacy'], state['command'], state['ctx']), (True, False, f'"{self.app}"', self.ctx))

    def test_startup_writes_nothing_when_off(self):
        with patch.object(menu, 'launch_command', return_value=[self.app]), patch.object(menu, 'ctx_exe_for', return_value=None):
            menu.refresh_registered(registry=self.reg)
        self.assertEqual(self.reg.data, {})

    def test_unregister_removes_both_keys_and_keeps_others(self):
        self.legacy_install()
        menu.register(self.app, registry=self.reg, ctx_exe=self.ctx)
        # register 가 옛 키를 지웠으므로 다시 옛 키를 만들어 '둘 다 있는' 상태를 시험한다.
        self.legacy_install()
        self.reg.data[menu.ROOT + r'\.pdf\shell\Other'] = {}
        menu.unregister(registry=self.reg)
        self.assertFalse(any('HwpAutoDocFit' in p or 'MongttangFileConvert' in p for p in self.reg.data))
        self.assertIn(menu.ROOT + r'\.pdf\shell\Other', self.reg.data)
        self.assertFalse(menu.status(registry=self.reg)['registered'])

    def test_refresh_rewrites_when_ctx_appears_or_path_changes(self):
        menu.register(self.app, registry=self.reg, ctx_exe=None)
        with patch.object(menu, 'launch_command', return_value=[self.app]), patch.object(menu, 'ctx_exe_for', return_value=Path(self.ctx)):
            menu.refresh_registered(registry=self.reg)
            self.assertEqual(menu.status(registry=self.reg)['ctx'], self.ctx)
            before = {k: dict(v) for k, v in self.reg.data.items()}
            menu.refresh_registered(registry=self.reg)
            self.assertEqual(self.reg.data, before)


class CtxExeLookupTests(unittest.TestCase):
    def test_not_used_for_python_script_launch(self):
        self.assertIsNone(menu.ctx_exe_for([r'C:\py\python.exe', r'C:\src\hwp-auto-docfit.py']))

    def test_found_in_bundle_resources(self):
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            (Path(tmp) / 'resources').mkdir()
            (Path(tmp) / 'resources' / 'mdm-ctx.exe').write_bytes(b'x')
            with patch.object(sys, '_MEIPASS', tmp, create=True):
                self.assertEqual(menu.ctx_exe_for([r'C:\app.exe']), Path(tmp) / 'resources' / 'mdm-ctx.exe')


@unittest.skipUnless(os.name == 'nt', 'Windows 레지스트리 시험')
class RealRegistryTempKeyTests(unittest.TestCase):
    """난수 이름의 임시 메뉴 키만 만들고 지운다. 실제 HwpAutoDocFit·MongttangFileConvert 키는 읽기만 한다."""

    def setUp(self):
        import winreg
        self.winreg = winreg
        suffix = uuid.uuid4().hex[:8]
        self.new = f'HwpAutoDocFitTest_{suffix}'
        self.old = f'HwpAutoDocFitTestOld_{suffix}'
        self.addCleanup(self.cleanup)

    def cleanup(self):
        menu.unregister(menu=self.new, legacy_menu=self.old)

    def exists(self, name, ext='.pdf'):
        try:
            self.winreg.CloseKey(self.winreg.OpenKey(self.winreg.HKEY_CURRENT_USER, menu._menu_key(ext, name)))
            return True
        except FileNotFoundError:
            return False

    def test_register_migrate_unregister_on_temp_keys(self):
        real_before = [self.exists(n, e) for n in (menu.MENU, menu.LEGACY_MENU) for e in menu.EXTENSIONS]
        app, ctx = r'C:\시험 폴더\app.exe', r'C:\시험 폴더\mdm-ctx.exe'
        # 옛 키(임시 이름) 만들기
        for ext in menu.EXTENSIONS:
            with self.winreg.CreateKey(self.winreg.HKEY_CURRENT_USER, menu._menu_key(ext, self.old)) as key:
                self.winreg.SetValueEx(key, 'DocFitCommand', 0, self.winreg.REG_SZ, '"old.exe"')
        self.assertTrue(menu.status(menu=self.new, legacy_menu=self.old)['legacy'])

        menu.register(app, ctx_exe=ctx, menu=self.new, legacy_menu=self.old)
        self.assertFalse(self.exists(self.old))  # 옛 키 정리
        for ext in menu.EXTENSIONS:
            self.assertTrue(self.exists(self.new, ext), ext)
        state = menu.status(menu=self.new, legacy_menu=self.old)
        self.assertEqual((state['registered'], state['legacy'], state['command'], state['ctx']), (True, False, f'"{app}"', ctx))
        with self.winreg.OpenKey(self.winreg.HKEY_CURRENT_USER, menu._menu_key('.hwp', self.new) + r'\shell\00_pdf\command') as key:
            value = self.winreg.QueryValueEx(key, '')[0]
        self.assertEqual(value, f'"{ctx}" --app "{app}" convert "%1" --to pdf --notify')
        with self.winreg.OpenKey(self.winreg.HKEY_CURRENT_USER, menu._menu_key('.hwp', self.new)) as key:
            self.assertEqual(self.winreg.QueryValueEx(key, 'MUIVerb')[0], '몽땅파일변환')

        menu.register(app, ctx_exe=ctx, menu=self.new, legacy_menu=self.old)  # 멱등
        menu.unregister(menu=self.new, legacy_menu=self.old)
        for ext in menu.EXTENSIONS:
            self.assertFalse(self.exists(self.new, ext))
            self.assertFalse(self.exists(self.old, ext))
        self.assertFalse(menu.status(menu=self.new, legacy_menu=self.old)['registered'])
        # 실제 키는 시험 전후 그대로다(읽기만 했다).
        self.assertEqual(real_before, [self.exists(n, e) for n in (menu.MENU, menu.LEGACY_MENU) for e in menu.EXTENSIONS])


if __name__ == '__main__':
    unittest.main()
