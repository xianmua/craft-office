# trim-iterate.ps1 — 分组验证可安全删除项（程序组），使用默认 profile
$ErrorActionPreference = 'Continue'
$root = Split-Path -Parent $PSScriptRoot
$full = Join-Path $root 'vendor\libreoffice\26.8.3'
$t    = Join-Path $root 'vendor\libreoffice\26.8.3-trimmed'
$samples = Join-Path $env:TEMP 'lo-samples'
$out = Join-Path $env:TEMP 'lo-it-out'
New-Item -ItemType Directory -Force -Path $out | Out-Null

function Kill-LO { Get-Process | Where-Object { $_.ProcessName -match 'soffice' } | Stop-Process -Force -ErrorAction SilentlyContinue; Start-Sleep -Milliseconds 500 }
function Test-Convert {
    param([string]$src, [string]$to)
    Kill-LO
    $o = Join-Path $out ([IO.Path]::GetFileNameWithoutExtension($src) + ".$to")
    if (Test-Path $o) { Remove-Item $o -Force }
    $args = @('--headless', '--norestore', '--convert-to', $to, '--outdir', $out, $src)
    $t0 = Get-Date
    & "$t\program\soffice.com" @args *> "$env:TEMP\lo-it.log" 2>&1
    $el = ((Get-Date) - $t0).TotalSeconds
    if ($el -gt 90) { Kill-LO; return $false }
    return ($LASTEXITCODE -eq 0 -and (Test-Path $o) -and (Get-Item $o).Length -gt 0)
}
function Drop($rel) { $p = Join-Path $t ($rel -replace '/', '\'); if (Test-Path $p) { Remove-Item $p -Recurse -Force -ErrorAction SilentlyContinue } }
function Restore($rel) {
    $s = Join-Path $full ($rel -replace '/', '\'); $d = Join-Path $t ($rel -replace '/', '\')
    if (Test-Path $s -PathType Container) { robocopy $s $d /E /NFL /NDL /NJH /NJS /NP | Out-Null }
    elseif (Test-Path $s) { $di = Split-Path $d -Parent; New-Item -ItemType Directory -Force -Path $di | Out-Null; Copy-Item $s $d -Force }
}

Write-Host '== 基线（全 program + 全 share） =='
$ok = Test-Convert "$samples\demo.doc" 'docx'
Write-Host ("基线: " + $(if($ok){'PASS'}else{'FAIL'}))

$groups = @{
  A_gui   = @('program/senddoc.exe','program/quickstart.exe','program/soffice_safe.exe','program/spsupp_helper.exe','program/spsupp_x64.dll','program/spsupp_x86.dll','program/minidump_upload.exe','program/regview.exe','program/updater.exe','program/update_service.exe','program/updchklo.dll','program/updatecheckuilo.dll','program/updater.ini','program/UAccCOM.dll','program/winaccessibility.dll','program/deploymentgui.dll','program/mscx_uno.dll','program/odbcconfig.exe','program/gengal.exe','program/mar.exe','program/uno.exe','program/unoinfo.exe','program/sbase.exe','program/scalc.exe','program/sdraw.exe','program/simpress.exe','program/smath.exe','program/sweb.exe','program/swriter.exe','program/twain32shim.exe')
  B_python = @('program/python313.dll','program/python3.dll','program/pyuno.pyd','program/pythonloaderlo.dll','program/python-core-3.13.18','program/__pycache__','program/access2base.py','program/mailmerge.py','program/msgbox.py','program/officehelper.py','program/uno.py','program/unohelper.py','program/pythonscript.py','program/pythonloader.py','program/scriptforge.py','program/scriptforge.pyi','program/pythonloader.uno')
  C_dbjava = @('program/hsqldb.dll','program/dbplo.dll','program/Engine12.dll','program/libpq.dll','program/postgresql-sdbc','program/jdbclo.dll','program/jvmfwk3.ini','program/jvmfwklo.dll','program/java_uno.dll','program/javaloaderlo.dll','program/juh.dll','program/juhx.dll','program/jvmaccesslo.dll','program/jpipe.dll','program/jpipx.dll','program/JREProperties.class')
  D_format = @('program/CoinMP.dll','program/lpsolve55.dll','program/pdfiumlo.dll','program/pdfimportlo.dll','program/xpdfimport.exe','program/etonyek.dll','program/mwaw.dll','program/wpd.dll','program/wpg.dll','program/wps.dll','program/orcus.dll','program/orcus-parser.dll','program/staroffice.dll','program/migrationoo2lo.dll','program/migrationoo3lo.dll')
  E_crypto = @('program/libcrypto-3.dll','program/libssl-3.dll','program/nss3.dll','program/nspr4.dll','program/plc4.dll','program/plds4.dll','program/ssl3.dll','program/smime3.dll','program/softokn3.dll','program/freebl3.dll','program/nssckbi.dll','program/nssdbm3.dll','program/nssutil3.dll','program/gpgmepp.dll','program/gpgme-w32spawn.exe','program/libxmlsec-mscng.dll','program/libxmlsec.dll','program/libxslt.dll','program/libxml2.dll','program/librdf.dll','program/raptor2.dll','program/rasqal.dll','program/cli_basetypes.dll','program/cli_cppuhelper.dll','program/cli_oootypes.dll','program/cli_uno.dll','program/cli_ure.dll','program/cli_uretypes.dll','program/vbaobjlo.dll','program/vbaswobjlo.dll')
  F_gpu    = @('program/opencl','program/opengl','program/clewlo.dll')
  G_merged = @('program/mergedlo.dll')
}
$result = @{}
foreach ($g in $groups.Keys) {
    foreach ($r in $groups[$g]) { Drop $r }
    $ok = Test-Convert "$samples\demo.doc" 'docx'
    if ($ok) { $result[$g] = 'PASS' }
    else {
        $result[$g] = 'FAIL'
        foreach ($r in $groups[$g]) { Restore $r }
        Test-Convert "$samples\demo.doc" 'docx' | Out-Null
    }
    Write-Host ("{0,-10} {1}" -f $g, $result[$g])
}
$f = Get-ChildItem $t -Recurse -File; $s = ($f | Measure-Object Length -Sum).Sum
Write-Host ("== 最终: {0} 文件, {1:N1} MB ==" -f $f.Count, ($s / 1MB))
$result.GetEnumerator() | ForEach-Object { "{0} = {1}" -f $_.Key, $_.Value }
