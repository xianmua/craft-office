# 全量构建：前端 → backend/static，后端 → release 二进制
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

Write-Host '[1/2] 构建前端…' -ForegroundColor Cyan
Set-Location -LiteralPath (Join-Path $root 'frontend')
npm ci
npm run build

Write-Host '[2/2] 构建后端…' -ForegroundColor Cyan
Set-Location -LiteralPath (Join-Path $root 'backend')
# 优先用 rustup 的 cargo；mise 的 shim 可能失效
$cargoPath = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
$cargo = if (Test-Path $cargoPath) { $cargoPath } elseif (Get-Command cargo -ErrorAction SilentlyContinue) { 'cargo' } else { $cargoPath }
& $cargo build --release

Write-Host "完成：$root\backend\target\release\artcraft-office.exe" -ForegroundColor Green
