# Builds the release exe and the installer: dist\mnRekord-<version>-setup.exe
# Needs Inno Setup 6 (https://jrsoftware.org/isinfo.php).
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
Set-Location $root

$version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value

cargo build --release
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

$candidates = @(@(
    (Get-Command iscc -ErrorAction SilentlyContinue).Source,
    "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe",
    "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
    "$env:ProgramFiles\Inno Setup 6\ISCC.exe"
) | Where-Object { $_ -and (Test-Path $_) })
if (-not $candidates) { throw "Inno Setup 6 was not found. Install it from https://jrsoftware.org/isinfo.php" }

& $candidates[0] "/DAppVersion=$version" installer\mnrekord.iss
if ($LASTEXITCODE -ne 0) { throw "ISCC failed" }
Get-Item "dist\mnRekord-$version-setup.exe" | Select-Object Name, Length
