# Manual web build pipeline: bypasses trunk's asset-copy stage, which hangs
# reproducibly in this environment (cargo finishes, trunk idles at ~0.3s CPU
# and never commits dist). Reproduces trunk's pipeline by hand:
#   cargo build --release (run separately) -> wasm-bindgen -> wasm-opt -Oz
#   -> assemble dist/web (stable names, recomputed SRI) -> deploy to backend.
# Usage: 1) cargo build --release -p wordcraft-web --target wasm32-unknown-unknown
#        2) powershell -File scripts\build-web-manual.ps1
# NOTE: keep this file pure ASCII (PS 5.1 parses BOM-less files as ANSI/GBK).
$ErrorActionPreference = "Stop"

$wb = "C:\Users\deren\AppData\Local\trunkrs\trunk\cache\wasm-bindgen-0.2.129\wasm-bindgen.exe"
$wo = "C:\Users\deren\AppData\Local\trunkrs\trunk\cache\wasm-opt-version_131\bin\wasm-opt.exe"
$srcWasm = "D:\Works\my\craft-office\.kernel-src\wordcraft-main\target\wasm32-unknown-unknown\release\wordcraft-web.wasm"
$dist = "D:\Works\my\craft-office\.kernel-src\wordcraft-main\dist\web"
$deployTo = "D:\Works\my\craft-office\backend\static\wordcraft"
$stage = "$env:TEMP\kc-stage"

Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory $stage | Out-Null

Write-Output "[1/5] wasm-bindgen"
& $wb --target web --out-dir $stage --out-name wordcraft-web --weak-refs $srcWasm
if ($LASTEXITCODE -ne 0) { throw "wasm-bindgen failed $LASTEXITCODE" }

Write-Output "[2/5] wasm-opt -Oz"
& $wo -Oz $stage\wordcraft-web_bg.wasm -o $stage\wordcraft-web_bg.opt.wasm
if ($LASTEXITCODE -ne 0) { throw "wasm-opt failed $LASTEXITCODE" }
Move-Item -Force $stage\wordcraft-web_bg.opt.wasm $stage\wordcraft-web_bg.wasm

Write-Output "[3/5] assemble dist"
New-Item -ItemType Directory $dist -Force | Out-Null
Copy-Item "$stage\wordcraft-web.js" "$dist\wordcraft-web.js" -Force
Copy-Item "$stage\wordcraft-web_bg.wasm" "$dist\wordcraft-web_bg.wasm" -Force
# remove stale trunk-hashed assets and old snippets, then copy fresh ones
Get-ChildItem $dist -File | Where-Object { $_.Name -match '^wordcraft-web-.+\.js$' -or $_.Name -match '^wordcraft-web-.+_bg\.wasm$' } | Remove-Item -Force
Remove-Item "$dist\snippets" -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item "$dist\.stage" -Recurse -Force -ErrorAction SilentlyContinue
if (Test-Path "$stage\snippets") { robocopy "$stage\snippets" "$dist\snippets" /E /NFL /NDL /NJH /NJS | Out-Null }

Write-Output "[4/5] write index.html"
function Sri($p) {
  $hex = (Get-FileHash $p -Algorithm SHA384).Hash
  $bytes = [byte[]](0..47 | ForEach-Object { [Convert]::ToByte($hex.Substring($_ * 2, 2), 16) })
  "sha384-" + [Convert]::ToBase64String($bytes)
}
$jsSri = Sri "$dist\wordcraft-web.js"
$wasmSri = Sri "$dist\wordcraft-web_bg.wasm"
$icoSri = Sri "$dist\wordcraft-fa59e09a3f921e03.ico"
$html = @"
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1, user-scalable=no">
  <title>WordCraft</title>
  <script>
    // Default UI language: zh-hans (craft-i18n built-in). Respect a user-chosen language.
    try {
      var KEY = 'wordcraft.ui';
      var prefs = {};
      try { prefs = JSON.parse(localStorage.getItem(KEY) || '{}') || {}; } catch (e) { prefs = {}; }
      if (!prefs.language || prefs.language === 'auto') {
        prefs.language = 'zh-hans';
        localStorage.setItem(KEY, JSON.stringify(prefs));
      }
    } catch (e) { /* keep auto if localStorage unavailable */ }
  </script>
  <link rel="icon" href="./wordcraft-fa59e09a3f921e03.ico" integrity="$icoSri"/>
  <script type="module">
import init, * as bindings from './wordcraft-web.js';
const wasm = await init({ module_or_path: './wordcraft-web_bg.wasm' });

window.wasmBindings = bindings;

dispatchEvent(new CustomEvent("TrunkApplicationStarted", {detail: {wasm}}));

</script>
  <style>
    html, body { margin: 0; padding: 0; width: 100%; height: 100%; overflow: hidden; background: #e6e6e6; }
    #wordcraft_canvas { position: absolute; inset: 0; width: 100%; height: 100%; display: block; }
    #wordcraft_loading { position: absolute; inset: 0; display: flex; flex-direction: column; align-items: center; justify-content: center;
      color: #555; font: 14px system-ui, sans-serif; pointer-events: none; }
  </style>
<link rel="modulepreload" href="./wordcraft-web.js" crossorigin="anonymous" integrity="$jsSri"><link rel="preload" href="./wordcraft-web_bg.wasm" crossorigin="anonymous" integrity="$wasmSri" as="fetch" type="application/wasm"></head>
<body>
  <canvas id="wordcraft_canvas"></canvas>
  <div id="wordcraft_loading">Loading WordCraft&hellip;</div>
</body>
</html>
"@
[IO.File]::WriteAllText("$dist\index.html", $html, (New-Object System.Text.UTF8Encoding($false)))

Write-Output "[5/5] deploy"
Remove-Item "$deployTo\.stage" -Recurse -Force -ErrorAction SilentlyContinue
robocopy $dist $deployTo /MIR /XF .trunk-lock /NFL /NDL /NJH /NJS | Out-Null
if ($LASTEXITCODE -gt 7) { throw "robocopy failed $LASTEXITCODE" }
Write-Output "DEPLOY_OK (robocopy exit $LASTEXITCODE, 0-7 = success)"
Get-ChildItem $deployTo -File | Select-Object Name, Length, LastWriteTime | Format-Table -AutoSize
