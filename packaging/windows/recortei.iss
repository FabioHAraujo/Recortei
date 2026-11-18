; Instalador do Recortei para Windows (Inno Setup 6).
;
; Instala só para o usuário atual, sem pedir administrador, em
; %LOCALAPPDATA%\Programs\Recortei, com atalho no Menu Iniciar, início junto com
; o Windows (tarefa marcada por padrão) e desinstalador em "Aplicativos".
; Aceita /VERYSILENT /SUPPRESSMSGBOXES para instalar e desinstalar sem janelas.
;
; Compilação (da raiz do repositório, depois do `cargo build --release`):
;   iscc /DAppVersion=0.1.0 packaging\windows\recortei.iss
; Gera dist\Recortei-Setup-x86_64.exe. BinDir e OutputDir podem ser trocados
; com /DBinDir=... e /DOutputDir=... (caminhos relativos a esta pasta).

#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif
#ifndef BinDir
  #define BinDir "..\..\target\release"
#endif
#ifndef OutputDir
  #define OutputDir "..\..\dist"
#endif

[Setup]
; O AppId identifica a instalação (chave do desinstalador); não mude entre versões.
AppId={{E2A776E0-CF17-4FE6-B7E8-B8A953B0FFF4}
AppName=Recortei
AppVersion={#AppVersion}
AppVerName=Recortei {#AppVersion}
AppPublisher=Fábio Henrique Araújo
AppPublisherURL=https://github.com/FabioHAraujo/Recortei
AppSupportURL=https://github.com/FabioHAraujo/Recortei/issues
AppUpdatesURL=https://github.com/FabioHAraujo/Recortei/releases
VersionInfoVersion={#AppVersion}
VersionInfoDescription=Instalador do Recortei
DefaultDirName={localappdata}\Programs\Recortei
DefaultGroupName=Recortei
DisableDirPage=yes
DisableProgramGroupPage=yes
DisableReadyPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
ShowLanguageDialog=auto
WizardStyle=modern
SetupIconFile=..\icons\recortei.ico
UninstallDisplayIcon={app}\recortei.ico
UninstallDisplayName=Recortei
OutputDir={#OutputDir}
OutputBaseFilename=Recortei-Setup-x86_64
Compression=lzma2/max
SolidCompression=yes

[Languages]
Name: "brazilianportuguese"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[CustomMessages]
brazilianportuguese.StartupTask=Iniciar o Recortei junto com o Windows
brazilianportuguese.AppComment=Captura de área de tela com anotações
english.StartupTask=Start Recortei with Windows
english.AppComment=Screen area capture with annotations

[Tasks]
Name: "startup"; Description: "{cm:StartupTask}"

[Files]
Source: "{#BinDir}\recortei.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\icons\recortei.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion

[Icons]
; Mesmos nomes de atalho que o install.ps1 cria, para um substituir o outro.
Name: "{autoprograms}\Recortei"; Filename: "{app}\recortei.exe"; WorkingDir: "{app}"; IconFilename: "{app}\recortei.ico"; Comment: "{cm:AppComment}"
Name: "{autostartup}\Recortei"; Filename: "{app}\recortei.exe"; WorkingDir: "{app}"; IconFilename: "{app}\recortei.ico"; Comment: "{cm:AppComment}"; Tasks: startup

[Run]
Filename: "{app}\recortei.exe"; Description: "{cm:LaunchProgram,Recortei}"; Flags: nowait postinstall skipifsilent

[Code]
// Encerra a instância em execução (recortei.exe --quit) para liberar o .exe
// antes de substituí-lo ou removê-lo. Sem nada rodando, o --quit só sai.
procedure QuitRecortei();
var
  Exe: String;
  ResultCode: Integer;
begin
  Exe := ExpandConstant('{app}\recortei.exe');
  if FileExists(Exe) then
  begin
    Exec(Exe, '--quit', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
    Sleep(500);
  end;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  QuitRecortei();
  Result := '';
end;

function InitializeUninstall(): Boolean;
begin
  QuitRecortei();
  Result := True;
end;
