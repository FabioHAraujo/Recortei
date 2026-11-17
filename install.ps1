# Instala o Recortei no Windows: compila (se preciso), copia para
# %LOCALAPPDATA%\Programs\Recortei, cria atalhos no Menu Iniciar e na
# Inicialização (abre junto com o Windows) e inicia o app.
#
#   powershell -ExecutionPolicy Bypass -File install.ps1             # instala
#   powershell -ExecutionPolicy Bypass -File install.ps1 -Uninstall  # remove
param([switch]$Uninstall)
$ErrorActionPreference = "Stop"

$Dir     = Split-Path -Parent $MyInvocation.MyCommand.Path
$Bin     = Join-Path $Dir "target\release\recortei.exe"
$Dest    = Join-Path $env:LOCALAPPDATA "Programs\Recortei"
$App     = Join-Path $Dest "recortei.exe"
$Startup = Join-Path ([Environment]::GetFolderPath("Startup")) "Recortei.lnk"
$Menu    = Join-Path ([Environment]::GetFolderPath("Programs")) "Recortei.lnk"

function Stop-Recortei {
    if (Test-Path $App) { & $App --quit; Start-Sleep -Milliseconds 500 }
    Get-Process recortei -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $App } | Stop-Process -Force
}

if ($Uninstall) {
    Stop-Recortei
    Remove-Item $Startup, $Menu -ErrorAction SilentlyContinue
    Remove-Item $Dest -Recurse -Force -ErrorAction SilentlyContinue
    Write-Host ">> Recortei removido. (Seus recortes em Imagens\Recortei foram mantidos.)"
    exit 0
}

# Compila se não houver binário ou se o código for mais novo que ele.
$newest = Get-ChildItem (Join-Path $Dir "src"), (Join-Path $Dir "Cargo.toml") -Recurse -File |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not (Test-Path $Bin) -or $newest.LastWriteTime -gt (Get-Item $Bin).LastWriteTime) {
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        Write-Host ">> É preciso o Rust para compilar: https://rustup.rs"
        exit 1
    }
    Write-Host ">> Compilando (cargo build --release)..."
    Push-Location $Dir
    try { cargo build --release; if ($LASTEXITCODE -ne 0) { throw "falha na compilação" } }
    finally { Pop-Location }
}

Stop-Recortei
New-Item -ItemType Directory -Force $Dest | Out-Null
Copy-Item $Bin $App -Force

$shell = New-Object -ComObject WScript.Shell
foreach ($lnk in @($Startup, $Menu)) {
    $s = $shell.CreateShortcut($lnk)
    $s.TargetPath = $App
    $s.WorkingDirectory = $Dest
    $s.Description = "Captura de área de tela com anotações"
    $s.Save()
}

Write-Host ">> Pronto. Iniciando o Recortei (ícone na bandeja; tecla Insert captura)..."
Start-Process $App
