# 启动后端（默认 0.0.0.0:8080）
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath (Join-Path $root 'backend')

# 优先用 rustup 的 cargo；mise 的 shim 可能失效
$cargoPath = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
$cargo = if (Test-Path $cargoPath) { $cargoPath } elseif (Get-Command cargo -ErrorAction SilentlyContinue) { 'cargo' } else { $cargoPath }
Write-Host "使用 cargo: $cargo" -ForegroundColor Cyan
& $cargo run
