# Manual web build pipeline for GridCraft: same rationale as build-web-manual.ps1
# (trunk's asset-copy stage hangs in this environment). Reproduces trunk by hand:
#   cargo build --release (run separately) -> wasm-bindgen -> wasm-opt -Oz
#   -> assemble dist/web (stable names, recomputed SRI) -> deploy to backend.
# Usage: 1) cargo build --release -p gridcraft-web --target wasm32-unknown-unknown
#        2) powershell -File scripts\build-gridcraft-web.ps1
# NOTE: keep this file pure ASCII (PS 5.1 parses BOM-less files as ANSI/GBK).
$ErrorActionPreference = "Stop"

$wb = "$env:LOCALAPPDATA\trunkrs\trunk\cache\wasm-bindgen-0.2.129\wasm-bindgen.exe"
$wo = "$env:LOCALAPPDATA\trunkrs\trunk\cache\wasm-opt-version_131\bin\wasm-opt.exe"
$srcWasm = "D:\Works\my\craft-office\.kernel-src\gridcraft-main\target\wasm32-unknown-unknown\release\gridcraft-web.wasm"
$dist = "D:\Works\my\craft-office\.kernel-src\gridcraft-main\dist\web"
$deployTo = "D:\Works\my\craft-office\backend\static\gridcraft"
$stage = "$env:TEMP\gc-stage"

Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory $stage | Out-Null

Write-Output "[1/5] wasm-bindgen"
& $wb --target web --out-dir $stage --out-name gridcraft-web --weak-refs $srcWasm
if ($LASTEXITCODE -ne 0) { throw "wasm-bindgen failed $LASTEXITCODE" }

Write-Output "[2/5] wasm-opt -Oz"
& $wo -Oz $stage\gridcraft-web_bg.wasm -o $stage\gridcraft-web_bg.opt.wasm
if ($LASTEXITCODE -ne 0) { throw "wasm-opt failed $LASTEXITCODE" }
Move-Item -Force $stage\gridcraft-web_bg.opt.wasm $stage\gridcraft-web_bg.wasm

Write-Output "[3/5] assemble dist"
New-Item -ItemType Directory $dist -Force | Out-Null
Copy-Item "$stage\gridcraft-web.js" "$dist\gridcraft-web.js" -Force
Copy-Item "$stage\gridcraft-web_bg.wasm" "$dist\gridcraft-web_bg.wasm" -Force
Get-ChildItem $dist -File | Where-Object { $_.Name -match '^gridcraft-web-.+\.js$' -or $_.Name -match '^gridcraft-web-.+_bg\.wasm$' } | Remove-Item -Force
Remove-Item "$dist\snippets" -Recurse -Force -ErrorAction SilentlyContinue
if (Test-Path "$stage\snippets") { robocopy "$stage\snippets" "$dist\snippets" /E /NFL /NDL /NJH /NJS | Out-Null }
# app icon: stable copy from the kernel repo assets
Copy-Item "D:\Works\my\craft-office\.kernel-src\gridcraft-main\assets\app-icon\gridcraft.ico" "$dist\gridcraft.ico" -Force

Write-Output "[4/5] write index.html"
function Sri($p) {
  $hex = (Get-FileHash $p -Algorithm SHA384).Hash
  $bytes = [byte[]](0..47 | ForEach-Object { [Convert]::ToByte($hex.Substring($_ * 2, 2), 16) })
  "sha384-" + [Convert]::ToBase64String($bytes)
}
$jsSri = Sri "$dist\gridcraft-web.js"
$wasmSri = Sri "$dist\gridcraft-web_bg.wasm"
$icoSri = Sri "$dist\gridcraft.ico"
$html = @"
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1, user-scalable=no">
  <title>GridCraft</title>
  <link rel="icon" href="./gridcraft.ico" integrity="$icoSri"/>
  <script type="module">
import init, * as bindings from './gridcraft-web.js';
const wasm = await init({ module_or_path: './gridcraft-web_bg.wasm' });

window.wasmBindings = bindings;

dispatchEvent(new CustomEvent("TrunkApplicationStarted", {detail: {wasm}}));

</script>
  <style>
    html, body { margin: 0; padding: 0; width: 100%; height: 100%; overflow: hidden; background: #262626; }
    #gridcraft_canvas { position: absolute; inset: 0; width: 100%; height: 100%; display: block; }
    #gridcraft_loading { position: absolute; inset: 0; display: flex; flex-direction: column; align-items: center; justify-content: center;
      color: #b8b8b8; font: 14px system-ui, sans-serif; pointer-events: none; }
  </style>
<link rel="modulepreload" href="./gridcraft-web.js" crossorigin="anonymous" integrity="$jsSri"><link rel="preload" href="./gridcraft-web_bg.wasm" crossorigin="anonymous" integrity="$wasmSri" as="fetch" type="application/wasm"></head>
<body>
  <canvas id="gridcraft_canvas"></canvas>
  <div id="gridcraft_loading">Loading GridCraft&hellip;</div>
</body>
</html>
"@
[IO.File]::WriteAllText("$dist\index.html", $html, (New-Object System.Text.UTF8Encoding($false)))

Write-Output "[5/5] deploy"
robocopy $dist $deployTo /MIR /XF .trunk-lock /NFL /NDL /NJH /NJS | Out-Null
if ($LASTEXITCODE -gt 7) { throw "robocopy failed $LASTEXITCODE" }
Write-Output "DEPLOY_OK (robocopy exit $LASTEXITCODE, 0-7 = success)"
Get-ChildItem $deployTo -File | Select-Object Name, Length, LastWriteTime | Format-Table -AutoSize
