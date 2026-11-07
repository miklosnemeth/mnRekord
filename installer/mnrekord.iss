; Inno Setup script for mnRekord. Build with: installer\build.ps1
#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

[Setup]
AppId={{6B1E5C3A-7D52-4F0B-9A3E-2C4D8E91A7F5}
AppName=mnRekord
AppVersion={#AppVersion}
AppPublisher=Miklós Németh
DefaultDirName={autopf}\mnRekord
DefaultGroupName=mnRekord
UninstallDisplayIcon={app}\mnrekord.exe
UninstallDisplayName=mnRekord
SetupIconFile=..\assets\icon.ico
LicenseFile=..\LICENSE
OutputDir=..\dist
OutputBaseFilename=mnRekord-{#AppVersion}-setup
Compression=lzma2
SolidCompression=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Per user by default (no administrator rights); the dialog offers an all-users install.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
WizardStyle=modern
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "hungarian"; MessagesFile: "compiler:Languages\Hungarian.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "..\target\release\mnrekord.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\mnRekord"; Filename: "{app}\mnrekord.exe"
Name: "{group}\{cm:UninstallProgram,mnRekord}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\mnRekord"; Filename: "{app}\mnrekord.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\mnrekord.exe"; Description: "{cm:LaunchProgram,mnRekord}"; Flags: nowait postinstall skipifsilent
