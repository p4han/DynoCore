; Dyno installer script (Inno Setup 6.3 or newer).
; Easiest way to use it: double-click make-installer.bat, which builds dyno.exe
; and then compiles this file into installer\dyno-win-x64.exe.

#define MyAppName "Dyno"
#define MyAppExe "dyno.exe"

[Setup]
AppId={{6F1B7C52-3D0A-4E8B-9A41-5C2D7E9B0A14}
AppName={#MyAppName}
AppVerName={#MyAppName}
AppPublisher=Dyno
AppComments=DynoCore: Free up RAM. Keep Windows fast.
VersionInfoDescription=DynoCore
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
OutputDir=installer
OutputBaseFilename=dyno-win-x64
SetupIconFile=dyno.ico
UninstallDisplayIcon={app}\{#MyAppExe}
UninstallDisplayName={#MyAppName}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=admin
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
DisableDirPage=yes
DisableReadyPage=yes
WizardImageFile=installer-art\wizard-164.bmp,installer-art\wizard-328.bmp
WizardSmallImageFile=installer-art\small-55.bmp,installer-art\small-110.bmp

[Messages]
WelcomeLabel1=Welcome to Dyno
WelcomeLabel2=Free up memory with one click, or let Dyno do it automatically whenever RAM runs low.%n%nIt's tiny, quiet, and runs from your system tray.%n%nClick Next to continue.
SelectTasksLabel2=Pick any extras, then click Install.
FinishedHeadingLabel=Dyno is ready
FinishedLabel=Find Dyno in your system tray. Click it to see your memory, or right-click to clean now.
FinishedLabelNoIcons=Find Dyno in your system tray. Click it to see your memory, or right-click to clean now.

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked

[Files]
Source: "target\release\{#MyAppExe}"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExe}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExe}"; Tasks: desktopicon
Name: "{commonstartup}\{#MyAppName}"; Filename: "{app}\{#MyAppExe}"; Parameters: "--tray"

[Run]
Filename: "{sys}\schtasks.exe"; Parameters: "/create /tn ""Dyno"" /tr """"{app}\{#MyAppExe}"""" --tray"" /sc onlogon /rl highest /f"; Flags: runhidden
Filename: "{app}\{#MyAppExe}"; Parameters: "--tray"; Description: "Launch Dyno into system tray"; Flags: nowait postinstall skipifsilent runascurrentuser

[UninstallRun]
Filename: "{sys}\taskkill.exe"; Parameters: "/F /IM {#MyAppExe}"; Flags: runhidden; RunOnceId: "DynoKill"
Filename: "{sys}\schtasks.exe"; Parameters: "/delete /tn ""Dyno"" /f"; Flags: runhidden; RunOnceId: "DynoTaskDel"

[UninstallDelete]
Type: files; Name: "{commonstartup}\{#MyAppName}.lnk"

[Code]
// Close a running Dyno before installing, so upgrades never hit a locked file.
function InitializeSetup(): Boolean;
var
  ResultCode: Integer;
begin
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM {#MyAppExe}', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Result := True;
end;
