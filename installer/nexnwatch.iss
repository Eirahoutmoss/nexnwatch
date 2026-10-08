; NexNWatch — Inno Setup 6 kurulum betiği
; Derleme: ISCC.exe /DAppVersion=0.3.0 installer\nexnwatch.iss

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

[Setup]
AppId={{FF1C2B1D-9213-4328-875C-A92E65804214}
AppName=NexNWatch
AppVersion={#AppVersion}
AppVerName=NexNWatch {#AppVersion}
AppPublisher=Hasan Güler
AppPublisherURL=https://github.com/Eirahoutmoss/nexnwatch
DefaultDirName={autopf}\NexNWatch
DefaultGroupName=NexNWatch
DisableProgramGroupPage=yes
PrivilegesRequired=admin
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
OutputDir=..\dist
OutputBaseFilename=NexNWatch-Setup-{#AppVersion}
SetupIconFile=..\assets\nexnwatch.ico
UninstallDisplayIcon={app}\nexnwatch.exe
UninstallDisplayName=NexNWatch
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
CloseApplications=force
RestartApplications=no

[Languages]
Name: "turkish"; MessagesFile: "compiler:Languages\Turkish.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"
Name: "autostart"; Description: "Windows oturum açılışında NexNWatch'u tepside başlat"; GroupDescription: "Başlangıç:"; Flags: unchecked

[Files]
Source: "..\target\release\nexnwatch.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\NexNWatch"; Filename: "{app}\nexnwatch.exe"
Name: "{autodesktop}\NexNWatch"; Filename: "{app}\nexnwatch.exe"; Tasks: desktopicon

[Run]
; Yönetici yetkisiyle oturum açılışında başlat (UAC sormadan): Görev Zamanlayıcı
Filename: "schtasks.exe"; Parameters: "/Create /F /TN NexNWatch /SC ONLOGON /RL HIGHEST /TR ""\""{app}\nexnwatch.exe\"" --minimized"""; Flags: runhidden; Tasks: autostart
Filename: "{app}\nexnwatch.exe"; Description: "{cm:LaunchProgram,NexNWatch}"; Flags: nowait postinstall skipifsilent shellexec

[UninstallRun]
Filename: "taskkill.exe"; Parameters: "/F /IM nexnwatch.exe"; Flags: runhidden; RunOnceId: "KillApp"
Filename: "schtasks.exe"; Parameters: "/Delete /F /TN NexNWatch"; Flags: runhidden; RunOnceId: "DelTask"
; Kalmış ETW oturumu varsa kapat
Filename: "logman.exe"; Parameters: "stop NexNWatch-KernelNetwork -ets"; Flags: runhidden; RunOnceId: "StopEtw"
