<#
.SYNOPSIS
  Instala o actualiza pcode en Windows (por usuario, sin permisos de administrador).
.DESCRIPTION
  Instala pcode.exe en %LOCALAPPDATA%\PublicCode\bin y añade esa carpeta al PATH del usuario.
  - Sin argumentos, descarga la última release de GitHub y verifica su SHA-256.
  - Desde la carpeta descomprimida de una release, instala el pcode.exe que hay al lado.
  - Con -Zip instala desde un .zip ya descargado (y lo verifica si se indica -Sha256).
.EXAMPLE
  irm https://raw.githubusercontent.com/jlasherasTracasa/PublicCode/main/install.ps1 | iex
.EXAMPLE
  .\install.ps1 -Version v0.4.0
#>
param(
  [string]$Version = $env:PCODE_VERSION,
  [string]$Zip,
  [string]$Sha256
)
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$repo = if ($env:PCODE_REPO) { $env:PCODE_REPO } else { "jlasherasTracasa/PublicCode" }
$dest = Join-Path $env:LOCALAPPDATA "PublicCode\bin"
New-Item -ItemType Directory -Force -Path $dest | Out-Null
$tmp = Join-Path ([IO.Path]::GetTempPath()) ("pcode-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $tmp | Out-Null

function Test-Sum([string]$File, [string]$Expected) {
  $real = (Get-FileHash -Algorithm SHA256 $File).Hash.ToLower()
  if ($real -ne $Expected.ToLower()) { throw "La suma SHA-256 de $File no coincide: $real" }
}

try {
  $local = if ($PSScriptRoot) { Join-Path $PSScriptRoot "pcode.exe" } else { $null }
  if ($Zip) {
    if ($Sha256) { Test-Sum $Zip $Sha256 }
    Expand-Archive -Path $Zip -DestinationPath $tmp
  } elseif ($local -and (Test-Path $local)) {
    Copy-Item $local -Destination $tmp
  } else {
    $headers = @{ "User-Agent" = "pcode-install" }
    if (-not $Version) {
      $Version = (Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest" -Headers $headers).tag_name
    }
    $name = "pcode-$Version-x86_64-pc-windows-msvc.zip"
    $base = "https://github.com/$repo/releases/download/$Version"
    Write-Host "Descargando $name..."
    $zipPath = Join-Path $tmp $name
    $sumsPath = Join-Path $tmp "SHA256SUMS"
    Invoke-WebRequest "$base/$name" -OutFile $zipPath -UseBasicParsing -Headers $headers
    Invoke-WebRequest "$base/SHA256SUMS" -OutFile $sumsPath -UseBasicParsing -Headers $headers
    $line = Get-Content $sumsPath | Where-Object { $_ -match ([regex]::Escape($name) + '$') } | Select-Object -First 1
    if (-not $line) { throw "SHA256SUMS no incluye $name" }
    Test-Sum $zipPath (($line -split '\s+')[0])
    Expand-Archive -Path $zipPath -DestinationPath $tmp
  }
  $exe = Get-ChildItem -Path $tmp -Recurse -Filter pcode.exe | Select-Object -First 1
  if (-not $exe) { throw "No se encontró pcode.exe" }
  $target = Join-Path $dest "pcode.exe"
  if (Test-Path $target) {
    # Un .exe en marcha no se puede sobrescribir, pero sí renombrar.
    $old = Join-Path $dest "pcode.antiguo.exe"
    Remove-Item $old -Force -ErrorAction SilentlyContinue
    Rename-Item $target -NewName "pcode.antiguo.exe" -ErrorAction SilentlyContinue
  }
  Copy-Item $exe.FullName -Destination $target -Force
} finally {
  Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
}

$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if (-not (($userPath -split ";") -contains $dest)) {
  [Environment]::SetEnvironmentVariable("Path", (($userPath.TrimEnd(";") + ";" + $dest).TrimStart(";")), "User")
  Write-Host "Añadido al PATH del usuario: $dest (las terminales nuevas ya lo verán)"
}
if (-not (($env:Path -split ";") -contains $dest)) { $env:Path = "$env:Path;$dest" }
& $target --version
Write-Host 'Siguiente: define $env:ANTHROPIC_API_KEY (o GEMINI_API_KEY u OPENAI_API_KEY) y ejecuta: pcode doctor'
