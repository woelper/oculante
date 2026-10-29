; The installer for Windows, made with Inno Setup 6. It installs oculante.exe for
; the user (no admin), or for all users when asked, and adds Oculante to "Open
; with" and to the default apps of Windows for every format it reads. Windows lets
; no app make itself the default, the last page offers to open that setting.
;
;   iscc /DAppVersion=1.0.0 /DNumericVersion=1.0.0 /DExe=..\..\target\release\oculante.exe res\windows\oculante.iss
;
; The formats below are those of SUPPORTED_EXTENSIONS, the unit test
; `ci_the_windows_installer_registers_every_format` says which lines are missing.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef NumericVersion
  #define NumericVersion "0.0.0"
#endif
#ifndef Exe
  #define Exe "..\..\target\release\oculante.exe"
#endif

[Setup]
; the same for every version, so a new one replaces the old
AppId={{39664534-07BE-474A-A843-D557C0D1A240}
AppName=Oculante
AppVersion={#AppVersion}
AppVerName=Oculante {#AppVersion}
AppPublisher=Johann Woelper
AppPublisherURL=https://github.com/woelper/oculante
AppSupportURL=https://github.com/woelper/oculante/issues
AppUpdatesURL=https://github.com/woelper/oculante/releases
VersionInfoVersion={#NumericVersion}
DefaultDirName={autopf}\Oculante
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog commandline
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
ChangesAssociations=yes
UninstallDisplayIcon={app}\oculante.exe
SetupIconFile=..\icons\icon.ico
WizardStyle=modern
Compression=lzma2/max
SolidCompression=yes
OutputBaseFilename=oculante_setup

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#Exe}"; DestDir: "{app}"; DestName: "oculante.exe"; Flags: ignoreversion
Source: "..\..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Oculante"; Filename: "{app}\oculante.exe"
Name: "{autodesktop}\Oculante"; Filename: "{app}\oculante.exe"; Tasks: desktopicon

[Registry]
; what opens a file, and its icon
Root: HKA; Subkey: "Software\Classes\Oculante.Image"; ValueType: string; ValueName: ""; ValueData: "Image"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Oculante.Image\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\oculante.exe,0"
Root: HKA; Subkey: "Software\Classes\Oculante.Image\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\oculante.exe"" ""%1"""
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "Oculante"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\oculante.exe"" ""%1"""
; the default apps of Windows list what is registered here
Root: HKA; Subkey: "Software\Oculante"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Oculante\Capabilities"; ValueType: string; ValueName: "ApplicationName"; ValueData: "Oculante"
Root: HKA; Subkey: "Software\Oculante\Capabilities"; ValueType: string; ValueName: "ApplicationDescription"; ValueData: "A fast image viewer with analysis and editing tools"
Root: HKA; Subkey: "Software\Oculante\Capabilities"; ValueType: string; ValueName: "ApplicationIcon"; ValueData: "{app}\oculante.exe,0"
Root: HKA; Subkey: "Software\RegisteredApplications"; ValueType: string; ValueName: "Oculante"; ValueData: "Software\Oculante\Capabilities"; Flags: uninsdeletevalue
; "oculante" in the run box
Root: HKA; Subkey: "Software\Microsoft\Windows\CurrentVersion\App Paths\oculante.exe"; ValueType: string; ValueName: ""; ValueData: "{app}\oculante.exe"; Flags: uninsdeletekey
; every format: in "Open with", and among those Oculante can be the default for
Root: HKA; Subkey: "Software\Classes\.bmp\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".bmp"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".bmp"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.dds\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".dds"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".dds"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.exr\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".exr"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".exr"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.ff\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".ff"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".ff"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.gif\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".gif"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".gif"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.hdr\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".hdr"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".hdr"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.ico\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".ico"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".ico"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.jpeg\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".jpeg"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".jpeg"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.jpg\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".jpg"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".jpg"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.jfif\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".jfif"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".jfif"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.png\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".png"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".png"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.pnm\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".pnm"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".pnm"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.psd\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".psd"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".psd"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.svg\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".svg"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".svg"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.tga\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".tga"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".tga"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.tif\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".tif"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".tif"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.tiff\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".tiff"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".tiff"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.webp\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".webp"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".webp"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.nef\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".nef"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".nef"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.cr2\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".cr2"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".cr2"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.dng\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".dng"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".dng"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.mos\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".mos"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".mos"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.erf\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".erf"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".erf"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.raf\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".raf"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".raf"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.arw\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".arw"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".arw"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.3fr\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".3fr"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".3fr"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.ari\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".ari"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".ari"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.srf\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".srf"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".srf"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.sr2\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".sr2"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".sr2"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.braw\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".braw"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".braw"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.r3d\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".r3d"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".r3d"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.icns\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".icns"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".icns"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.nrw\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".nrw"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".nrw"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.raw\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".raw"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".raw"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.cr3\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".cr3"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".cr3"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.crw\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".crw"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".crw"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.rw2\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".rw2"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".rw2"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.rwl\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".rwl"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".rwl"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.orf\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".orf"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".orf"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.ori\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".ori"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".ori"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.pef\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".pef"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".pef"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.srw\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".srw"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".srw"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.mrw\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".mrw"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".mrw"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.iiq\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".iiq"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".iiq"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.fff\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".fff"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".fff"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.kdc\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".kdc"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".kdc"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.mef\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".mef"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".mef"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.x3f\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".x3f"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".x3f"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.arq\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".arq"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".arq"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.avif\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".avif"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".avif"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.jxl\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".jxl"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".jxl"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.ppm\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".ppm"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".ppm"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.dcm\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".dcm"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".dcm"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.ima\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".ima"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".ima"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.qoi\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".qoi"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".qoi"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.ktx2\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".ktx2"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".ktx2"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.kra\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".kra"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".kra"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.ora\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".ora"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".ora"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.otb\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".otb"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".otb"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.pcx\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".pcx"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".pcx"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.sgi\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".sgi"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".sgi"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.wbmp\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".wbmp"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".wbmp"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.xbm\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".xbm"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".xbm"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.xpm\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".xpm"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".xpm"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.jp2\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".jp2"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".jp2"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.heif\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".heif"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".heif"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.heic\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".heic"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".heic"; ValueData: "Oculante.Image"
Root: HKA; Subkey: "Software\Classes\.hif\OpenWithProgids"; ValueType: string; ValueName: "Oculante.Image"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\Applications\oculante.exe\SupportedTypes"; ValueType: string; ValueName: ".hif"; ValueData: ""
Root: HKA; Subkey: "Software\Oculante\Capabilities\FileAssociations"; ValueType: string; ValueName: ".hif"; ValueData: "Oculante.Image"

[Run]
Filename: "{app}\oculante.exe"; Description: "{cm:LaunchProgram,Oculante}"; Flags: nowait postinstall skipifsilent
Filename: "ms-settings:defaultapps?registeredAppUser=Oculante"; Description: "Choose Oculante as the default image viewer"; Flags: shellexec nowait postinstall skipifsilent unchecked; Check: not IsAdminInstallMode
Filename: "ms-settings:defaultapps?registeredAppMachine=Oculante"; Description: "Choose Oculante as the default image viewer"; Flags: shellexec nowait postinstall skipifsilent unchecked; Check: IsAdminInstallMode
