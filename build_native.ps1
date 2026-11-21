param([string]$Flutter = "flutter", [string]$Python = "python", [string]$Npm = "npm", [string]$Output = "dist-native")
$ErrorActionPreference = "Stop"
$Root = $PSScriptRoot
$OutputPath = Join-Path $Root $Output
if ([IO.Path]::GetFullPath($OutputPath) -eq [IO.Path]::GetFullPath($Root)) {
    throw "The output directory must differ from the source directory."
}
$Bundle = Join-Path $OutputPath "prototype"
if (Test-Path $Bundle) { throw "Prototype output already exists. Choose a new -Output directory." }
Push-Location $Root
try {
    $SavedCargoTargetDir = $env:CARGO_TARGET_DIR
    try {
        if (-not $env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR = Join-Path $Root 'rename_win\target' }
        $RenameCargoTargetDir = $env:CARGO_TARGET_DIR
        $env:PATH = "$env:LOCALAPPDATA\DocFitTools\mingw64\bin;$env:PATH"
        cargo test --release --locked --manifest-path rename_win/Cargo.toml
        if ($LASTEXITCODE -ne 0) { throw "rename_win tests failed" }
        cargo build --release --locked --manifest-path rename_win/Cargo.toml --bin rename_win
        if ($LASTEXITCODE -ne 0) { throw "rename_win build failed" }
    } finally { $env:CARGO_TARGET_DIR = $SavedCargoTargetDir }
    & (Join-Path $Root 'scripts\build_flow_editor.ps1') -Npm $Npm
    & $Python -m pip check
    if ($LASTEXITCODE -ne 0) { throw "Python dependency check failed" }
    Push-Location (Join-Path $Root "flutter_ui")
    try {
        & $Flutter pub get --enforce-lockfile
        if ($LASTEXITCODE -ne 0) { throw "Flutter dependency check failed" }
        & $Flutter analyze
        if ($LASTEXITCODE -ne 0) { throw "Flutter analyze failed" }
        $PreviousTestPython = $env:DOCFIT_TEST_PYTHON
        $PreviousTestRepo = $env:DOCFIT_TEST_REPO
        try {
            $env:DOCFIT_TEST_PYTHON = (Get-Command $Python -ErrorAction Stop).Source
            $env:DOCFIT_TEST_REPO = $Root
            & $Flutter test
            $TestExitCode = $LASTEXITCODE
        } finally {
            $env:DOCFIT_TEST_PYTHON = $PreviousTestPython
            $env:DOCFIT_TEST_REPO = $PreviousTestRepo
        }
        if ($TestExitCode -ne 0) { throw "Flutter tests failed" }
        & $Flutter build windows --release --no-pub
        if ($LASTEXITCODE -ne 0) { throw "Flutter Windows build failed" }
    } finally { Pop-Location }
    & $Python -m PyInstaller --noconfirm --clean --onedir --console `
        --paths $Root --name HWP_AutoDocFit_Backend `
        --collect-submodules docfit_app --collect-submodules docfit_core --collect-submodules docfit_engine `
        --distpath (Join-Path $OutputPath "backend-build") `
        --workpath (Join-Path $OutputPath "work") `
        --specpath (Join-Path $OutputPath "spec") `
        (Join-Path $Root "docfit_app/backend_main.py")
    if ($LASTEXITCODE -ne 0) { throw "Backend packaging failed" }
    $Bundle = Join-Path $OutputPath "prototype"
    New-Item -ItemType Directory -Force $Bundle | Out-Null
    Copy-Item -Recurse -Force (Join-Path $Root "flutter_ui/build/windows/x64/runner/Release/*") $Bundle
    $Backend = Join-Path $Bundle "backend"
    New-Item -ItemType Directory -Force $Backend | Out-Null
    Copy-Item -Recurse -Force (Join-Path $OutputPath "backend-build/HWP_AutoDocFit_Backend/*") $Backend
    Copy-Item -Force (Join-Path $Root "MapoHwpAutoDocFitSecurity.dll") $Backend
    $NativeResources = Join-Path $Bundle "resources"
    New-Item -ItemType Directory -Force $NativeResources | Out-Null
    Copy-Item -Force (Join-Path $RenameCargoTargetDir "release\rename_win.exe") $NativeResources
    @{ kind = "native-prototype"; protocol_version = 1; engine_ready = $false;
       version = "1.74 Native Prototype 1" } | ConvertTo-Json | Set-Content -Encoding utf8 (Join-Path $Bundle "manifest.json")
    Get-ChildItem -Recurse -File $Bundle | ForEach-Object {
        $hash = Get-FileHash -Algorithm SHA256 $_.FullName
        "$($hash.Hash)  $($_.FullName.Substring($Bundle.Length + 1))"
    } | Set-Content -Encoding utf8 (Join-Path $OutputPath "SHA256SUMS.txt")
} finally { Pop-Location }
