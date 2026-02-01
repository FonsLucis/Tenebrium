param(
  [string]$Version = "0.1.0",
  [string]$Message = "Release"
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root

$tag = "v$Version"
Write-Host "Creating git tag $tag..."

git tag -a $tag -m "$Message $tag"

Write-Host "Tag created: $tag"
Pop-Location
