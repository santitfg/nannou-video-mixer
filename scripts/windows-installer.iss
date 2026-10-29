; Inno Setup script for the Windows installer. Built by
; scripts\build-windows-installer.ps1, which passes /DAppVersion, /DSourceDir
; (the staged dist\windows folder) and /DOutputDir.
;
; Per-user install (no admin prompt) in %LOCALAPPDATA%\Programs\Video Mixer.
; The program reads and writes shaders\, settings, presets and the last
; session in its working directory, so the shortcuts start it in
; %APPDATA%\Video Mixer, seeded with the bundled shaders: new ones are added
; on every install, ones the user edited are never overwritten.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceDir
  #define SourceDir "..\dist\windows"
#endif
#ifndef OutputDir
  #define OutputDir "..\dist"
#endif
#define AppName "Video Mixer"
#define AppExe "nannou_sketch.exe"
#define DataDir "{userappdata}\Video Mixer"

[Setup]
AppId={{C9D78E48-9CBD-44AA-9B31-368A7A69A0C6}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
OutputDir={#OutputDir}
OutputBaseFilename=Video-Mixer-{#AppVersion}-x64-setup
SetupIconFile={#SourceDir}\icon.ico
UninstallDisplayIcon={app}\icon.ico
UninstallDisplayName={#AppName}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes

[Languages]
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Dirs]
Name: "{#DataDir}"; Flags: uninsneveruninstall

[Files]
Source: "{#SourceDir}\{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\icon.ico"; DestDir: "{app}"; Flags: ignoreversion
; Untouched copy next to the program, to restore a shader from.
Source: "{#SourceDir}\shaders\*"; DestDir: "{app}\shaders"; Flags: ignoreversion recursesubdirs createallsubdirs
; The working copy the program uses (kept on uninstall, like the user's settings).
Source: "{#SourceDir}\shaders\*"; DestDir: "{#DataDir}\shaders"; Flags: onlyifdoesntexist uninsneveruninstall recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"; WorkingDir: "{#DataDir}"; IconFilename: "{app}\icon.ico"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; WorkingDir: "{#DataDir}"; IconFilename: "{app}\icon.ico"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExe}"; WorkingDir: "{#DataDir}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent
