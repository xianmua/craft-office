# 启动后端（默认 0.0.0.0:8080）
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath (Join-Path $root 'backend')

$cargo = if (Get-Command cargo -ErrorAction SilentlyContinue) { 'cargo' } else { 'C:\Users\deren\.cargo\bin\cargo.exe' }
Write-Host "使用 cargo: $cargo" -ForegroundColor Cyan
& $cargo run
