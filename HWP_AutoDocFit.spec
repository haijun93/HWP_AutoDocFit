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
dock_exe = root / 'resources' / 'dock.exe'
dock_sha256 = root / 'resources' / 'dock.exe.sha256'
if not dock_exe.is_file() or not dock_sha256.is_file():
    raise RuntimeError('Build the dock Rust app before PyInstaller packaging')
binaries.append((str(dock_exe), 'resources'))
binaries.append((str(dock_sha256), 'resources'))
klib_exe = root / 'resources' / 'klib.exe'
if klib_exe.is_file():
    binaries.append((str(klib_exe), 'resources'))
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
