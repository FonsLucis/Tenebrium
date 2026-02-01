param(
  [string]$Version = "0.1.0",
  [string]$OutDir = "dist",
  [switch]$Sign,
  [string]$GpgKey = ""
)

$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$dist = Join-Path $root $OutDir
New-Item -ItemType Directory -Force -Path $dist | Out-Null

Write-Host "Building release artifacts..."
Push-Location $root
cargo build --release -p tenebriumd
cargo build --release -p tenebrium-cli
Pop-Location

$tenebriumd = Join-Path $root "target/release/tenebriumd.exe"
$tenebriumcli = Join-Path $root "target/release/tenebrium-cli.exe"

Copy-Item $tenebriumd (Join-Path $dist "tenebriumd-$Version.exe") -Force
Copy-Item $tenebriumcli (Join-Path $dist "tenebrium-cli-$Version.exe") -Force

Write-Host "Generating SHA256 checksums..."
$checksums = @()
Get-ChildItem $dist -Filter "*.exe" | ForEach-Object {
  $hash = (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLower()
  $checksums += "$hash  $($_.Name)"
}
$checksums | Set-Content (Join-Path $dist "SHA256SUMS")

if ($Sign) {
  Write-Host "Signing artifacts with GPG..."
  $gpgArgs = @("--detach-sign", "--armor")
  if ($GpgKey -ne "") {
    $gpgArgs += @("--local-user", $GpgKey)
  }
  & gpg @gpgArgs (Join-Path $dist "SHA256SUMS")
  Get-ChildItem $dist -Filter "*.exe" | ForEach-Object {
    & gpg @gpgArgs $_.FullName
  }
}

Write-Host "Done. Artifacts in $dist"