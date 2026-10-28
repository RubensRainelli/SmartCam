; Personal and Educational Free License (PEF License / PEFL)
; Version 1.0 - 2026-05-26
; Copyright (c) 2026 Rubens Rainelli
;
; SmartCam v2.0.0 — Official Windows x64 Installer (Inno Setup 6)
; Pure Rust + Windows Media Foundation Virtual Camera Architecture

#define MyAppName "SmartCam"
#define MyAppVersion "2.0.0"
#define MyAppPublisher "Rubens Rainelli"
#define MyAppURL "https://github.com/RubensRainelli/SmartCam"
#define MyAppExeName "SmartCam.exe"
#define MyAppId "{{9D4B8E71-3F5A-4E9C-A8B2-71C9D0E54F21}"

[Setup]
AppId={#MyAppId}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} v{#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}/issues
AppUpdatesURL={#MyAppURL}/releases
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
LicenseFile=LICENSE
SourceDir=..\..\..
OutputDir=dist
OutputBaseFilename=SmartCam-Setup-v2.0.0-x64
SetupIconFile=assets\icons\smartcam.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
UninstallDisplayName={#MyAppName}
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=admin
CloseApplications=force

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "italian"; MessagesFile: "compiler:Languages\Italian.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"
Name: "autostart"; Description: "Start SmartCam automatically when Windows starts"; GroupDescription: "Startup Options:"; Flags: unchecked

[Files]
Source: "target\release\SmartCam.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "target\release\smartcam_mf_source.dll"; DestDir: "{app}"; Flags: ignoreversion regserver 64bit
Source: "platforms\windows\installer\smartcam_publisher.cer"; DestDir: "{app}"; Flags: ignoreversion
Source: "assets\*"; DestDir: "{app}\assets"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "config\default_config.yaml"; DestDir: "{app}\config"; Flags: ignoreversion onlyifdoesntexist
Source: "LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "LICENSE.it"; DestDir: "{app}"; Flags: ignoreversion
Source: "NOTICE.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "README.it.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\assets\icons\smartcam.ico"; Comment: "SmartCam Virtual Camera & AI Studio"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\assets\icons\smartcam.ico"; Tasks: desktopicon

[Registry]
; WinGet & Windows Installed Apps Metadata
Root: HKLM; Subkey: "SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\{#MyAppId}_is1"; ValueType: string; ValueName: "DisplayVersion"; ValueData: "{#MyAppVersion}"; Flags: uninsdeletekey
Root: HKLM; Subkey: "SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\{#MyAppId}_is1"; ValueType: string; ValueName: "Publisher"; ValueData: "{#MyAppPublisher}"; Flags: uninsdeletekey
; Media Foundation FrameServer Mode Enablement
Root: HKLM; Subkey: "SOFTWARE\Microsoft\Windows Media Foundation\Platform"; ValueType: dword; ValueName: "EnableFrameServerMode"; ValueData: "1"
; Optional Startup Registration
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "SmartCam"; ValueData: """{app}\{#MyAppExeName}"" --minimized"; Flags: uninsdeletevalue; Tasks: autostart

[Run]
Filename: "certutil.exe"; Parameters: "-addstore -f ""Root"" ""{app}\smartcam_publisher.cer"""; Flags: runhidden waituntilterminated; StatusMsg: "Installing SmartCam Root Certificate..."
Filename: "certutil.exe"; Parameters: "-addstore -f ""TrustedPublisher"" ""{app}\smartcam_publisher.cer"""; Flags: runhidden waituntilterminated; StatusMsg: "Installing SmartCam Trusted Publisher Certificate..."
Filename: "regsvr32.exe"; Parameters: "/s ""{app}\smartcam_mf_source.dll"""; Flags: runhidden waituntilterminated; StatusMsg: "Registering SmartCam Media Foundation Virtual Camera..."
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(MyAppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent

[UninstallRun]
Filename: "regsvr32.exe"; Parameters: "/u /s ""{app}\smartcam_mf_source.dll"""; Flags: runhidden waituntilterminated; RunOnceId: "UnregisterSmartCamMF"
