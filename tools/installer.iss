; The Windows installer (Inno Setup), built by the release workflow from the packaged folder:
; KNT_VERSION is the version, KNT_SRC the folder (Kontinuum.exe, knt.exe, ffmpeg.exe…).
#define AppVersion GetEnv("KNT_VERSION")
#define Src GetEnv("KNT_SRC")

[Setup]
AppId={{6E3B2E7A-1B7C-4F41-9C0A-4B4E5450AA01}
AppName=Kontinuum
AppVersion={#AppVersion}
AppPublisher=Almaper
AppPublisherURL=https://github.com/Almaper/Kontinuum
AppUpdatesURL=https://github.com/Almaper/Kontinuum/releases
DefaultDirName={autopf}\Kontinuum
DefaultGroupName=Kontinuum
OutputBaseFilename=Kontinuum-v{#AppVersion}-windows-x64-setup
OutputDir=..\dist
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
LicenseFile={#Src}\LICENSE
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UninstallDisplayName=Kontinuum

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "{#Src}\*"; DestDir: "{app}"; Flags: recursesubdirs ignoreversion

[Icons]
Name: "{group}\Kontinuum"; Filename: "{app}\Kontinuum.exe"
Name: "{group}\{cm:UninstallProgram,Kontinuum}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\Kontinuum"; Filename: "{app}\Kontinuum.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\Kontinuum.exe"; Description: "{cm:LaunchProgram,Kontinuum}"; Flags: nowait postinstall skipifsilent
