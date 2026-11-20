# Publish: gather a deployable bundle (backend exe + static + plugins + samples
# + config templates) into a zip. Keep this file pure ASCII (PS 5.1 parses
# BOM-less files as ANSI/GBK).
# Usage: powershell -File scripts\publish.ps1 [-OutDir dist] [-SkipBuild]
param(
  [string]$OutDir = "$PSScriptRoot\..\dist",
  [switch]$SkipBuild
)
$ErrorActionPreference = "Stop"
$root = (Resolve-Path "$PSScriptRoot\..").Path

# 1) backend release exe
if (-not $SkipBuild) {
  Push-Location "$root\backend"
  cargo build --release
  if ($LASTEXITCODE -ne 0) { Pop-Location; throw "cargo build --release failed" }
  Pop-Location
}
$exe = "$root\backend\target\release\artcraft-office.exe"
if (-not (Test-Path $exe)) { throw "artcraft-office.exe not found (run without -SkipBuild)" }

# 2) frontend + kernel artifacts must already be deployed
if (-not (Test-Path "$root\backend\static\index.html")) {
  throw "backend\static\index.html missing; first run: cd frontend && npm run build (+ kernel deploy scripts)"
}

# 3) stage the bundle (runtime paths are cwd-relative: ./static ./samples ./assets)
$stage = "$env:TEMP\artcraft-publish"
Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory "$stage\artcraft-office" | Out-Null
Copy-Item $exe "$stage\artcraft-office\"
robocopy "$root\backend\static" "$stage\artcraft-office\static" /E /XF .trunk-lock /NFL /NDL /NJH /NJS | Out-Null
if ($LASTEXITCODE -gt 7) { throw "robocopy static failed $LASTEXITCODE" }
Copy-Item "$root\assets" "$stage\artcraft-office\assets" -Recurse
# legacy converter: bundle the Node sidecar (fast path) and vendor/libreoffice
# (fallback) when present; the exe auto-detects both at runtime
if (Test-Path "$root\vendor\convert-svc") {
  robocopy "$root\vendor\convert-svc" "$stage\artcraft-office\vendor\convert-svc" /E /NFL /NDL /NJH /NJS | Out-Null
  if ($LASTEXITCODE -gt 7) { throw "robocopy convert-svc failed $LASTEXITCODE" }
  Write-Output "BUNDLED: vendor/convert-svc (sidecar conversion engine)"
} else {
  Write-Output "SKIPPED: vendor/convert-svc absent, falling back to LibreOffice only"
}
if (Test-Path "$root\vendor\libreoffice") {
  robocopy "$root\vendor\libreoffice" "$stage\artcraft-office\vendor\libreoffice" /E /NFL /NDL /NJH /NJS | Out-Null
  if ($LASTEXITCODE -gt 7) { throw "robocopy libreoffice failed $LASTEXITCODE" }
  Write-Output "BUNDLED: vendor/libreoffice (legacy doc/xls/ppt conversion enabled)"
} else {
  Write-Output "SKIPPED: vendor/libreoffice absent, legacy formats stay unsupported in the bundle"
}
if (Test-Path "$root\backend\samples") { Copy-Item "$root\backend\samples" "$stage\artcraft-office\samples" -Recurse }
Copy-Item "$root\backend\.env.example" "$stage\artcraft-office\.env.example"
# config template: the bundle runs the exe at the root, so dev_dir ../assets/plugins -> ./assets/plugins
$cfg = Get-Content "$root\backend\config.toml.example" -Raw -Encoding UTF8
$cfg = $cfg -replace 'dev_dir = "\.\./assets/plugins"', 'dev_dir = "./assets/plugins"'
[IO.File]::WriteAllText("$stage\artcraft-office\config.toml.example", $cfg, (New-Object System.Text.UTF8Encoding($false)))

# 4) zip
New-Item -ItemType Directory $OutDir -Force | Out-Null
$zip = Join-Path $OutDir ("artcraft-office-{0}.zip" -f (Get-Date -Format "yyyyMMdd-HHmm"))
Compress-Archive -Path "$stage\artcraft-office" -DestinationPath $zip -Force
Remove-Item $stage -Recurse -Force
Write-Output ("PUBLISHED: {0} ({1} MB)" -f $zip, [Math]::Round((Get-Item $zip).Length / 1MB, 1))
