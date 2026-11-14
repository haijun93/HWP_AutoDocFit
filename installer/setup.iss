#define AppName "한글편집 후처리 (HWP_AutoDocFit)"
#define AppVersion GetEnv("DOCFIT_VERSION")
#define AppExe "HWP_AutoDocFit.exe"

[Setup]
AppId={{A7D3B5B8-824B-4F97-A33D-C208F6E895F4}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher=HWP_AutoDocFit
DefaultDirName={localappdata}\Programs\HWP_AutoDocFit
DefaultGroupName={#AppName}
PrivilegesRequired=lowest
OutputDir=..\dist
OutputBaseFilename=HWP_AutoDocFit_v{#AppVersion}_Setup
SetupIconFile=..\resources\app_icon.ico
UninstallDisplayIcon={app}\{#AppExe}
ArchitecturesInstallIn64BitMode=x64
ArchitecturesAllowed=x64
; force: 창 없는 도우미(앱이 띄운 숨은 프로세스)도 Restart Manager가 끝낸다. 그대로 두면 _internal의 DLL을 못 바꿔
; "DeleteFile 실패; 코드 5"가 난다(v2.2.65 오류 보고, 2026-11-14).
CloseApplications=force
RestartApplications=no
CloseApplicationsFilter=*.exe,*.dll,*.pyd
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
DisableProgramGroupPage=yes
Uninstallable=yes

[Languages]
Name: "korean"; MessagesFile: "compiler:Languages\Korean.isl"

[Tasks]
Name: "desktopicon"; Description: "바탕 화면 바로 가기 만들기"; GroupDescription: "추가 바로 가기:"; Flags: unchecked

[Files]
; kvf.exe와 SHA256은 resources 꾸러미에 포함. 설치만으로 자동 실행하지 않는다.
Source: "..\dist\HWP_AutoDocFit\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"; WorkingDir: "{app}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
; 몽땅검색(ktm.exe, Ctrl+Alt+Space): 기본 켬. 사용자가 설정에서 끈 경우(ktm.disabled)는 설치본만 바꾸고 켜지 않는다.
Filename: "{app}\_internal\resources\ktm.exe"; Parameters: "--install --respect-disabled"; Flags: runhidden waituntilterminated skipifdoesntexist
Filename: "{app}\{#AppExe}"; Description: "한글편집 후처리 실행"; Flags: nowait postinstall skipifsilent

Filename: "{app}\{#AppExe}"; Flags: nowait; Check: WizardSilent

[UninstallRun]
; 사용자 권한 모드 kfind(이 계정 것만)를 끝내야 실행 파일·색인을 지울 수 있다. 서비스 계정 프로세스는 건드리지 않는다.
Filename: "{sys}\taskkill.exe"; Parameters: "/F /IM kfind.exe /FI ""USERNAME eq {username}"""; Flags: runhidden; RunOnceId: "StopKfindUserMode"
; 앱을 지우면 몽땅검색도 끄고 자동 실행·설치본을 지운다(사용자 데이터 폴더의 설정·기록은 남김).
Filename: "{localappdata}\HwpAutoDocFit\ktm\bin\ktm.exe"; Parameters: "--uninstall"; Flags: runhidden waituntilterminated skipifdoesntexist; RunOnceId: "MongttangSearchUninstall"

[UninstallDelete]
Type: filesandordirs; Name: "{app}"
; Q16: 색인 삭제. 공개 색인({commonappdata})은 관리자 서비스가 있을 때 아래 [Code]가 kfind --uninstall로 지운다.
Type: filesandordirs; Name: "{commonappdata}\HwpAutoDocFit\index"
Type: filesandordirs; Name: "{localappdata}\HwpAutoDocFit\kfind"
Type: filesandordirs; Name: "{localappdata}\HwpAutoDocFit\index"

[Registry]
Root: HKCU; Subkey: "Software\Classes\SystemFileAssociations\.hwp\shell\HwpAutoDocFit"; Flags: dontcreatekey uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\SystemFileAssociations\.hwpx\shell\HwpAutoDocFit"; Flags: dontcreatekey uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\SystemFileAssociations\.pdf\shell\HwpAutoDocFit"; Flags: dontcreatekey uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\SystemFileAssociations\.png\shell\HwpAutoDocFit"; Flags: dontcreatekey uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\SystemFileAssociations\.jpg\shell\HwpAutoDocFit"; Flags: dontcreatekey uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\SystemFileAssociations\.jpeg\shell\HwpAutoDocFit"; Flags: dontcreatekey uninsdeletekey

[Code]
// 설치(업데이트) 전에 이 계정에서 돌고 있는 앱 도우미를 끝낸다(v2.2.65 오류 보고, 2026-11-14).
// 몽땅클립보드(kagent)·자료함(klib)·가상 폴더(kvf)·색인(kfind 사용자 모드)·위계(hierarchy)는 창이 없어
// Restart Manager가 정상 종료를 요청하지 못하고, 붙잡은 {app} 안 파일(_internal\VCRUNTIME140.dll 등) 교체가 실패한다.
// 관리자 서비스로 도는 kfind는 다른 계정이라 /FI USERNAME 조건으로 건드리지 않는다. 주 앱 창은 Restart Manager가 묻고 닫는다.
procedure StopHelper(const ExeName: String);
var
  ResultCode: Integer;
begin
  Exec(ExpandConstant('{sys}\taskkill.exe'),
       '/F /IM ' + ExeName + ' /FI "USERNAME eq ' + GetUserNameString + '"',
       '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  StopHelper('kagent.exe');
  StopHelper('klib.exe');
  StopHelper('kvf.exe');
  StopHelper('kfind.exe');
  StopHelper('hierarchy.exe');
  // 몽땅검색 단축키 앱도 창 없이 상주해 Restart Manager가 닫지 못한다(이 PC 재현: "모든 응용 프로그램을 자동으로 닫지 못했습니다", 2026-11-14).
  StopHelper('ktm.exe');
  Sleep(500);
  Result := '';
end;

// 설정에서 관리자 서비스(HwpAutoDocFit.Index)를 설치했다면 서비스가 {app} 안의 kfind.exe를 가리킨다.
// 파일을 지우기 전에 관리자 권한으로 kfind --uninstall(서비스 중지·삭제·공개 색인 삭제)을 부른다.
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  ResultCode: Integer;
  KfindExe: String;
begin
  if CurUninstallStep = usUninstall then
  begin
    KfindExe := ExpandConstant('{app}\_internal\resources\kfind.exe');
    if FileExists(KfindExe) and
       Exec(ExpandConstant('{sys}\sc.exe'), 'query HwpAutoDocFit.Index', '', SW_HIDE, ewWaitUntilTerminated, ResultCode) and
       (ResultCode = 0) then
      ShellExec('runas', KfindExe, '--uninstall', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  end;
end;
