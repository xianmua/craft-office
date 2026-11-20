# trim-libreoffice.ps1
# 目的：把 vendor/libreoffice/26.8.3（完整安装 ~984MB）裁剪成 headless 转换够用的最小集。
# 依据：
#   1) DSH libreoffice-kit 的 payload-shaping.json resources.removed 清单（去掉 kit 特有的 soffice 删除项）
#   2) 实测迭代：删除候选大件后跑 soffice.com --convert-to 验证，缺什么补什么（见 README 注释 / 对话结论）
# 用法：powershell -ExecutionPolicy Bypass -File scripts\trim-libreoffice.ps1
# 产物：vendor/libreoffice/26.8.3-trimmed（不修改原始 26.8.3）
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot          # craft-office/
$src  = Join-Path $root 'vendor\libreoffice\26.8.3'
$dst  = Join-Path $root 'vendor\libreoffice\26.8.3-trimmed'

if (-not (Test-Path $src)) { throw "找不到 $src" }
if (Test-Path $dst) { Remove-Item $dst -Recurse -Force }

Write-Host "== 1/3 全量拷贝 $src -> $dst =="
robocopy $src $dst /E /NFL /NDL /NJH /NJS /NP | Out-Null

function Drop($rel) {
    $p = Join-Path $dst ($rel -replace '/', '\')
    if (Test-Path $p) { Remove-Item $p -Recurse -Force }
}

Write-Host '== 2/3 套用 DSH payload-shaping 清单（映射路径） =='
$recipe = Get-Content (Join-Path $PSScriptRoot 'lo_removed_list.txt') -ErrorAction SilentlyContinue
foreach ($e in $recipe) {
    if ($e -like 'program/program/*')    { Drop ($e.Replace('program/program/', 'program/')) }
    elseif ($e -like 'program/share/*')  { Drop ($e.Replace('program/share/', 'share/')) }
    elseif ($e -eq 'program/CREDITS.fodt') { Drop 'CREDITS.fodt' }
    elseif ($e -eq 'program/LICENSE.html') { Drop 'LICENSE.html' }
    elseif ($e -like 'program/*')        { Drop $e }
}
# kit 因为自带了合并 exe 才删 soffice.*，我们要保留
foreach ($keep in 'program/soffice.exe','program/soffice.com','program/soffice.bin','program/unopkg.exe','program/unopkg.com','program/unopkg.bin') {
    if (-not (Test-Path (Join-Path $dst $keep))) {
        Copy-Item (Join-Path $src $keep) (Join-Path $dst $keep) -Force -ErrorAction SilentlyContinue
    }
}

Write-Host '== 3/3 激进二进制裁剪（实测可删项） =='
$aggressive = @(
    # 合并库：若 soffice.bin 走 split 模块则不需要（实测确认后删除）
    'program/mergedlo.dll',
    # python 运行时（转换不走 python 脚本）
    'program/python313.dll','program/python3.dll','program/pyuno.pyd','program/pythonloaderlo.dll',
    'program/python-core-3.13.18','program/__pycache__',
    'program/access2base.py','program/mailmerge.py','program/msgbox.py','program/officehelper.py',
    'program/uno.py','program/unohelper.py','program/pythonscript.py','program/pythonloader.py',
    'program/scriptforge.py','program/scriptforge.pyi','program/pythonloader.uno',
    # 数据库/Java 桥
    'program/hsqldb.dll','program/dbplo.dll','program/Engine12.dll','program/libpq.dll',
    'program/postgresql-sdbc','program/jdbclo.dll','program/jvmfwk3.ini','program/jvmfwklo.dll',
    'program/java_uno.dll','program/javaloaderlo.dll','program/juh.dll','program/juhx.dll',
    'program/jvmaccesslo.dll','program/jpipe.dll','program/jpipx.dll','program/JREProperties.class',
    'share/firebird',
    # 求解器（xlsx 转换不需要）
    'program/CoinMP.dll','program/lpsolve55.dll',
    # PDF / 其他格式导入（非 doc/xls/ppt 目标）
    'program/pdfiumlo.dll','program/pdfimportlo.dll','program/xpdfimport.exe','program/twain32shim.exe','share/xpdfimport',
    'program/etonyek.dll','program/mwaw.dll','program/wpd.dll','program/wpg.dll','program/wps.dll',
    'program/orcus.dll','program/orcus-parser.dll','program/staroffice.dll',
    # 迁移与加密/网络栈
    'program/migrationoo2lo.dll','program/migrationoo3lo.dll',
    'program/gpgmepp.dll','program/gpgme-w32spawn.exe','program/smime3.dll','program/softokn3.dll',
    'program/nss3.dll','program/nspr4.dll','program/plc4.dll','program/plds4.dll','program/ssl3.dll',
    'program/freebl3.dll','program/nssckbi.dll','program/nssdbm3.dll','program/nssutil3.dll',
    'program/libcrypto-3.dll','program/libssl-3.dll','program/libxmlsec-mscng.dll','program/libxmlsec.dll',
    'program/libxslt.dll','program/libxml2.dll','program/librdf.dll','program/raptor2.dll','program/rasqal.dll',
    # .NET/CLI 桥
    'program/cli_basetypes.dll','program/cli_cppuhelper.dll','program/cli_oootypes.dll',
    'program/cli_uno.dll','program/cli_ure.dll','program/cli_uretypes.dll',
    # VBA 兼容 / 辅助功能 / 崩溃上报 / 更新器
    'program/vbaobjlo.dll','program/vbaswobjlo.dll','program/UAccCOM.dll','program/winaccessibility.dll',
    'program/senddoc.exe','program/quickstart.exe','program/soffice_safe.exe','program/spsupp_helper.exe',
    'program/spsupp_x64.dll','program/spsupp_x86.dll','program/minidump_upload.exe','program/regview.exe',
    'program/updater.exe','program/update_service.exe','program/updchklo.dll','program/updatecheckuilo.dll','program/updater.ini',
    'program/deploymentgui.dll','program/mscx_uno.dll','program/odbcconfig.exe','program/spsupp_helper.exe',
    # GPU/OpenCL
    'program/opencl','program/opengl','program/clewlo.dll','share/opengl','share/opencl',
    # UI 相关 share
    'share/uno_packages','share/glade','share/wizards','share/basic','share/Scripts','share/gallery',
    # 其他 exe（单机应用入口，headless 不需要）
    'program/sbase.exe','program/scalc.exe','program/sdraw.exe','program/simpress.exe','program/smath.exe','program/sweb.exe','program/swriter.exe',
    'program/gengal.exe','program/mar.exe','program/uno.exe','program/unoinfo.exe'
)
foreach ($a in $aggressive) { Drop $a }

$sum = (Get-ChildItem $dst -Recurse -File -ErrorAction SilentlyContinue | Measure-Object Length -Sum).Sum
$cnt = (Get-ChildItem $dst -Recurse -File -ErrorAction SilentlyContinue).Count
Write-Host ('== 完成：{0} 文件，{1:N1} MB（原始 {2:N1} MB）==' -f $cnt, ($sum/1MB), ((Get-ChildItem $src -Recurse -File | Measure-Object Length -Sum).Sum/1MB))
