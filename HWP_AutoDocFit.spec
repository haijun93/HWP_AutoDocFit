# Single packaging definition shared by build.bat and Windows CI.
from PyInstaller.utils.hooks import collect_all, collect_submodules
from pathlib import Path
root = Path(SPECPATH)
datas = [(str(root / 'MapoHwpAutoDocFitSecurity.dll'), '.'),
         (str(root / 'desktop_web'), 'desktop_web'),
         (str(root / 'resources'), 'resources')]
pack = root / 'resources' / 'HWP_AutoDocFit_kordoc_runtime.zip'
if not pack.is_file() or not pack.with_name(pack.name + '.sha256').is_file():
    raise RuntimeError('Build the embedded offline runtime before PyInstaller packaging')
binaries, hiddenimports = [], []
pdf_edit_exe = root / 'resources' / 'pdf-edit.exe'
pdf_edit_sha256 = root / 'resources' / 'pdf-edit.exe.sha256'
if not pdf_edit_exe.is_file() or not pdf_edit_sha256.is_file():
    raise RuntimeError('Build the pdf-edit Rust engine before PyInstaller packaging')
binaries.append((str(pdf_edit_exe), 'resources'))
outliner_exe = root / 'resources' / 'outliner.exe'
outliner_sha256 = root / 'resources' / 'outliner.exe.sha256'
if not outliner_exe.is_file() or not outliner_sha256.is_file():
    raise RuntimeError('Build the outliner Rust engine before PyInstaller packaging')
binaries.append((str(outliner_exe), 'resources'))
qms_bench_exe = root / 'resources' / 'qms_bench.exe'
qms_bench_sha256 = root / 'resources' / 'qms_bench.exe.sha256'
if not qms_bench_exe.is_file() or not qms_bench_sha256.is_file():
    raise RuntimeError('Build the qms_bench Rust engine before PyInstaller packaging')
binaries.append((str(qms_bench_exe), 'resources'))
binaries.append((str(qms_bench_sha256), 'resources'))
dock_exe = root / 'resources' / 'dock.exe'
dock_sha256 = root / 'resources' / 'dock.exe.sha256'
if not dock_exe.is_file() or not dock_sha256.is_file():
    raise RuntimeError('Build the dock Rust app before PyInstaller packaging')
binaries.append((str(dock_exe), 'resources'))
binaries.append((str(dock_sha256), 'resources'))
# 사이드바 실행 파일(dock 크레이트). 선택 실행 방식이라 있으면 넣는다(없으면 앱이 기존 화면을 연다).
sidebar_exe = root / 'resources' / 'sidebar.exe'
if sidebar_exe.is_file():
    binaries.append((str(sidebar_exe), 'resources'))
    sidebar_sha = root / 'resources' / 'sidebar.exe.sha256'
    if sidebar_sha.is_file():
        datas.append((str(sidebar_sha), 'resources'))
klib_exe = root / 'resources' / 'klib.exe'
if klib_exe.is_file():
    binaries.append((str(klib_exe), 'resources'))
mdm_ctx_exe = root / 'resources' / 'mdm-ctx.exe'
if mdm_ctx_exe.is_file():
    binaries.append((str(mdm_ctx_exe), 'resources'))
# 아웃라이너 Win32 창(설정 outliner_ui='native' 일 때 앱이 연다). mdm-ctx 처럼 있으면 넣는다(없으면 앱이 안내만 한다).
outliner_win_exe = root / 'resources' / 'outliner_win.exe'
if outliner_win_exe.is_file():
    binaries.append((str(outliner_win_exe), 'resources'))
# 설정 Win32 창(설정 settings_ui='native' 일 때 앱의 [설정] 진입이 연다). 있으면 넣는다(없으면 앱이 안내만 하고 기존 화면을 연다).
settings_win_exe = root / 'resources' / 'settings_win.exe'
if settings_win_exe.is_file():
    binaries.append((str(settings_win_exe), 'resources'))
# 문서뷰 Win32 창(설정 docview_ui='native' 일 때 앱의 문서뷰 탭이 연다). 있으면 넣는다(없으면 앱이 안내만 하고 기존 화면을 연다).
docview_win_exe = root / 'resources' / 'docview_win.exe'
if docview_win_exe.is_file():
    binaries.append((str(docview_win_exe), 'resources'))
# 사진뷰 Win32 창(설정 photoview_ui='native' 일 때 앱의 사진뷰 탭이 연다). 있으면 넣는다(없으면 앱이 안내만 하고 기존 화면을 연다).
photoview_win_exe = root / 'resources' / 'photoview_win.exe'
if photoview_win_exe.is_file():
    binaries.append((str(photoview_win_exe), 'resources'))
# 몽땅파일변환 Win32 창(설정 conversion_ui='native' 일 때 앱의 변환 탭이 연다). 있으면 넣는다(없으면 앱이 안내만 하고 기존 화면을 연다).
convert_win_exe = root / 'resources' / 'convert_win.exe'
if convert_win_exe.is_file():
    binaries.append((str(convert_win_exe), 'resources'))
# 파일명 변경 미리보기 Win32 창(rename_ui='native').
rename_win_exe = root / 'resources' / 'rename_win.exe'
if rename_win_exe.is_file():
    binaries.append((str(rename_win_exe), 'resources'))
hierarchy_exe = root / 'resources' / 'hierarchy.exe'
if hierarchy_exe.is_file():
    binaries.append((str(hierarchy_exe), 'resources'))
hierarchy_dll = root / 'resources' / 'hierarchy.dll'
if hierarchy_dll.is_file():
    binaries.append((str(hierarchy_dll), 'resources'))
kclip_ffi = root / 'kagent' / 'target' / 'release' / 'kclip_ffi.dll'
if kclip_ffi.is_file():
    binaries.append((str(kclip_ffi), '.'))
for package in ('docfit_core', 'docfit_engine', 'docfit_app', 'docfit_ui'):
    hiddenimports += collect_submodules(package)
for package in ('tkinterdnd2', 'PIL', 'webview', 'pythonnet', 'clr_loader', 'pypdfium2', 'pypdfium2_raw', 'pypdf', 'cryptography'):
    data, binary, imports = collect_all(package)
    datas += data
    binaries += binary
    hiddenimports += imports
a = Analysis([str(root / 'hwp-auto-docfit.py')], pathex=[str(root)],
             binaries=binaries, datas=datas, hiddenimports=hiddenimports)
pyz = PYZ(a.pure)
installer_exe = EXE(pyz, a.scripts, [], name='HWP_AutoDocFit',
                     console=False, icon=str(root / 'resources' / 'app_icon.ico'),
                     exclude_binaries=True)
installer_bundle = COLLECT(installer_exe, a.binaries, a.datas, name='HWP_AutoDocFit')
