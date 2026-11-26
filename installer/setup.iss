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
; kvf.exe, pdf-edit.exe, outliner.exe, outliner_win.exe, settings_win.exe, docview_win.exe, photoview_win.exe, convert_win.exe, dock.exe, qms_bench.exe와 SHA256은 resources 꾸러미에 포함. Dock은 DOCK2에서 자동 시작하지 않는다.
; 설치 전 안전 종료 도우미(PrepareToInstall 이 임시 폴더에 풀어 실행한다). 이전 버전에 없어도 되도록 설치 파일에 담는다.
Source: "..\dist\HWP_AutoDocFit\_internal\resources\sidebar.exe"; DestName: "docfit-close.exe"; Flags: dontcopy skipifsourcedoesntexist
Source: "..\dist\HWP_AutoDocFit\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"; WorkingDir: "{app}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
; 몽땅검색(ktm.exe, Ctrl+Alt+Space): 기본 켬. 사용자가 설정에서 끈 경우(ktm.disabled)는 설치본만 바꾸고 켜지 않는다.
Filename: "{app}\_internal\resources\ktm.exe"; Parameters: "--install --respect-disabled"; Flags: runhidden waituntilterminated skipifdoesntexist; BeforeInstall: LogRunStart('ktm.exe'); AfterInstall: LogRunEnd('ktm.exe')
; 퀵가상폴더(kvf.exe, Win+Shift+F): 기본 켬. 사용자가 설정에서 끈 경우(kvf.disabled)는 설치본만 바꾸고 켜지 않는다.
Filename: "{app}\_internal\resources\kvf.exe"; Parameters: "--install --respect-disabled"; Flags: runhidden waituntilterminated skipifdoesntexist; BeforeInstall: LogRunStart('kvf.exe'); AfterInstall: LogRunEnd('kvf.exe')
Filename: "{app}\{#AppExe}"; Description: "한글편집 후처리 실행"; Flags: nowait postinstall skipifsilent

Filename: "{app}\{#AppExe}"; Flags: nowait; Check: WizardSilent

[UninstallRun]
; 사용자 권한 모드 kfind(이 계정 것만)를 끝내야 실행 파일·색인을 지울 수 있다. 서비스 계정 프로세스는 건드리지 않는다.
Filename: "{sys}\taskkill.exe"; Parameters: "/F /IM kfind.exe /FI ""USERNAME eq {username}"""; Flags: runhidden; RunOnceId: "StopKfindUserMode"
; 앱을 지우면 몽땅검색도 끄고 자동 실행·설치본을 지운다(사용자 데이터 폴더의 설정·기록은 남김).
Filename: "{localappdata}\HwpAutoDocFit\ktm\bin\ktm.exe"; Parameters: "--uninstall"; Flags: runhidden waituntilterminated skipifdoesntexist; RunOnceId: "MongttangSearchUninstall"
; 앱을 지우면 퀵가상폴더도 끄고 자동 실행·설치본을 지운다.
Filename: "{localappdata}\HwpAutoDocFit\kvf\kvf.exe"; Parameters: "--uninstall"; Flags: runhidden waituntilterminated skipifdoesntexist; RunOnceId: "KvfUninstall"

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
; 탐색기 오른쪽 버튼 메뉴 '몽땅파일변환'(CTXMENU2). 등록은 앱 설정 '탐색기 우클릭 메뉴'를 켤 때만 하고, 제거 때 새 키를 지운다(위 줄은 예전 '한글편집 후처리' 키 흔적 정리).
Root: HKCU; Subkey: "Software\Classes\SystemFileAssociations\.hwp\shell\MongttangFileConvert"; Flags: dontcreatekey uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\SystemFileAssociations\.hwpx\shell\MongttangFileConvert"; Flags: dontcreatekey uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\SystemFileAssociations\.pdf\shell\MongttangFileConvert"; Flags: dontcreatekey uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\SystemFileAssociations\.png\shell\MongttangFileConvert"; Flags: dontcreatekey uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\SystemFileAssociations\.jpg\shell\MongttangFileConvert"; Flags: dontcreatekey uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\SystemFileAssociations\.jpeg\shell\MongttangFileConvert"; Flags: dontcreatekey uninsdeletekey

; 사이드바 '로그온 때 시작'(사이드바 오른쪽 메뉴에서만 켠다, 기본 꺼짐). 제거 때 값만 지운다.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: none; ValueName: "HwpAutoDocFitSidebar"; Flags: dontcreatekey uninsdeletevalue

[Code]
// 설치 기록(INSTALL1, 2026-11-22): 설치·정리 단계의 결과를 %LOCALAPPDATA%\HwpAutoDocFit\logs\install.log에 덧붙인다.
// 기록 실패는 설치를 막지 않는다(예외를 삼킨다). Inno는 [Run] 종료 코드를 [Code]에 주지 않으므로
// [Run]은 "시작 전 대상 파일 유무"와 "끝남"만 남기고, 종료 코드는 /LOG 로 만든 Inno 자체 로그(Running ... / Process exit code)로 확인한다.
procedure InstallLog(const Msg: String);
var
  Dir: String;
begin
  try
    Dir := ExpandConstant('{localappdata}\HwpAutoDocFit\logs');
    ForceDirectories(Dir);
    SaveStringToFile(Dir + '\install.log',
      GetDateTimeString('yyyy-mm-dd hh:nn:ss', '-', ':') + ' v{#AppVersion} ' + Msg + #13#10, True);
  except
  end;
end;

#include "close_windows.iss"

procedure LogRunStart(const ExePath: String);
var
  P: String;
begin
  P := ExpandConstant('{app}\_internal\resources\' + ExePath);
  if FileExists(P) then
    InstallLog('[Run] 시작: ' + P)
  else
    InstallLog('[Run] 건너뜀(파일 없음): ' + P);
end;

procedure LogRunEnd(const Name: String);
begin
  InstallLog('[Run] 끝: ' + Name);
end;

// 설치(업데이트) 전에 이 계정에서 돌고 있는 앱 도우미를 끝낸다(v2.2.65 오류 보고, 2026-11-14).
// 몽땅클립보드(kagent)·자료함(klib)·가상 폴더(kvf)·색인(kfind 사용자 모드)·위계(hierarchy)는 창이 없어
// Restart Manager가 정상 종료를 요청하지 못하고, 붙잡은 {app} 안 파일(_internal\VCRUNTIME140.dll 등) 교체가 실패한다.
// 관리자 서비스로 도는 kfind는 다른 계정이라 /FI USERNAME 조건으로 건드리지 않는다. 주 앱 창은 Restart Manager가 묻고 닫는다.
procedure StopHelper(const ExeName: String);
var
  ResultCode: Integer;
begin
  if Exec(ExpandConstant('{sys}\taskkill.exe'),
       '/F /IM ' + ExeName + ' /FI "USERNAME eq ' + GetUserNameString + '"',
       '', SW_HIDE, ewWaitUntilTerminated, ResultCode) then
  begin
    // taskkill: 0=종료함, 128=해당 프로세스 없음(정상). 그 밖의 코드만 기록한다.
    if (ResultCode <> 0) and (ResultCode <> 128) then
      InstallLog('taskkill ' + ExeName + ' 종료 코드 ' + IntToStr(ResultCode));
  end
  else
    InstallLog('taskkill ' + ExeName + ' 실행 실패: ' + SysErrorMessage(ResultCode));
end;

// 잠긴 실행 파일 이름 바꾸기(INSTALL3, 2026-11-23): 관리자 색인 서비스(HwpAutoDocFit.Index)가 {app}\_internal\resources\kfind.exe 를
// 직접 실행하므로 앱을 모두 닫아도 이 파일은 잠겨 있다. 실행 중인 파일은 지울 수는 없지만 이름은 바꿀 수 있으므로,
// 복사 전에 잠긴 exe/dll 만 `<이름>.old-<시각>` 으로 바꿔 두면 새 파일이 원래 이름으로 설치된다(안 그러면 DeleteFile 코드 5 → 전체 롤백).
// 잠기지 않은 파일은 건드리지 않는다. 설치가 끝까지 가면 .old-* 를 지우고(잠겨 있으면 다음 설치 때 정리),
// 중간에 중단·롤백되면 원래 이름으로 되돌린다(원래 이름 파일이 없을 때만).
var
  RenamedOrig: array of String;
  RenamedOld: array of String;
  RenamedCount: Integer;
  KfindRenamed: Boolean;

function ResourcesDir: String;
begin
  Result := ExpandConstant('{app}\_internal\resources\');
end;

// 쓰기 공유 없이 열 수 있는지 확인한다(파일은 바꾸지 않음). 실행 중인 exe·로드된 dll 이면 열기가 실패한다.
function IsFileLocked(const Path: String): Boolean;
var
  FS: TFileStream;
begin
  Result := False;
  try
    FS := TFileStream.Create(Path, fmOpenReadWrite or fmShareExclusive);
    FS.Free;
  except
    Result := True;
  end;
end;

procedure RememberRename(const Orig, OldPath: String);
begin
  SetArrayLength(RenamedOrig, RenamedCount + 1);
  SetArrayLength(RenamedOld, RenamedCount + 1);
  RenamedOrig[RenamedCount] := Orig;
  RenamedOld[RenamedCount] := OldPath;
  RenamedCount := RenamedCount + 1;
end;

// 앞선 설치가 남긴 .old-* 정리: 원래 이름 파일이 없으면 되돌리고(앞선 설치가 비정상 종료한 경우), 있으면 지운다(잠겨 있으면 그대로 둔다).
procedure CleanStaleOld;
var
  FR: TFindRec;
  Dir, Name, Orig: String;
  P: Integer;
begin
  Dir := ResourcesDir;
  if not DirExists(Dir) then Exit;
  try
    if FindFirst(Dir + '*.old-*', FR) then
    try
      repeat
        Name := FR.Name;
        P := Pos('.old-', Name);
        if (P > 1) and (FR.Attributes and FILE_ATTRIBUTE_DIRECTORY = 0) then
        begin
          Orig := Dir + Copy(Name, 1, P - 1);
          if not FileExists(Orig) then
          begin
            if RenameFile(Dir + Name, Orig) then
              InstallLog('이전 설치가 남긴 ' + Name + ' 를 원래 이름으로 되돌림');
          end
          else
            DeleteFile(Dir + Name);
        end;
      until not FindNext(FR);
    finally
      FindClose(FR);
    end;
  except
    InstallLog('오래된 .old-* 정리 중 오류(무시)');
  end;
end;

procedure RenameLockedResources;
var
  FR: TFindRec;
  Dir, Name, Stamp, OldPath: String;
  Pat: Integer;
  Patterns: array of String;
begin
  RenamedCount := 0;
  KfindRenamed := False;
  Dir := ResourcesDir;
  if not DirExists(Dir) then Exit;
  Stamp := GetDateTimeString('yyyymmddhhnnss', #0, #0);
  SetArrayLength(Patterns, 3);
  Patterns[0] := '*.exe';
  Patterns[1] := '*.dll';
  Patterns[2] := '*.pyd';
  try
    for Pat := 0 to 2 do
    begin
      if FindFirst(Dir + Patterns[Pat], FR) then
      try
        repeat
          Name := FR.Name;
          if (FR.Attributes and FILE_ATTRIBUTE_DIRECTORY = 0) and (Pos('.old-', Name) = 0) and IsFileLocked(Dir + Name) then
          begin
            OldPath := Dir + Name + '.old-' + Stamp;
            if RenameFile(Dir + Name, OldPath) then
            begin
              RememberRename(Dir + Name, OldPath);
              if CompareText(Name, 'kfind.exe') = 0 then KfindRenamed := True;
              InstallLog('잠긴 파일 이름 변경: ' + Name + ' -> ' + Name + '.old-' + Stamp);
            end
            else
              InstallLog('잠긴 파일 이름 변경 실패: ' + Name + ' (' + SysErrorMessage(DLLGetLastError) + ')');
          end;
        until not FindNext(FR);
      finally
        FindClose(FR);
      end;
    end;
  except
    InstallLog('잠긴 파일 확인 중 오류(무시)');
  end;
end;

// 이름을 바꿔 둔 파일을 원래대로: 원래 이름 파일이 없을 때만(새 파일이 설치돼 있으면 그대로 둔다).
// Final 이 참(설치 성공)이면 새 꾸러미에 없던 파일만 되돌리고, 나머지는 .old-* 를 지운다. 거짓(실패)이면 모두 되돌린다.
procedure FinishRenamed(const Final: Boolean);
var
  I: Integer;
begin
  for I := 0 to RenamedCount - 1 do
  begin
    if not FileExists(RenamedOrig[I]) then
    begin
      if RenameFile(RenamedOld[I], RenamedOrig[I]) then
        InstallLog('원래 이름으로 복원: ' + ExtractFileName(RenamedOrig[I]))
      else
        InstallLog('복원 실패(수동 확인 필요): ' + RenamedOld[I]);
    end
    else if Final then
    begin
      if DeleteFile(RenamedOld[I]) then
        InstallLog('옛 파일 삭제: ' + ExtractFileName(RenamedOld[I]))
      else
        InstallLog('옛 파일이 실행 중이라 남겨 둠(다음 설치 때 정리): ' + ExtractFileName(RenamedOld[I]));
    end
    else
      InstallLog('복원 생략(원래 이름 파일이 이미 있음): ' + ExtractFileName(RenamedOrig[I]));
  end;
  RenamedCount := 0;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  InstallLog('설치 준비 시작');
  // 먼저 열린 창들에 닫기를 요청해 저장 확인을 거친다. 아래의 강제 종료는 창 없는 도우미를 위한 것이다.
  try
    ExtractTemporaryFile('docfit-close.exe');
  except
    InstallLog('창 닫기 도우미가 설치 파일에 없음');
  end;
  if not CloseAppWindows(ExpandConstant('{tmp}\docfit-close.exe'), ExpandConstant('{app}'), WizardSilent) then
  begin
    Result := '열려 있는 창을 닫지 않아 설치를 중단했습니다. 작업을 저장하고 창을 닫은 뒤 다시 실행해 주세요.';
    Exit;
  end;
  StopHelper('kagent.exe');
  StopHelper('klib.exe');
  StopHelper('kvf.exe');
  StopHelper('kfind.exe');
  StopHelper('hierarchy.exe');
  // 몽땅검색 단축키 앱도 창 없이 상주해 Restart Manager가 닫지 못한다(이 PC 재현: "모든 응용 프로그램을 자동으로 닫지 못했습니다", 2026-11-14).
  StopHelper('ktm.exe');
  // 우클릭 메뉴 전달 프로그램(mdm-ctx.exe)은 클릭 때만 잠깐 도는 비상주 프로그램이지만, 모으는 동안(최대 30초) 파일을 붙잡을 수 있다.
  StopHelper('mdm-ctx.exe');
  // 아웃라이너 창(outliner_win.exe)이 열려 있으면 {app} 안 파일을 붙잡는다(설정 outliner_ui='native').
  StopHelper('outliner_win.exe');
  // 설정 창(settings_win.exe)이 열려 있으면 {app} 안 파일을 붙잡는다(설정 settings_ui='native').
  StopHelper('settings_win.exe');
  // 문서뷰 창(docview_win.exe)이 열려 있으면 {app} 안 파일을 붙잡는다(설정 docview_ui='native').
  StopHelper('docview_win.exe');
  // 사진뷰 창(photoview_win.exe)이 열려 있으면 {app} 안 파일을 붙잡는다(설정 photoview_ui='native').
  StopHelper('photoview_win.exe');
  // 변환 창(convert_win.exe)이 열려 있으면 {app} 안 파일을 붙잡는다(설정 conversion_ui='native').
  StopHelper('convert_win.exe');
  // 파일명 변경 미리보기 창(rename_win.exe)이 열려 있으면 설치 파일 교체를 막는다.
  StopHelper('rename_win.exe');
  // 사이드바(sidebar.exe)는 계속 떠 있는 창이라 {app} 안 파일을 붙잡는다.
  StopHelper('sidebar.exe');
  Sleep(500);
  // 앱·도우미를 끝낸 뒤에도 잠긴 파일(색인 서비스의 kfind.exe 등)은 이름을 바꿔 둔다.
  CleanStaleOld;
  RenameLockedResources;
  Result := '';
end;

// 설치 완료 확인(PKGWIN1, 2026-11-22): 설치가 파일 교체 도중 끊기면(앱 실행 중 DeleteFile 코드 5 등) Inno는 조용히 되돌리고
// 끝낸다. 시작·완료·미완료를 install.log 에 남기고, 완료 직후 필수 Win32 창 프로그램 유무를 확인해 빠졌으면 안내한다.
// 기존 설치 동작(파일 목록·순서·[Run])은 바꾸지 않는다. 기록·안내 실패는 설치를 막지 않는다.
var
  InstallStarted: Boolean;
  InstallFinished: Boolean;

function MissingWindowExes: String;
var
  Names: array of String;
  I: Integer;
begin
  Result := '';
  SetArrayLength(Names, 7);
  Names[0] := 'outliner_win.exe';
  Names[1] := 'settings_win.exe';
  Names[2] := 'docview_win.exe';
  Names[3] := 'photoview_win.exe';
  Names[4] := 'convert_win.exe';
  Names[5] := 'rename_win.exe';
  Names[6] := 'mdm-ctx.exe';
  for I := 0 to 6 do
    if not FileExists(ExpandConstant('{app}\_internal\resources\' + Names[I])) then
      Result := Result + ' ' + Names[I];
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  Missing: String;
begin
  if CurStep = ssInstall then
  begin
    InstallStarted := True;
    InstallLog('파일 설치 시작');
  end;
  if CurStep = ssPostInstall then
  begin
    InstallFinished := True;
    FinishRenamed(True);
    Missing := MissingWindowExes;
    if Missing = '' then
      InstallLog('파일 설치 끝(필수 창 프로그램 확인됨)')
    else
    begin
      InstallLog('파일 설치 끝, 그러나 꾸러미에 없는 파일:' + Missing);
      if not WizardSilent then
        MsgBox('설치 파일 일부(' + Trim(Missing) + ')가 빠져 있습니다.' + #13#10 +
               '한/글 후처리 앱을 모두 닫고(트레이 아이콘 포함) 설치 프로그램을 다시 실행해 주세요.', mbError, MB_OK);
    end;
    if KfindRenamed then
    begin
      InstallLog('색인 서비스가 실행 중이어서 새 색인 프로그램은 서비스를 다시 시작해야 적용됩니다');
      if not WizardSilent then
        MsgBox('색인 서비스가 실행 중이어서 새 색인 프로그램은 서비스를 다시 시작해야 적용됩니다.' + #13#10 +
               '(관리자 권한으로 서비스 HwpAutoDocFit.Index 를 다시 시작하세요.)', mbInformation, MB_OK);
    end;
  end;
end;

procedure DeinitializeSetup;
begin
  if InstallStarted and (not InstallFinished) then
  begin
    InstallLog('설치가 끝나지 않고 중단됨(변경은 되돌려짐)');
    FinishRenamed(False);
    if not WizardSilent then
      MsgBox('설치가 완료되지 않았습니다. 한/글 후처리 앱이 실행 중이면 파일을 바꿀 수 없습니다.' + #13#10 +
             '앱을 모두 닫고(트레이 아이콘 포함) 설치 프로그램을 다시 실행해 주세요.', mbError, MB_OK);
  end;
end;

// 제거 전에도 열린 창의 저장 확인을 거친다. 닫지 않으면 제거를 시작하지 않는다.
function InitializeUninstall: Boolean;
begin
  Result := CloseAppWindows(ExpandConstant('{app}\_internal\resources\sidebar.exe'), ExpandConstant('{app}'), UninstallSilent);
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
    begin
      if ShellExec('runas', KfindExe, '--uninstall', '', SW_HIDE, ewWaitUntilTerminated, ResultCode) then
        InstallLog('제거: kfind --uninstall 종료 코드 ' + IntToStr(ResultCode))
      else
        InstallLog('제거: kfind --uninstall 실행 실패(권한 거부 등): ' + SysErrorMessage(ResultCode));
    end;
  end;
end;
