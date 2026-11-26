// 설치·업데이트·제거 전에 이 설치 폴더의 창을 안전하게 닫는다(RUSTSIDE2). setup.iss 의 [Code] 가 포함한다.
// 판단·창 찾기는 `sidebar.exe --close-all`(dock\src\sidebar_os.rs): 창마다 닫기를 한 번 보내 저장 확인을 거치게 하고,
// 강제 종료하지 않는다. 종료 코드 0 = 모두 닫힘, 3 = 남음.
// 상태: 닫기 요청 → 모두 닫힘(진행) | 남음 → (대화) 다시 시도·그래도 계속·취소 / (조용한 설치) 중단.
// 도우미를 실행하지 못하면 기록만 하고 진행한다(그 뒤의 Restart Manager·taskkill 은 예전과 같다).

#ifndef CloseWaitInteractive
  #define CloseWaitInteractive 20
#endif
#ifndef CloseWaitSilent
  // 앱의 자동 업데이트(/SILENT)는 사용자가 열린 창의 저장 확인에 답할 시간을 넉넉히 준다.
  #define CloseWaitSilent 300
#endif
#ifndef CloseSilentNotice
  #define CloseSilentNotice 1
#endif

function RunCloseAll(const Helper, Dir: String; const Seconds: Integer; var Remaining: String): Integer;
var
  Report: String;
  Lines: TArrayOfString;
  I, Code: Integer;
begin
  Result := -1;
  Remaining := '';
  Report := ExpandConstant('{tmp}\docfit-close.txt');
  DeleteFile(Report);
  if not FileExists(Helper) then Exit;
  if not Exec(Helper, '--close-all --install-dir "' + RemoveBackslashUnlessRoot(Dir) + '" --wait ' + IntToStr(Seconds) +
              ' --report "' + Report + '"', '', SW_HIDE, ewWaitUntilTerminated, Code) then Exit;
  Result := Code;
  if (Code = 3) and LoadStringsFromFile(Report, Lines) then
    for I := 0 to GetArrayLength(Lines) - 1 do
      Remaining := Remaining + '  · ' + Lines[I] + #13#10;
end;

// True = 설치(제거)를 계속한다.
function CloseAppWindows(const Helper, Dir: String; const Silent: Boolean): Boolean;
var
  Code, Seconds, Answer: Integer;
  Remaining: String;
begin
  Result := True;
  if Silent then Seconds := {#CloseWaitSilent} else Seconds := {#CloseWaitInteractive};
  while True do
  begin
    Code := RunCloseAll(Helper, Dir, Seconds, Remaining);
    if Code = 0 then
    begin
      InstallLog('열린 창을 모두 닫음');
      Exit;
    end;
    if Code <> 3 then
    begin
      InstallLog('창 닫기 도우미를 실행하지 못함(코드 ' + IntToStr(Code) + ') - 예전 방식으로 진행');
      Exit;
    end;
    InstallLog('닫히지 않은 창이 있음: ' + Remaining);
    if Silent then
    begin
      Result := False;
#if CloseSilentNotice
      MsgBox('저장 확인이 끝나지 않았거나 작업 중인 창이 있어 업데이트를 미뤘습니다.' + #13#10#13#10 + Remaining + #13#10 +
             '작업을 마친 뒤 앱을 다시 열어 업데이트해 주세요.', mbInformation, MB_OK);
#endif
      Exit;
    end;
    Answer := MsgBox('다음 창이 아직 열려 있습니다.' + #13#10#13#10 + Remaining + #13#10 +
                     '각 창에서 저장하거나 작업을 마친 뒤 닫아 주세요.' + #13#10#13#10 +
                     '[예] 다시 확인' + #13#10 +
                     '[아니요] 강제로 닫고 계속 (저장하지 않은 내용은 사라집니다)' + #13#10 +
                     '[취소] 지금은 하지 않음', mbConfirmation, MB_YESNOCANCEL);
    if Answer = IDCANCEL then
    begin
      Result := False;
      Exit;
    end;
    if Answer = IDNO then
    begin
      InstallLog('사용자가 남은 창을 강제로 닫고 계속하기로 함');
      Exit;
    end;
  end;
end;
