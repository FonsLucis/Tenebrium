param(
  [string]$Version = "0.1.0",
  [string]$OutFile = "docs/release/release-notes.md",
  [string]$Since = ""
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root

$tag = "v$Version"
$range = if ($Since -ne "") { "$Since..HEAD" } else { "HEAD" }

$notes = @()
$notes += "# Release Notes"
$notes += ""
$notes += "## 버전"
$notes += "- $tag"
$notes += ""
$notes += "## 변경 사항"
$notes += ""

$commits = git log $range --pretty=format:"- %s (%h)"
if ($commits) {
  $notes += $commits
} else {
  $notes += "- (no commits)"
}

$notes += ""
$notes += "## 마이그레이션"
$notes += "- 없음"
$notes += ""
$notes += "## 알려진 이슈"
$notes += "- 없음"

$notes | Set-Content $OutFile
Write-Host "Release notes written to $OutFile"

Pop-Location
