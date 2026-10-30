<#
.SYNOPSIS
  Instala pcode.exe en Windows (por usuario, sin privilegios de administrador).
.DESCRIPTION
  Copia pcode.exe en %LOCALAPPDATA%\PublicCode\bin y añade esa carpeta al PATH del usuario.
  - Si se ejecuta desde la carpeta descomprimida de la release, usa el pcode.exe que hay al lado.
  - Con -Zip instala desde un .zip descargado y, si se indica -Sha256, verifica su suma antes.
.EXAMPLE
  .\install.ps1
.EXAMPLE
  .\install.ps1 -Zip .\pcode-v0.1.0-x86_64-pc-windows-msvc.zip -Sha256 <suma de SHA256SUMS>
#>
param(
  [string]$Zip,
  [string]$Sha256
)
$ErrorActionPreference = "Stop"
$dest = Join-Path $env:LOCALAPPDATA "PublicCode\bin"
New-Item -ItemType Directory -Force -Path $dest | Out-Null

if ($Zip) {
  if ($Sha256) {
    $real = (Get-FileHash -Algorithm SHA256 $Zip).Hash.ToLower()
    if ($real -ne $Sha256.ToLower()) { throw "La suma SHA-256 no coincide: $real" }
  }
  $tmp = Join-Path $env:TEMP ("pcode-" + [guid]::NewGuid())
  Expand-Archive -Path $Zip -DestinationPath $tmp
  $exe = Get-ChildItem -Path $tmp -Recurse -Filter pcode.exe | Select-Object -First 1
} else {
  $exe = Get-Item (Join-Path $PSScriptRoot "pcode.exe")
}
if (-not $exe) { throw "No se encontró pcode.exe" }
Copy-Item $exe.FullName -Destination (Join-Path $dest "pcode.exe") -Force

$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if (-not ($userPath -split ";" | Where-Object { $_ -eq $dest })) {
  [Environment]::SetEnvironmentVariable("Path", ($userPath.TrimEnd(";") + ";" + $dest), "User")
  Write-Host "Añadido al PATH del usuario: $dest (abre una terminal nueva)"
}
& (Join-Path $dest "pcode.exe") --version
