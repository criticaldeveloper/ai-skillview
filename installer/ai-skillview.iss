#ifndef AppVersion
  #error AppVersion is required. Build with scripts\package.ps1.
#endif
#ifndef ProjectRoot
  #define ProjectRoot ".."
#endif

#define AppName "AI Skillview"
#define AppExe "AI Skillview.exe"

[Setup]
AppId={{85A8BA53-B7EC-4EB8-9082-C0CA664CC7E2}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher=Critical Developer
AppVerName={#AppName} {#AppVersion}
DefaultDirName={localappdata}\Programs\{#AppName}
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
SourceDir={#ProjectRoot}
OutputDir=dist
OutputBaseFilename=AI Skillview Setup
SetupIconFile=resources\app.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}
AppMutex=Local\CriticalDeveloper.AISkillview.Instance
SetupMutex=Local\CriticalDeveloper.AISkillview.Setup
CloseApplications=no
RestartApplications=no
DisableProgramGroupPage=yes
DisableWelcomePage=no
WizardStyle=modern
WizardImageFile=resources\setup-wizard.bmp
WizardSmallImageFile=resources\setup-header.bmp
Compression=lzma2
SolidCompression=yes
UsePreviousAppDir=yes
UsePreviousTasks=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "startup"; Description: "Start with Windows (minimized to the system tray)"; GroupDescription: "Preferences:"; Flags: unchecked
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Shortcuts:"; Flags: unchecked

[Files]
Source: "dist\{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"; WorkingDir: "{app}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; WorkingDir: "{app}"; Tasks: desktopicon

[Registry]
; Always record value cleanup, including when startup is enabled later in the app.
; Never remove the shared Windows Run key or the user's separate skills data folder.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueName: "{#AppName}"; Flags: dontcreatekey uninsdeletevalue
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueName: "{#AppName}"; Flags: deletevalue; Check: ShouldRemoveStartup
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "{#AppName}"; ValueData: """{app}\{#AppExe}"" --minimized"; Flags: uninsdeletevalue; Check: ShouldRegisterStartup

[Run]
Filename: "{app}\{#AppExe}"; Description: "Open {#AppName}"; WorkingDir: "{app}"; Flags: nowait postinstall skipifsilent

[Code]
const
  StartupKey = 'Software\Microsoft\Windows\CurrentVersion\Run';

var
  ExistingStartup: Boolean;
  StartupTaskInitialized: Boolean;
  StartupTaskOverride: Integer;

function ReadStartupTaskOverride: Integer;
var
  Index, Separator: Integer;
  Parameter, Tasks, Task: String;
begin
  Result := -1;
  for Index := 1 to ParamCount do
  begin
    Parameter := LowerCase(ParamStr(Index));
    if (Pos('/tasks=', Parameter) = 1) or
      (Pos('/mergetasks=', Parameter) = 1) then
    begin
      Tasks := RemoveQuotes(Copy(Parameter, Pos('=', Parameter) + 1, Length(Parameter)));
      while Tasks <> '' do
      begin
        Separator := Pos(',', Tasks);
        if Separator = 0 then
        begin
          Task := Trim(Tasks);
          Tasks := '';
        end
        else
        begin
          Task := Trim(Copy(Tasks, 1, Separator - 1));
          Delete(Tasks, 1, Separator);
        end;
        if (Task = 'startup') or (Task = '*startup') then
          Result := 1
        else if Task = '!startup' then
          Result := 0;
      end;
    end;
  end;
end;

function InitializeSetup: Boolean;
var
  Command: String;
begin
  ExistingStartup := RegQueryStringValue(HKCU, StartupKey, '{#AppName}', Command)
    and (Command <> '');
  StartupTaskOverride := ReadStartupTaskOverride;
  Result := True;
end;

procedure CurPageChanged(CurPageID: Integer);
begin
  if (CurPageID = wpSelectTasks) and not StartupTaskInitialized then
  begin
    { Reflect changes made through the app since the previous installation. }
    if (StartupTaskOverride = 1) or
      ((StartupTaskOverride = -1) and ExistingStartup) then
      WizardSelectTasks('startup')
    else
      WizardSelectTasks('!startup');
    StartupTaskInitialized := True;
  end;
end;

function ShouldRegisterStartup: Boolean;
begin
  if StartupTaskInitialized and not WizardSilent then
    Result := WizardIsTaskSelected('startup')
  else if StartupTaskOverride <> -1 then
    Result := StartupTaskOverride = 1
  else
    { A silent/skipped-task upgrade retains changes made through the app. }
    Result := ExistingStartup;
end;

function ShouldRemoveStartup: Boolean;
begin
  { An explicit interactive uncheck or !startup command-line choice disables it. }
  Result := not ShouldRegisterStartup and
    ((StartupTaskInitialized and not WizardSilent) or (StartupTaskOverride = 0));
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  Result := '';
  if ShouldRegisterStartup and
    (Length(ExpandConstant('"{app}\{#AppExe}" --minimized')) > 260) then
    Result := 'The installation folder is too long for Windows startup. Choose a shorter folder or disable Start with Windows.';
end;
