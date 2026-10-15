#define ProductName "Digital Wellbeing"
#define ProductVersion "0.2.0"
#define ProductAppId "DigitalWellbeing.Desktop"

[Setup]
AppId={{B7BA29C1-195A-4755-A2D6-5B8B3B056C37}
AppName={#ProductName}
AppVersion={#ProductVersion}
AppPublisher=Aryan1771
DefaultDirName={localappdata}\Programs\DigitalWellbeing
DefaultGroupName={#ProductName}
PrivilegesRequired=lowest
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\..\target\installer
OutputBaseFilename=DigitalWellbeing-Setup-x64
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UninstallDisplayIcon={app}\digital-wellbeing.exe
LicenseFile=..\..\LICENSE

[Tasks]
Name: "autostart"; Description: "Start Digital Wellbeing when I sign in"; GroupDescription: "Startup:"

[Files]
Source: "..\..\target\release\digital-wellbeing.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\browser\chromium\*"; DestDir: "{app}\browser\chromium"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "browser-bridge-manifest.json"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\Digital Wellbeing"; Filename: "{app}\digital-wellbeing.exe"; WorkingDir: "{app}"; AppUserModelID: "{#ProductAppId}"
Name: "{group}\Uninstall Digital Wellbeing"; Filename: "{uninstallexe}"

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "DigitalWellbeing"; ValueData: """{app}\digital-wellbeing.exe"" --background"; Tasks: autostart; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Google\Chrome\NativeMessagingHosts\com.digitalwellbeing.browser_bridge"; ValueType: string; ValueName: ""; ValueData: "{app}\browser-bridge-manifest.json"; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Microsoft\Edge\NativeMessagingHosts\com.digitalwellbeing.browser_bridge"; ValueType: string; ValueName: ""; ValueData: "{app}\browser-bridge-manifest.json"; Flags: uninsdeletevalue

[Run]
Filename: "{app}\digital-wellbeing.exe"; Description: "Launch Digital Wellbeing"; Flags: nowait postinstall skipifsilent

[UninstallRun]
Filename: "{sys}\reg.exe"; Parameters: "delete ""HKCU\Software\Microsoft\Windows\CurrentVersion\Run"" /v ""DigitalWellbeing"" /f"; Flags: runhidden waituntilterminated; RunOnceId: "RemoveDigitalWellbeingStartup"

[UninstallDelete]
Type: files; Name: "{app}\browser-bridge-manifest.json"
