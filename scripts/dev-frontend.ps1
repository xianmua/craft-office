# 启动前端 dev server（5173，/api 与 /artcraft 代理到后端）
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath (Join-Path $root 'frontend')

if (-not (Test-Path -LiteralPath 'node_modules')) {
    Write-Host '首次运行，安装依赖…' -ForegroundColor Cyan
    npm install
}
npm run dev
