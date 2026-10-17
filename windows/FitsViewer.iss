#ifndef AppVersion
  #define AppVersion "0.1.4rc2"
#endif
#ifndef PayloadDir
  #define PayloadDir "payload"
#endif
#ifndef ReleaseDir
  #define ReleaseDir "..\release\0.1.4rc2"
#endif
#define WheelName "fitsviewer-" + AppVersion + "-py3-none-win_amd64.whl"

[Setup]
AppId={{9E94705D-9E69-4E53-8876-8383E8C471B1}
AppName=FitsViewer
AppVersion={#AppVersion}
AppPublisher=FitsViewer contributors
AppPublisherURL=https://github.com/minglinwang86/FitsViewer
AppSupportURL=https://github.com/minglinwang86/FitsViewer/issues
DefaultDirName={localappdata}\Programs\FitsViewer
DefaultGroupName=FitsViewer
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
DisableWelcomePage=no
DisableProgramGroupPage=yes
LicenseFile=..\LICENSE
UninstallDisplayIcon={app}\fitsviewer.exe
SetupIconFile=app.ico
ChangesAssociations=yes
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
OutputDir={#ReleaseDir}
OutputBaseFilename=FitsViewer-{#AppVersion}-windows-x86_64-setup
VersionInfoVersion=0.1.4.2

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked
Name: "associate"; Description: "Use FitsViewer as the default for .fits, .fit and .fts files"; Flags: unchecked

[Files]
Source: "{#PayloadDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\FitsViewer"; Filename: "{app}\fitsviewer.exe"; AppUserModelID: "FitsViewer.Desktop"
Name: "{group}\FITS default app settings"; Filename: "{app}\fitsviewer.exe"; Parameters: "--desktop-default settings"
Name: "{group}\Uninstall FitsViewer"; Filename: "{uninstallexe}"
Name: "{autodesktop}\FitsViewer"; Filename: "{app}\fitsviewer.exe"; Tasks: desktopicon; AppUserModelID: "FitsViewer.Desktop"

[Run]
Filename: "{app}\fitsviewer.exe"; Description: "Launch FitsViewer"; Flags: nowait postinstall skipifsilent unchecked
Filename: "{app}\fitsviewer.exe"; Parameters: "--desktop-default settings"; Description: "Confirm FITS defaults in Windows Settings"; Flags: nowait postinstall skipifsilent; Check: NeedDefaultSettings

[Code]
var
  EnvPage: TWizardPage;
  EnvChoice: TNewComboBox;
  EnvPath: TNewEdit;
  DependencyCheck, JupyterDefaultCheck: TNewCheckBox;
  EnvExecutables: TStringList;
  PythonButton, CondaButton: TNewButton;
  LinkedPython: String;

function PythonIn(const Candidate: String): String;
begin
  Result := Candidate;
  if DirExists(Candidate) then begin
    Result := AddBackslash(Candidate) + 'python.exe';
    if not FileExists(Result) then Result := AddBackslash(Candidate) + 'Scripts\python.exe';
  end;
end;

procedure AddEnvironment(const Candidate, LabelText: String);
var Executable: String;
begin
  Executable := PythonIn(Candidate);
  if not FileExists(Executable) then Exit;
  if EnvExecutables.IndexOf(Executable) >= 0 then Exit;
  EnvExecutables.Add(Executable);
  EnvChoice.Items.Add(LabelText + ' - ' + Executable);
end;

procedure AddRegisteredPython(const RootKey: Integer);
var Versions: TArrayOfString; I: Integer; Prefix: String;
begin
  if RegGetSubkeyNames(RootKey, 'Software\Python\PythonCore', Versions) then
    for I := 0 to GetArrayLength(Versions)-1 do
      if RegQueryStringValue(RootKey, 'Software\Python\PythonCore\' + Versions[I] + '\InstallPath', '', Prefix) then
        AddEnvironment(Prefix, 'Python ' + Versions[I]);
end;

procedure ChoiceChanged(Sender: TObject);
begin
  EnvPath.Enabled := EnvChoice.ItemIndex <> 0;
  DependencyCheck.Enabled := EnvPath.Enabled;
  JupyterDefaultCheck.Enabled := EnvPath.Enabled;
  if EnvChoice.ItemIndex > 1 then EnvPath.Text := EnvExecutables[EnvChoice.ItemIndex];
end;

procedure BrowsePython(Sender: TObject);
var Selected: String;
begin
  Selected := EnvPath.Text;
  if GetOpenFileName('Select Python executable', Selected, '', 'Python executable|python.exe|Executables|*.exe', 'exe') then begin
    EnvChoice.ItemIndex := 1; ChoiceChanged(EnvChoice); EnvPath.Text := Selected;
  end;
end;

procedure BrowseConda(Sender: TObject);
var Selected: String;
begin
  Selected := '';
  if BrowseForFolder('Select an existing Conda or Python environment folder', Selected, False) then begin
    EnvChoice.ItemIndex := 1; ChoiceChanged(EnvChoice); EnvPath.Text := PythonIn(Selected);
  end;
end;

procedure InitializeWizard;
var CaptionLabel: TNewStaticText; Lines: TArrayOfString; I: Integer; ExplicitPython: String;
begin
  EnvPage := CreateCustomPage(wpSelectTasks, 'Python / Conda environment (optional)',
    'The desktop viewer works without Python. Choose an environment only for JupyterLab / Notebook integration.');
  CaptionLabel := TNewStaticText.Create(EnvPage);
  CaptionLabel.Parent := EnvPage.Surface; CaptionLabel.Caption := 'Existing environment:';
  CaptionLabel.Top := ScaleY(4);
  EnvChoice := TNewComboBox.Create(EnvPage);
  EnvChoice.Parent := EnvPage.Surface; EnvChoice.Top := ScaleY(25);
  EnvChoice.Width := EnvPage.SurfaceWidth; EnvChoice.Style := csDropDownList;
  EnvChoice.Items.Add('No Python association - desktop viewer only');
  EnvChoice.Items.Add('Choose a Python executable or Conda environment folder');
  EnvChoice.ItemIndex := 0; EnvChoice.OnChange := @ChoiceChanged;
  EnvExecutables := TStringList.Create;
  EnvExecutables.CaseSensitive := False;
  EnvExecutables.Add(''); EnvExecutables.Add('');
  EnvPath := TNewEdit.Create(EnvPage);
  EnvPath.Parent := EnvPage.Surface; EnvPath.Top := ScaleY(60); EnvPath.Width := EnvPage.SurfaceWidth;
  PythonButton := TNewButton.Create(EnvPage);
  PythonButton.Parent := EnvPage.Surface; PythonButton.Top := ScaleY(92);
  PythonButton.Width := ScaleX(150); PythonButton.Caption := 'Browse Python...'; PythonButton.OnClick := @BrowsePython;
  CondaButton := TNewButton.Create(EnvPage);
  CondaButton.Parent := EnvPage.Surface; CondaButton.Top := ScaleY(92); CondaButton.Left := ScaleX(165);
  CondaButton.Width := ScaleX(180); CondaButton.Caption := 'Browse Conda / venv...'; CondaButton.OnClick := @BrowseConda;
  DependencyCheck := TNewCheckBox.Create(EnvPage);
  DependencyCheck.Parent := EnvPage.Surface; DependencyCheck.Top := ScaleY(136);
  DependencyCheck.Width := EnvPage.SurfaceWidth; DependencyCheck.Checked := True;
  DependencyCheck.Caption := 'Install or update Jupyter dependencies (downloads from PyPI)';
  JupyterDefaultCheck := TNewCheckBox.Create(EnvPage);
  JupyterDefaultCheck.Parent := EnvPage.Surface; JupyterDefaultCheck.Top := ScaleY(160);
  JupyterDefaultCheck.Width := EnvPage.SurfaceWidth; JupyterDefaultCheck.Checked := True;
  JupyterDefaultCheck.Caption := 'Default FITS viewer in JupyterLab (double-click files)';
  CaptionLabel := TNewStaticText.Create(EnvPage);
  CaptionLabel.Parent := EnvPage.Surface; CaptionLabel.Top := ScaleY(190); CaptionLabel.Width := EnvPage.SurfaceWidth;
  CaptionLabel.AutoSize := False; CaptionLabel.Height := ScaleY(46); CaptionLabel.WordWrap := True;
  CaptionLabel.Caption := 'Python 3.10+ and JupyterLab 4.4+ (<5) are required for integration. Restart JupyterLab after installation.';
  AddRegisteredPython(HKCU); AddRegisteredPython(HKLM64); AddRegisteredPython(HKLM32);
  AddEnvironment(GetEnv('CONDA_PREFIX'), 'Conda');
  AddEnvironment(AddBackslash(GetEnv('USERPROFILE')) + 'miniconda3', 'Conda base');
  AddEnvironment(AddBackslash(GetEnv('USERPROFILE')) + 'anaconda3', 'Conda base');
  AddEnvironment(ExpandConstant('{commonappdata}\anaconda3'), 'Conda base');
  if LoadStringsFromFile(AddBackslash(GetEnv('USERPROFILE')) + '.conda\environments.txt', Lines) then
    for I := 0 to GetArrayLength(Lines)-1 do AddEnvironment(Trim(Lines[I]), 'Conda');
  ExplicitPython := ExpandConstant('{param:PYTHON|}');
  if ExplicitPython = '' then ExplicitPython := ExpandConstant('{param:CONDAENV|}');
  if ExplicitPython <> '' then begin EnvChoice.ItemIndex := 1; EnvPath.Text := PythonIn(ExplicitPython); end;
  DependencyCheck.Checked := Lowercase(ExpandConstant('{param:WITHJUPYTER|yes}')) = 'yes';
  JupyterDefaultCheck.Checked := Lowercase(ExpandConstant('{param:JUPYTERDEFAULT|yes}')) = 'yes';
  ChoiceChanged(EnvChoice);
end;

function ValidateEnvironment: String;
var ExitCode: Integer;
begin
  Result := '';
  if EnvChoice.ItemIndex <> 0 then begin
    EnvPath.Text := PythonIn(Trim(EnvPath.Text));
    if not FileExists(EnvPath.Text) then begin
      Result := 'Select an existing Python executable or Conda environment folder.'; Exit;
    end;
    if not Exec(EnvPath.Text, '-c "import sys,pip; assert sys.version_info >= (3,10)"', '', SW_HIDE, ewWaitUntilTerminated, ExitCode) or (ExitCode <> 0) then begin
      Result := 'This environment needs Python 3.10 or newer and pip.'; Exit;
    end;
    if not Exec(EnvPath.Text, '-c "import importlib.metadata as m, importlib.util as u; v=m.version(''jupyterlab'') if u.find_spec(''jupyterlab'') else ''0''; assert int(v.split(''.'')[0])<5"', '', SW_HIDE, ewWaitUntilTerminated, ExitCode) or (ExitCode <> 0) then begin
      Result := 'JupyterLab 5.x is not supported yet. Select a separate JupyterLab 4 environment, or choose desktop only. No downgrade will be performed.'; Exit;
    end;
    if not DependencyCheck.Checked then
      if not Exec(EnvPath.Text, '-c "import importlib.metadata as m; v=m.version(''jupyterlab'').split(''.''); assert int(v[0])==4 and int(v[1])>=4; import jupyter_server_proxy,IPython"', '', SW_HIDE, ewWaitUntilTerminated, ExitCode) or (ExitCode <> 0) then
        Result := 'This environment needs JupyterLab 4.4+ (<5), jupyter-server-proxy and IPython. Enable dependency installation or select another environment.';
  end;
end;

function NextButtonClick(CurPageID: Integer): Boolean;
var Problem: String;
begin
  Result := True;
  if CurPageID = EnvPage.ID then begin
    Problem := ValidateEnvironment;
    if Problem <> '' then begin MsgBox(Problem, mbError, MB_OK); Result := False; end;
  end;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  Result := ValidateEnvironment;
end;

function NeedDefaultSettings: Boolean;
var I: Integer; Owner: String; Extensions: TArrayOfString;
begin
  Result := False;
  if not WizardIsTaskSelected('associate') then Exit;
  Extensions := ['.fits', '.fit', '.fts'];
  for I := 0 to 2 do
    if RegQueryStringValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\' + Extensions[I] + '\UserChoice', 'ProgId', Owner) then
      if Owner <> 'FitsViewer.FITS' then Result := True;
end;

procedure CurStepChanged(CurStep: TSetupStep);
var ExitCode: Integer; Parameters, Action, Summary: String;
begin
  if CurStep = ssPostInstall then begin
    if EnvChoice.ItemIndex <> 0 then begin
      LinkedPython := PythonIn(EnvPath.Text);
      WizardForm.StatusLabel.Caption := 'Installing Jupyter integration into the selected environment...';
      Parameters := '"' + ExpandConstant('{app}\windows\link_environment.py') + '" --wheel "' + ExpandConstant('{app}\{#WheelName}') + '" --state "' + ExpandConstant('{localappdata}\FitsViewer\environment.json') + '" --default-viewer ';
      if JupyterDefaultCheck.Checked then Parameters := Parameters + 'yes' else Parameters := Parameters + 'no';
      if DependencyCheck.Checked then Parameters := Parameters + ' --with-jupyter';
      if not Exec(LinkedPython, Parameters, '', SW_HIDE, ewWaitUntilTerminated, ExitCode) or (ExitCode <> 0) then
        RaiseException('Jupyter integration failed. See ' + ExpandConstant('{localappdata}\FitsViewer\install-jupyter.log') + '. Re-run Setup after resolving the reported error.');
    end;
    Action := 'register';
    if WizardIsTaskSelected('associate') then Action := 'yes';
    if not Exec(ExpandConstant('{app}\fitsviewer.exe'), '--desktop-default ' + Action, '', SW_HIDE, ewWaitUntilTerminated, ExitCode) or (ExitCode <> 0) then
      RaiseException('Windows file association registration failed.');
    Summary := 'FitsViewer installation successful.' + #13#10 + #13#10 + 'Desktop viewer: installed (no server port required).' + #13#10;
    if LinkedPython <> '' then
      Summary := Summary + 'Python environment: ' + LinkedPython + #13#10 + 'JupyterLab binding: enabled. Restart JupyterLab and refresh the browser.' + #13#10
    else Summary := Summary + 'Python environment: none selected.' + #13#10 + 'JupyterLab binding: not requested.' + #13#10;
    if WizardIsTaskSelected('associate') then begin
      if NeedDefaultSettings then Summary := Summary + 'Default FITS viewer: confirm in Windows Settings.'
      else Summary := Summary + 'Default FITS viewer: FitsViewer.';
    end else Summary := Summary + 'Default FITS viewer: existing setting retained.';
    WizardForm.FinishedLabel.Caption := Summary;
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var ExitCode: Integer;
begin
  if CurUninstallStep = usUninstall then
    Exec(ExpandConstant('{app}\fitsviewer.exe'), '--desktop-default remove', '', SW_HIDE, ewWaitUntilTerminated, ExitCode);
end;

procedure DeinitializeSetup;
begin
  if EnvExecutables <> nil then EnvExecutables.Free;
end;
