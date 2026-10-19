#ifndef BundleDir
  #error BundleDir is required
#endif
#ifndef OutputDir
  #error OutputDir is required
#endif
#ifndef Arch
  #define Arch "arm64"
#endif
#ifndef TestProfile
  #define TestProfile ""
#endif
#if TestProfile != ""
  #define ProductId "GlazeBar-Setup-Isolated-Test"
  #define FileSuffix "-test"
#else
  #define ProductId "GlazeBar-GlazeWM-Community"
  #define FileSuffix ""
#endif

[Setup]
AppId={#ProductId}
AppName=GlazeBar + GlazeWM
AppVersion=0.1.0-preview.4
AppPublisher=lamininA1
AppPublisherURL=https://github.com/lamininA1/GlazeBar
AppSupportURL=https://github.com/lamininA1/GlazeBar/issues
DefaultDirName={localappdata}\Programs\GlazeBar-GlazeWM
DefaultGroupName=GlazeBar + GlazeWM
PrivilegesRequired=lowest
#if Arch == "arm64"
ArchitecturesAllowed=arm64
ArchitecturesInstallIn64BitMode=arm64
#else
ArchitecturesAllowed=x64compatible and not arm64
ArchitecturesInstallIn64BitMode=x64compatible
#endif
MinVersion=10.0.22000
DisableDirPage=yes
DisableProgramGroupPage=yes
DisableWelcomePage=no
LicenseFile={#BundleDir}\LICENSE
InfoBeforeFile=Setup-Info.txt
OutputDir={#OutputDir}
OutputBaseFilename=GlazeBar-GlazeWM-Setup-0.1.0-preview.4-{#Arch}{#FileSuffix}
Compression=lzma2/fast
SolidCompression=no
WizardStyle=modern
SetupLogging=yes
CloseApplications=no
RestartApplications=no
UninstallDisplayName=GlazeBar + GlazeWM (community setup)
UninstallDisplayIcon={app}\bundle\payload\GlazeBar.exe

[Languages]
Name: "korean"; MessagesFile: "compiler:Languages\Korean.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "autostart"; Description: "{cm:AutoStart}"; Flags: unchecked

[CustomMessages]
korean.AutoStart=Windows 로그인 시 GlazeBar + GlazeWM 자동 시작
english.AutoStart=Start GlazeBar + GlazeWM when I sign in

[Files]
Source: "{#BundleDir}\*"; DestDir: "{app}\bundle"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "Setup-Bridge.ps1"; DestDir: "{app}"; Flags: ignoreversion
Source: "Setup-Info.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "GlazeBar-Setup.iss"; DestDir: "{app}\setup-source"; Flags: ignoreversion
Source: "Build-Setup.ps1"; DestDir: "{app}\setup-source"; Flags: ignoreversion
Source: "Test-SetupStartup.ps1"; DestDir: "{app}\setup-source"; Flags: ignoreversion
Source: "Setup-Bridge.ps1"; DestDir: "{app}\setup-source"; Flags: ignoreversion
Source: "Setup-Info.txt"; DestDir: "{app}\setup-source"; Flags: ignoreversion
Source: "BUILDING.md"; DestDir: "{app}\setup-source"; Flags: ignoreversion
Source: "..\licenses\INNO-SETUP-LICENSE.txt"; DestDir: "{app}\setup-source"; Flags: ignoreversion

[Icons]
#if TestProfile == ""
Name: "{group}\GlazeBar + GlazeWM"; Filename: "{code:GetProfileRoot}\.glzr\glazewm\wm\glazewm.exe"; Parameters: "start"; WorkingDir: "{code:GetProfileRoot}\.glzr\glazewm\wm"
Name: "{group}\Installation and licenses"; Filename: "{app}\Setup-Info.txt"
Name: "{group}\Uninstall GlazeBar bundle"; Filename: "{uninstallexe}"
#endif

[Code]
var
  StagingRoot: String;
  PayloadInstalled, SetupCompleted: Boolean;

procedure ExitProcess(ExitCode: Cardinal);
  external 'ExitProcess@kernel32.dll stdcall';
function GetProfileRoot(Param: String): String;
begin
  Result := GetEnv('USERPROFILE');
end;

function InitializeSetup: Boolean;
var
  StatePath, ProfileRoot: String;
  I: Integer;
begin
  Result := False;
  ProfileRoot := GetProfileRoot('');
  if (ProfileRoot = '') or not DirExists(ProfileRoot) then begin
    SuppressibleMsgBox('Windows user profile could not be found.', mbError, MB_OK, IDOK);
    exit;
  end;
  StatePath := AddBackslash(ProfileRoot) + '.glzr\glazewm\community-install.json';
  { Exercise the production startup path without installing anything. }
  for I := 1 to ParamCount do
    if CompareText(ParamStr(I), '/CHECKSTARTUP') = 0 then begin
      Log('GlazeBar startup check passed: ' + StatePath);
      exit;
    end;
#if TestProfile != ""
  StatePath := '{#TestProfile}\.glzr\glazewm\community-install.json';
#endif
  Result := not FileExists(StatePath);
  if not Result then
    SuppressibleMsgBox('An existing GlazeBar community bundle was found. Uninstall it to restore its original settings before installing this version.', mbError, MB_OK, IDOK);
end;

function RunBridgeAt(BasePath, Action: String): Boolean;
var
  Code: Integer;
  Args: String;
begin
  Args := '-NoProfile -NonInteractive -ExecutionPolicy Bypass -File "' +
    BasePath + '\Setup-Bridge.ps1' + '" -Action ' + Action +
    ' -Bundle "' + BasePath + '\bundle' + '"';
  if Action = 'Install' then
    if WizardIsTaskSelected('autostart') then
      Args := Args + ' -AutoStart';
#if TestProfile != ""
  Args := Args + ' -TestProfile "{#TestProfile}"';
#endif
  Result := Exec(ExpandConstant('{sysnative}\WindowsPowerShell\v1.0\powershell.exe'),
    Args, BasePath, SW_HIDE, ewWaitUntilTerminated, Code);
  Result := Result and (Code = 0);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  Result := '';
  if PayloadInstalled then exit;
  ExtractTemporaryFiles('*');
  StagingRoot := ExpandConstant('{tmp}\') + '{app}';
  if not RunBridgeAt(StagingRoot, 'Install') then
    Result := 'Installation failed. See the GlazeBarSetup install.log for details.'
  else
    PayloadInstalled := True;
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssDone then SetupCompleted := True;
end;

procedure DeinitializeSetup;
begin
  if PayloadInstalled and not SetupCompleted then
    if not RunBridgeAt(StagingRoot, 'Uninstall') then
      SuppressibleMsgBox('Installation was interrupted and settings could not be restored. Keep the community backup and check the uninstall log.', mbError, MB_OK, IDOK);
end;
function InitializeUninstall: Boolean;
begin
  Result := RunBridgeAt(ExpandConstant('{app}'), 'CheckUninstall');
  if not Result then
    SuppressibleMsgBox('Restore could not finish. Close GlazeWM and GlazeBar and retry. The uninstaller and backups have been retained. Details: ' +
      ExpandConstant('{localappdata}\GlazeBarSetup\uninstall.log'), mbError, MB_OK, IDOK);
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then
    if not RunBridgeAt(ExpandConstant('{app}'), 'Uninstall') then begin
      SuppressibleMsgBox('Settings could not be restored. Setup files were retained. Check the uninstall log before retrying.', mbError, MB_OK, IDOK);
      ExitProcess(1);
    end;
end;
