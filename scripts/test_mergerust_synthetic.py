"""Rust 창 통합 관련 시험. 기존 시험의 문서 입력만 합성 자료로 바꾼다.

저장소 루트에서 앱 가상환경 Python으로 실행한다. 빌드한 klib와 sidebar는
KLIB_EXE/DOCFIT_KLIB 및 DOCFIT_SIDEBAR_TEST_EXE로 지정한다.
시험 파일·기대값·프로덕션 코드는 바꾸지 않는다.
"""
from __future__ import annotations

import argparse
import builtins
from contextlib import ExitStack
import io
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch
import zipfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from scripts.test_merge110_synthetic import MODULES as DB_MODULES, synthetic_preview


WINDOW_MODULES = (
    'test_native_tasks', 'test_conversion_stop', 'test_conversion_file_actions',
    'test_convert_pipe', 'test_conversion_window_service', 'test_settings_window_service',
    'test_settings_compare', 'test_settings_list_keys', 'test_idiom_file_save',
    'test_rename_window_service', 'test_file_rename_service', 'test_file_rename_recovery',
    'test_file_rename_simple_rules', 'test_file_rename_date_format',
    'test_file_rename_number_shift', 'test_file_rename_plus', 'test_file_rename_name_limits',
    'test_ipc_job_file_rename', 'test_jobs', 'test_sidebar_launch', 'test_file_conversion',
    'test_pdf_compress', 'test_cli', 'test_file_rename_core', 'test_file_rename_rust_preview',
    'test_hwp_idioms', 'test_hwpx_idioms_roundtrip', 'test_text_conversion_embedded_runtime',
    'test_window_handoff', 'test_distribution_build', 'test_bridge_methods', 'test_mergerust',
    'test_desktop_close_watchdog', 'test_engine_extraction', 'test_force_stop',
    'test_hwp_process_encoding', 'test_hwp_process_restart_policy', 'test_sm6_data_loss_risks',
)


def synthetic_template(path: Path) -> None:
    """구역 설정과 글자 문단이 있는 합성 HWPX. 사용자/배포 문서는 읽지 않는다."""
    with zipfile.ZipFile(path, 'w') as archive:
        archive.writestr('mimetype', 'application/hwp+zip')
        archive.writestr('Preview/PrvText.txt', '합성 문단')
        archive.writestr('Contents/content.hpf',
            '<opf:package xmlns:opf="http://www.idpf.org/2007/opf/">'
            '<opf:manifest><opf:item id="header" href="header.xml" media-type="application/xml"/>'
            '<opf:item id="section0" href="section0.xml" media-type="application/xml"/>'
            '</opf:manifest></opf:package>')
        archive.writestr('Contents/header.xml',
            '<hh:head xmlns:hh="http://www.hancom.co.kr/hwpml/2011/head">'
            '<hh:refList><hh:fontfaces><hh:fontface lang="HANGUL" fontCnt="1">'
            '<hh:font id="0" face="한컴돋움" type="TTF"/></hh:fontface></hh:fontfaces>'
            '<hh:borderFills itemCnt="1"><hh:borderFill id="1"/></hh:borderFills>'
            '<hh:charProperties itemCnt="1"><hh:charPr id="0" height="1000" textColor="#000000">'
            '<hh:fontRef hangul="0"/></hh:charPr></hh:charProperties>'
            '<hh:paraProperties itemCnt="1"><hh:paraPr id="0">'
            '<hh:align horizontal="LEFT"/></hh:paraPr></hh:paraProperties></hh:refList></hh:head>')
        archive.writestr('Contents/section0.xml',
            '<hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section" '
            'xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph">'
            '<hp:p id="0" paraPrIDRef="0" styleIDRef="0">'
            '<hp:run charPrIDRef="0"><hp:secPr><hp:pagePr width="59528" height="84188">'
            '<hp:margin left="8504" right="8504" top="5669" bottom="4252"/>'
            '</hp:pagePr></hp:secPr></hp:run></hp:p>'
            '<hp:p id="1" paraPrIDRef="0" styleIDRef="0">'
            '<hp:run charPrIDRef="0"><hp:t>합성 문단</hp:t></hp:run></hp:p></hs:sec>')


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--stage', choices=('1', '2', '3'), default='3')
    args = parser.parse_args()
    names = set(WINDOW_MODULES + DB_MODULES)
    if args.stage in ('2', '3'):
        names.update(('test_photoview_library_schema', 'test_docview_window_service',
                      'test_photoview_window_service'))
    if args.stage == '3':
        names.update(p.stem for p in (ROOT / 'tests').glob('test_*outlin*.py'))
        names.add('test_package_integrity')
    print('MODULES=' + ','.join(sorted(names)), flush=True)
    suite = unittest.defaultTestLoader.loadTestsFromNames(['tests.' + n for n in sorted(names)])
    preview = synthetic_preview()
    read_bytes = Path.read_bytes
    uses = []

    def synthetic_hwp(path: Path) -> bytes:
        if path.name == 'KIDX13_reference.hwp':
            uses.append(True)
            return preview
        return read_bytes(path)

    with tempfile.TemporaryDirectory(prefix='mergerust-input-') as folder, ExitStack() as stack:
        template = Path(folder) / 'synthetic.hwpx'
        synthetic_template(template)

        def redirect(original):
            def opened(file, mode='r', *positional, **keywords):
                if isinstance(file, (str, os.PathLike)) and Path(file).name == '기본 서식.hwpx' and 'r' in mode:
                    file = template
                return original(file, mode, *positional, **keywords)
            return opened

        stack.enter_context(patch.object(io, 'open', redirect(io.open)))
        stack.enter_context(patch.object(builtins, 'open', redirect(builtins.open)))
        stack.enter_context(patch.object(Path, 'read_bytes', synthetic_hwp))
        result = unittest.TextTestRunner(verbosity=1).run(suite)
    print('SYNTHETIC_HWP_FIXTURE_READS=' + str(len(uses)))
    return 0 if result.wasSuccessful() else 1


if __name__ == '__main__':
    raise SystemExit(main())
