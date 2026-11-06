# Bangun installer CutKit secara lokal.
# Butuh: Rust (cargo), Node.js (npm), dan Inno Setup 6 (ISCC.exe).
# Pemakaian:  powershell -ExecutionPolicy Bypass -File installer\build.ps1 [-Iscc "C:\path\ISCC.exe"]
param([string]$Iscc = '')
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Set-Location $root

$version = (Get-Content src-tauri\tauri.conf.json -Raw | ConvertFrom-Json).version

cargo build --release -p cutkit-cli
if ($LASTEXITCODE -ne 0) { throw 'cargo build cutkit-cli gagal' }
npx tauri build --no-bundle
if ($LASTEXITCODE -ne 0) { throw 'tauri build gagal' }

if (-not $Iscc) {
    $cands = @(
        "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
        "$env:ProgramFiles\Inno Setup 6\ISCC.exe",
        "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe",
        'D:\DevTools\InnoSetup\ISCC.exe'
    )
    $Iscc = $cands | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $Iscc) { throw 'ISCC.exe (Inno Setup 6) tidak ditemukan. Pasang dari https://jrsoftware.org/isdl.php atau beri -Iscc.' }

& $Iscc "/DAppVersion=$version" "$PSScriptRoot\cutkit.iss"
if ($LASTEXITCODE -ne 0) { throw 'ISCC gagal' }
Get-Item "$PSScriptRoot\Output\cutkit-setup-$version.exe" | Select-Object FullName, @{n='MB';e={[math]::Round($_.Length/1MB,2)}}
