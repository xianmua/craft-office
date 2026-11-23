# Manual web build pipeline for PdfCraft: same rationale as build-gridcraft-web.ps1
# (trunk's asset-copy stage hangs in this environment). Reproduces trunk by hand:
#   cargo build --release (run separately) -> wasm-bindgen -> wasm-opt
#   -> assemble dist/web (main app + OCR worker, hashed worker names + meta)
#   -> deploy to backend.
# Usage: 1) $env:CRAFT_I18N_DIR = "<repo>\.kernel-src\craft-i18n-main"
#           cargo build --release -p pdfcraft-web -p pdfcraft-ocr-worker --target wasm32-unknown-unknown
#        2) powershell -File scripts\build-pdfcraft-web.ps1
#        3) node scripts\inject-kernel-page.mjs
# NOTE: keep this file pure ASCII (PS 5.1 parses BOM-less files as ANSI/GBK).
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent

$wb = "$env:LOCALAPPDATA\trunkrs\trunk\cache\wasm-bindgen-0.2.129\wasm-bindgen.exe"
$wo = "$env:LOCALAPPDATA\trunkrs\trunk\cache\wasm-opt-version_131\bin\wasm-opt.exe"
$srcWasm = "$root\.kernel-src\pdfcraft-main\target\wasm32-unknown-unknown\release\pdfcraft-web.wasm"
$srcWorker = "$root\.kernel-src\pdfcraft-main\target\wasm32-unknown-unknown\release\pdfcraft-ocr-worker.wasm"
$dist = "$root\.kernel-src\pdfcraft-main\dist\web"
$deployTo = "$root\backend\static\pdfcraft"
$stage = "$env:TEMP\pc-stage"
$wstage = "$env:TEMP\pc-worker-stage"

Remove-Item $stage, $wstage -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory $stage, $wstage | Out-Null

Write-Output "[1/6] wasm-bindgen (app)"
& $wb --target web --out-dir $stage --out-name pdfcraft-web --weak-refs $srcWasm
if ($LASTEXITCODE -ne 0) { throw "wasm-bindgen failed $LASTEXITCODE" }

Write-Output "[2/6] wasm-bindgen (ocr worker)"
& $wb --target web --out-dir $wstage --out-name pdfcraft-ocr-worker --weak-refs $srcWorker
if ($LASTEXITCODE -ne 0) { throw "wasm-bindgen worker failed $LASTEXITCODE" }

Write-Output "[3/6] wasm-opt"
& $wo -Oz $stage\pdfcraft-web_bg.wasm -o $stage\pdfcraft-web_bg.opt.wasm
if ($LASTEXITCODE -ne 0) { throw "wasm-opt failed $LASTEXITCODE" }
Move-Item -Force $stage\pdfcraft-web_bg.opt.wasm $stage\pdfcraft-web_bg.wasm
& $wo -O3 $wstage\pdfcraft-ocr-worker_bg.wasm -o $wstage\pdfcraft-ocr-worker_bg.opt.wasm
if ($LASTEXITCODE -ne 0) { throw "wasm-opt worker failed $LASTEXITCODE" }
Move-Item -Force $wstage\pdfcraft-ocr-worker_bg.opt.wasm $wstage\pdfcraft-ocr-worker_bg.wasm

Write-Output "[4/6] assemble dist"
New-Item -ItemType Directory $dist -Force | Out-Null
Copy-Item "$stage\pdfcraft-web.js" "$dist\pdfcraft-web.js" -Force
Copy-Item "$stage\pdfcraft-web_bg.wasm" "$dist\pdfcraft-web_bg.wasm" -Force
Get-ChildItem $dist -File | Where-Object { $_.Name -match '^pdfcraft-web-.+\.js$' -or $_.Name -match '^pdfcraft-web-.+_bg\.wasm$' -or $_.Name -match '^pdfcraft-ocr-worker' } | Remove-Item -Force
Remove-Item "$dist\snippets" -Recurse -Force -ErrorAction SilentlyContinue
if (Test-Path "$stage\snippets") { robocopy "$stage\snippets" "$dist\snippets" /E /NFL /NDL /NJH /NJS | Out-Null }
if (Test-Path "$wstage\snippets") { robocopy "$wstage\snippets" "$dist\snippets" /E /NFL /NDL /NJH /NJS | Out-Null }
# worker glue + loader shim, plain names first (hashed below, same as cargo xtask web-worker-link)
Copy-Item "$wstage\pdfcraft-ocr-worker.js" "$dist\pdfcraft-ocr-worker.js" -Force
Copy-Item "$wstage\pdfcraft-ocr-worker_bg.wasm" "$dist\pdfcraft-ocr-worker_bg.wasm" -Force
[IO.File]::WriteAllText("$dist\pdfcraft-ocr-worker_loader.js", "import init from './pdfcraft-ocr-worker.js';await init();", (New-Object System.Text.UTF8Encoding($false)))

# hash the worker trio and rewrite the names inside them (xtask/src/web.rs logic)
$glue = [IO.File]::ReadAllBytes("$dist\pdfcraft-ocr-worker.js")
$wasm = [IO.File]::ReadAllBytes("$dist\pdfcraft-ocr-worker_bg.wasm")
$sha = [System.Security.Cryptography.SHA256]::Create()
$both = New-Object byte[] ($glue.Length + $wasm.Length)
[Array]::Copy($glue, 0, $both, 0, $glue.Length)
[Array]::Copy($wasm, 0, $both, $glue.Length, $wasm.Length)
$hash = ([BitConverter]::ToString($sha.ComputeHash($both)) -replace '-', '').ToLower().Substring(0, 16)
$wjs = "pdfcraft-ocr-worker-$hash.js"
$wwasm = "pdfcraft-ocr-worker-${hash}_bg.wasm"
$wloader = "pdfcraft-ocr-worker-${hash}_loader.js"
$glueText = [IO.File]::ReadAllText("$dist\pdfcraft-ocr-worker.js")
# PS 5.1 String.Split("str") splits by CHAR SET, count via regex full-string match
if (([regex]::Matches($glueText, [regex]::Escape("pdfcraft-ocr-worker_bg.wasm"))).Count -ne 1) { throw "worker glue does not name its wasm exactly once" }
[IO.File]::WriteAllText("$dist\$wjs", $glueText.Replace("pdfcraft-ocr-worker_bg.wasm", $wwasm), (New-Object System.Text.UTF8Encoding($false)))
Move-Item -Force "$dist\pdfcraft-ocr-worker_bg.wasm" "$dist\$wwasm"
$shim = [IO.File]::ReadAllText("$dist\pdfcraft-ocr-worker_loader.js")
[IO.File]::WriteAllText("$dist\$wloader", $shim.Replace("./pdfcraft-ocr-worker.js", "./$wjs"), (New-Object System.Text.UTF8Encoding($false)))
Remove-Item "$dist\pdfcraft-ocr-worker.js", "$dist\pdfcraft-ocr-worker_loader.js" -Force

Write-Output "[5/6] write index.html"
function Sri($p) {
  $hex = (Get-FileHash $p -Algorithm SHA384).Hash
  $bytes = [byte[]](0..47 | ForEach-Object { [Convert]::ToByte($hex.Substring($_ * 2, 2), 16) })
  "sha384-" + [Convert]::ToBase64String($bytes)
}
$jsSri = Sri "$dist\pdfcraft-web.js"
$wasmSri = Sri "$dist\pdfcraft-web_bg.wasm"
$html = @"
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1, user-scalable=no">
  <title>PdfCraft</title>
  <meta name="pdfcraft-ocr-worker" content="./$wloader" />
  <script type="module">
import init, * as bindings from './pdfcraft-web.js';
const wasm = await init({ module_or_path: './pdfcraft-web_bg.wasm' });

window.wasmBindings = bindings;

dispatchEvent(new CustomEvent("TrunkApplicationStarted", {detail: {wasm}}));

</script>
  <style>
    html, body { margin: 0; height: 100%; overflow: hidden; background: #f1f1f3; }
    @media (prefers-color-scheme: dark) { html, body { background: #19191c; } }
    #pdfcraft { width: 100%; height: 100%; display: block; outline: none; }
  </style>
<link rel="modulepreload" href="./pdfcraft-web.js" crossorigin="anonymous" integrity="$jsSri"><link rel="preload" href="./pdfcraft-web_bg.wasm" crossorigin="anonymous" integrity="$wasmSri" as="fetch" type="application/wasm"></head>
<body>
  <canvas id="pdfcraft"></canvas>
</body>
</html>
"@
[IO.File]::WriteAllText("$dist\index.html", $html, (New-Object System.Text.UTF8Encoding($false)))

Write-Output "[6/6] deploy"
robocopy $dist $deployTo /MIR /XF .trunk-lock /NFL /NDL /NJH /NJS | Out-Null
if ($LASTEXITCODE -gt 7) { throw "robocopy failed $LASTEXITCODE" }
Write-Output "DEPLOY_OK (robocopy exit $LASTEXITCODE, 0-7 = success)"
Get-ChildItem $deployTo -File | Select-Object Name, Length, LastWriteTime | Format-Table -AutoSize
