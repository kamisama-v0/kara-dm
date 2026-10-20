; Installer Windows Kara Download Manager (Inno Setup 6). Per-user: tidak butuh hak admin.
; Bangun: iscc /DAppVersion=0.2.0 packaging\windows\kara-dm.iss  (setelah bundle.sh)
#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

[Setup]
AppId={{6E3F2B4A-7C1D-4E8A-9B2F-0A5D4C3B2E1F}
AppName=Kara Download Manager
AppVersion={#AppVersion}
AppPublisher=kamisama-v0
AppPublisherURL=https://github.com/kamisama-v0/kara-dm
AppSupportURL=https://github.com/kamisama-v0/kara-dm/issues
DefaultDirName={localappdata}\Programs\Kara DM
DefaultGroupName=Kara Download Manager
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputDir=..\..\dist\windows
OutputBaseFilename=kara-dm-{#AppVersion}-windows-x64-setup
SetupIconFile=kara-dm.ico
UninstallDisplayIcon={app}\kara-dm.exe
LicenseFile=..\..\LICENSE
Compression=lzma2/max
SolidCompression=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
WizardStyle=modern
CloseApplications=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "autostart"; Description: "Run at sign-in (catch downloads from the browser)"; Flags: checkedonce
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; Flags: unchecked

[Files]
Source: "..\..\dist\windows\kara-dm\*"; DestDir: "{app}"; Flags: recursesubdirs ignoreversion

[Icons]
Name: "{group}\Kara Download Manager"; Filename: "{app}\kara-dm.exe"
Name: "{group}\Uninstall Kara Download Manager"; Filename: "{uninstallexe}"
Name: "{userdesktop}\Kara Download Manager"; Filename: "{app}\kara-dm.exe"; Tasks: desktopicon
Name: "{userstartup}\Kara Download Manager"; Filename: "{app}\kara-dm.exe"; Parameters: "--service"; Tasks: autostart

[Run]
Filename: "{app}\kara-dm.exe"; Parameters: "--service"; Flags: nowait postinstall skipifsilent; Description: "Start Kara Download Manager"
; Pembaruan dari dalam aplikasi (installer dijalankan /SILENT): buka lagi jendela Kara DM setelah dipasang.
Filename: "{app}\kara-dm.exe"; Flags: nowait; Check: WizardSilent

[UninstallRun]
Filename: "{app}\kara-dm.exe"; Parameters: "--quit"; Flags: runhidden skipifdoesntexist; RunOnceId: "QuitKaraDM"
