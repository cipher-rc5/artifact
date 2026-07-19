# Authenticode-sign the Windows executable with signtool.
#
# Required environment:
#   WINDOWS_CERTIFICATE_PFX_BASE64  base64 of the code-signing .pfx
#   WINDOWS_CERTIFICATE_PASSWORD    password for that .pfx
#
# Input:  $env:EXE_PATH (default: target/dist/artifact-windows-x86_64.exe)
# Effect: signs the .exe in place with an RFC-3161 timestamp.

$ErrorActionPreference = 'Stop'

$exe = $env:EXE_PATH
if ([string]::IsNullOrEmpty($exe)) {
  $exe = 'target/dist/artifact-windows-x86_64.exe'
}

if ([string]::IsNullOrEmpty($env:WINDOWS_CERTIFICATE_PFX_BASE64)) {
  Write-Error 'missing required secret: WINDOWS_CERTIFICATE_PFX_BASE64'
  exit 1
}
if ([string]::IsNullOrEmpty($env:WINDOWS_CERTIFICATE_PASSWORD)) {
  Write-Error 'missing required secret: WINDOWS_CERTIFICATE_PASSWORD'
  exit 1
}
if (-not (Test-Path $exe)) {
  Write-Error "executable not found at $exe"
  exit 1
}

$pfx = Join-Path $env:RUNNER_TEMP 'artifact-codesign.pfx'
[IO.File]::WriteAllBytes(
  $pfx,
  [Convert]::FromBase64String($env:WINDOWS_CERTIFICATE_PFX_BASE64)
)

try {
  # Locate signtool from the installed Windows SDK.
  $signtool = Get-ChildItem `
    'C:\Program Files (x86)\Windows Kits\10\bin\*\x64\signtool.exe' `
    -ErrorAction SilentlyContinue |
    Sort-Object FullName -Descending |
    Select-Object -First 1 -ExpandProperty FullName
  if ([string]::IsNullOrEmpty($signtool)) {
    $signtool = 'signtool.exe'
  }

  Write-Host "==> Signing $exe with $signtool"
  & $signtool sign `
    /f $pfx `
    /p $env:WINDOWS_CERTIFICATE_PASSWORD `
    /fd SHA256 `
    /tr http://timestamp.digicert.com `
    /td SHA256 `
    $exe
  if ($LASTEXITCODE -ne 0) { throw "signtool sign failed ($LASTEXITCODE)" }

  & $signtool verify /pa /v $exe
  if ($LASTEXITCODE -ne 0) { throw "signtool verify failed ($LASTEXITCODE)" }

  Write-Host "==> Signed and verified $exe"
} finally {
  Remove-Item $pfx -ErrorAction SilentlyContinue
}
