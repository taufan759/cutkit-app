; Installer CutKit (Inno Setup 6), dengan pola yang sama seperti herdlite.
; Build:  powershell -ExecutionPolicy Bypass -File installer\build.ps1
; Hasil:  installer\Output\cutkit-setup-<versi>.exe
;
; Dipasang per user, tanpa hak admin. Setelah file disalin, cutkit-cli menyiapkan
; kebutuhan (WebView2, ffmpeg, model AI, Claude/MCP) di jendela yang menampilkan progres.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

[Setup]
AppId={{B6F3D0A2-4C1E-4F7B-9A85-2D7E61C93F40}
AppName=CutKit
AppVersion={#AppVersion}
AppVerName=CutKit {#AppVersion}
AppPublisher=Taufan
AppPublisherURL=https://github.com/taufan759/cutkit-app
AppSupportURL=https://github.com/taufan759/cutkit-app/issues
DefaultDirName={localappdata}\CutKit
DisableProgramGroupPage=yes
DisableDirPage=auto
PrivilegesRequired=lowest
OutputDir=Output
OutputBaseFilename=cutkit-setup-{#AppVersion}
SetupIconFile=..\src-tauri\icons\icon.ico
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
WizardSizePercent=110
UninstallDisplayName=CutKit (editor video)
UninstallDisplayIcon={app}\cutkit-app.exe
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.17763
LicenseFile=..\LICENSE
CloseApplications=no

[Messages]
; Terjemahan bagian utama wizard (Inno Setup belum punya bahasa Indonesia resmi).
ButtonBack=< &Kembali
ButtonNext=&Lanjut >
ButtonInstall=&Pasang
ButtonCancel=Batal
ButtonFinish=&Selesai
ButtonBrowse=&Pilih...
WelcomeLabel1=Selamat datang di CutKit
WelcomeLabel2=CutKit adalah editor video ringan untuk Shorts 9:16 dan video YouTube: potong, susun, transisi, efek, caption otomatis dari suara, TTS, dan hapus background. Semuanya diproses di komputermu.%n%nInstaller akan menyiapkan semua yang dibutuhkan (ffmpeg dan model AI yang kamu pilih), jadi pastikan internet tersambung.
WizardLicense=Lisensi
LicenseLabel=Baca lisensi berikut sebelum melanjutkan.
LicenseLabel3=CutKit gratis dan open source (lisensi MIT).
LicenseAccepted=Saya &setuju
LicenseNotAccepted=Saya &tidak setuju
WizardSelectDir=Lokasi program
SelectDirDesc=Di mana CutKit dipasang?
SelectDirLabel3=CutKit akan dipasang di folder berikut (±10 MB, ditambah ±70 MB bila Node.js untuk Claude ikut dipasang).
SelectDirBrowseLabel=Klik Lanjut untuk memakai folder ini, atau Pilih... untuk folder lain.
WizardSelectTasks=Pilih yang ikut disiapkan
SelectTasksDesc=Komponen apa saja yang disiapkan sekarang?
SelectTasksLabel2=Yang tidak dicentang tetap bisa dipasang nanti dari dalam aplikasi. Klik Lanjut bila sudah.
WizardReady=Siap memasang
ReadyLabel1=CutKit siap dipasang.
ReadyLabel2a=Klik Pasang. Setelah file disalin, sebuah jendela akan menampilkan proses persiapan (mengunduh ffmpeg dan model AI). Biarkan terbuka sampai selesai.
WizardInstalling=Memasang
InstallingLabel=Tunggu sebentar, CutKit sedang dipasang dan disiapkan...
FinishedHeadingLabel=CutKit sudah terpasang
FinishedLabelNoIcons=CutKit sudah siap dipakai.%n%nBuka "CutKit" dari Start Menu kapan saja. Di layar pertama, coba "Coba contoh" untuk latihan atau "Buat video cepat" untuk langsung membuat video.
FinishedLabel=CutKit sudah siap dipakai.%n%nBuka "CutKit" dari Start Menu kapan saja. Di layar pertama, coba "Coba contoh" untuk latihan atau "Buat video cepat" untuk langsung membuat video.
ConfirmUninstall=Hapus CutKit dari komputer ini?%n%nProyek, video, dan template buatanmu TIDAK akan dihapus.
UninstalledAll=CutKit sudah dihapus. Proyek dan video buatanmu tetap utuh.

[CustomMessages]
GroupBasic=Wajib untuk mengedit:
TaskFfmpeg=ffmpeg untuk impor, preview, dan ekspor video (dilewati bila sudah ada, ±100 MB lewat winget)
GroupAi=Fitur AI (offline, diunduh sekali):
TaskCaption=Auto caption dari suara: model Whisper small (190 MB)
TaskTts=Suara TTS Bahasa Indonesia: Piper (±85 MB)
GroupExtra=Tambahan:
TaskClaude=Hubungkan ke Claude (MCP) supaya Claude bisa mengedit lewat chat (Node.js portabel ±30 MB bila belum ada)
TaskDesktop=Buat shortcut di Desktop
OpenNow=Buka CutKit sekarang
RunSetup=Menyiapkan CutKit: WebView2, ffmpeg, dan model AI...
AskModels=Hapus juga model AI (Whisper, Piper, hapus background) yang sudah diunduh?%n%nPilih Tidak kalau ingin memakainya lagi setelah memasang ulang CutKit.

[Tasks]
Name: "ffmpeg"; Description: "{cm:TaskFfmpeg}"; GroupDescription: "{cm:GroupBasic}"
Name: "caption"; Description: "{cm:TaskCaption}"; GroupDescription: "{cm:GroupAi}"
Name: "tts"; Description: "{cm:TaskTts}"; GroupDescription: "{cm:GroupAi}"; Flags: unchecked
Name: "claude"; Description: "{cm:TaskClaude}"; GroupDescription: "{cm:GroupExtra}"; Check: ClaudeDetected
Name: "desktop"; Description: "{cm:TaskDesktop}"; GroupDescription: "{cm:GroupExtra}"; Flags: unchecked

[Files]
Source: "..\target\release\cutkit-app.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\target\release\cutkit-cli.exe"; DestDir: "{app}\mcp"; Flags: ignoreversion
Source: "..\mcp\server.js"; DestDir: "{app}\mcp"; Flags: ignoreversion
Source: "..\ui\core\*.js"; DestDir: "{app}\ui\core"; Flags: ignoreversion
Source: "..\ui\templates\*.json"; DestDir: "{app}\ui\templates"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion
; Versi untuk serverInfo MCP.
Source: "..\package.json"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\CutKit"; Filename: "{app}\cutkit-app.exe"; Comment: "Editor video ringan"
Name: "{autodesktop}\CutKit"; Filename: "{app}\cutkit-app.exe"; Tasks: desktop

[Run]
; Jendela konsol sengaja terlihat supaya progres unduhan dan pesan error bisa dibaca.
Filename: "{app}\mcp\cutkit-cli.exe"; Parameters: "setup --installer{code:SetupFlags}"; StatusMsg: "{cm:RunSetup}"; Flags: waituntilterminated
Filename: "{app}\cutkit-app.exe"; Description: "{cm:OpenNow}"; Flags: postinstall nowait skipifsilent

[UninstallDelete]
; Hanya file milik CutKit; proyek dan video tidak pernah disentuh.
Type: filesandordirs; Name: "{app}\node"
Type: filesandordirs; Name: "{app}\mcp"
Type: filesandordirs; Name: "{app}\ui"
Type: filesandordirs; Name: "{app}\downloads"
Type: dirifempty; Name: "{app}"

[Code]
function ClaudeDetected: Boolean;
begin
  Result := FileExists(ExpandConstant('{%USERPROFILE}\.claude.json')) or DirExists(ExpandConstant('{userappdata}\Claude'));
end;

function SetupFlags(Param: String): String;
var
  Models: String;
begin
  Result := '';
  if WizardIsTaskSelected('ffmpeg') then Result := Result + ' --ffmpeg';
  Models := '';
  if WizardIsTaskSelected('caption') then Models := Models + 'whisper-small,';
  if WizardIsTaskSelected('tts') then Models := Models + 'piper-engine,piper-id,piper-id-config,';
  if Models <> '' then Result := Result + ' --download ' + Copy(Models, 1, Length(Models) - 1);
  if ClaudeDetected and WizardIsTaskSelected('claude') then Result := Result + ' --claude';
end;

procedure StopCutKit;
var
  Code: Integer;
begin
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM cutkit-app.exe', '', SW_HIDE, ewWaitUntilTerminated, Code);
end;

{ Tutup CutKit yang sedang terbuka dan lepas versi lama (installer NSIS 0.1.x) sebelum file ditimpa. }
function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  Code: Integer;
  Old: String;
begin
  Result := '';
  StopCutKit;
  if RegQueryStringValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Uninstall\CutKit', 'UninstallString', Old) then
  begin
    Old := RemoveQuotes(Old);
    if FileExists(Old) then
      Exec(Old, '/S', '', SW_HIDE, ewWaitUntilTerminated, Code);
    RegDeleteKeyIncludingSubkeys(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Uninstall\CutKit');
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Code: Integer;
  Args: String;
begin
  if CurUninstallStep = usUninstall then
  begin
    StopCutKit;
    Args := 'uninstall';
    { Mode senyap: model dibiarkan supaya tidak perlu diunduh ulang. }
    if not UninstallSilent then
      if MsgBox(CustomMessage('AskModels'), mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
        Args := Args + ' --delete-models';
    Exec(ExpandConstant('{app}\mcp\cutkit-cli.exe'), Args, '', SW_HIDE, ewWaitUntilTerminated, Code);
  end;
end;
