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
CloseApplications=yes
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
Filename: "{app}\{#AppExe}"; Description: "한글편집 후처리 실행"; Flags: nowait postinstall skipifsilent

Filename: "{app}\{#AppExe}"; Flags: nowait; Check: WizardSilent

[UninstallRun]
; 사용자 권한 모드 kfind(이 계정 것만)를 끝내야 실행 파일·색인을 지울 수 있다. 서비스 계정 프로세스는 건드리지 않는다.
Filename: "{sys}\taskkill.exe"; Parameters: "/F /IM kfind.exe /FI ""USERNAME eq {username}"""; Flags: runhidden; RunOnceId: "StopKfindUserMode"

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
