# Compile PDFool (Rust) puis, si Inno Setup 6 est installe, genere l'installeur.
$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

cargo test --release
cargo build --release
New-Item -ItemType Directory -Force dist | Out-Null
Copy-Item target\release\PDFool.exe dist\PDFool.exe -Force
Write-Host "Application creee : $PWD\dist\PDFool.exe"

$iscc = @(
    "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe",
    "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
    "$env:ProgramFiles\Inno Setup 6\ISCC.exe"
) | Where-Object { Test-Path $_ } | Select-Object -First 1
if ($iscc) {
    & $iscc installer\PDFool.iss
    Write-Host "Installeur cree dans $PWD\installer\Output"
} else {
    Write-Host "Inno Setup 6 introuvable : installeur non genere."
}
