# ArtCraft Office 接口回归测试脚本
# 用法: powershell -File scripts/verify.ps1
# 输出: 控制台每行 [PASS]/[FAIL]，并写 verify-results.json
param(
  [string]$Base = "http://127.0.0.1:8080"
)

$ErrorActionPreference = 'Continue'
$script:pass = 0
$script:fail = 0
$script:results = @()

function Case([string]$id, [string]$name, [scriptblock]$check) {
  try {
    & $check
    $script:pass++
    $script:results += [pscustomobject]@{ id = $id; name = $name; result = 'PASS'; detail = '' }
    Write-Host "[PASS] $id $name"
  } catch {
    $script:fail++
    $msg = $_.Exception.Message
    $script:results += [pscustomobject]@{ id = $id; name = $name; result = 'FAIL'; detail = $msg }
    Write-Host "[FAIL] $id $name -- $msg"
  }
}

function Should([bool]$cond, [string]$msg) { if (-not $cond) { throw $msg } }

function Req([string]$path) {
  $url = $Base + $path
  try {
    return Invoke-WebRequest -Uri $url -TimeoutSec 60 -UseBasicParsing
  } catch {
    $resp = $_.Exception.Response
    if ($null -eq $resp) { throw "请求异常: $($_.Exception.Message)" }
    # Windows PowerShell 5.1：非 2xx 抛异常，需从异常里取状态码与响应体
    $status = [int]$resp.StatusCode
    # PS5.1 在 HtmlWebResponseException 上直接给了 ErrorDetails.Message；stream 兜底
    $body = $_.ErrorDetails.Message
    if (-not $body) {
      try { $body = (New-Object IO.StreamReader($resp.GetResponseStream())).ReadToEnd() } catch { $body = '' }
    }
    $headers = @{}
    try { foreach ($h in $resp.Headers.AllKeys) { $headers[$h] = $resp.Headers[$h] } } catch {}
    return [pscustomobject]@{ StatusCode = $status; Content = $body; Headers = $headers }
  }
}

$demo = 'https://calibre-ebook.com/downloads/demos/demo.docx'
$demoEnc = [uri]::EscapeDataString($demo)

# ---- A. 健康检查与静态页面 ----
Case 'A01' 'health 200' {
  $r = Req '/api/health'; Should ($r.StatusCode -eq 200) "got $($r.StatusCode)"
  $j = $r.Content | ConvertFrom-Json
  Should $j.ok 'ok 字段不为 true'
}
Case 'A02' 'health 含关键字段' {
  $j = (Req '/api/health').Content | ConvertFrom-Json
  Should ($j.service -eq 'artcraft-office') 'service 字段缺失'
  Should ($j.max_bytes -gt 0) 'max_bytes 缺失'
  Should ($j.allow_hosts.Count -ge 2) 'allow_hosts 过少'
}
Case 'A03' 'GET / 返回 SPA' { $r = Req '/'; Should ($r.StatusCode -eq 200) ''; Should ($r.Content -match 'id="app"') '无 #app 挂载点' }
Case 'A04' 'GET /office 返回 SPA' { $r = Req '/office'; Should ($r.StatusCode -eq 200) ''; Should ($r.Content -match 'id="app"') '' }
Case 'A05' 'SPA fallback 到任意非 api 路径' { $r = Req '/some/deep/route'; Should ($r.StatusCode -eq 200) ''; Should ($r.Content -match 'id="app"') '' }
Case 'A06' '未知 /api/* 返回 JSON 404' { $r = Req '/api/nope'; Should ($r.StatusCode -eq 404) "got $($r.StatusCode)" }
Case 'A07' '现有静态资源可达' { $asset = ((Req '/office').Content | Select-String -Pattern 'assets/index-[^"]+\.js').Matches.Value; Should ($asset.Length -gt 0) '未找到 JS 资产引用'; if (-not $asset.StartsWith('/')) { $asset = '/' + $asset }; $r = Req $asset; Should ($r.StatusCode -eq 200) "JS 资产 $asset 404" }
Case 'A08' '缺失资产走 SPA 兜底（200 HTML，非 JS）' { $r = Req '/assets/__nope__.js'; Should ($r.StatusCode -eq 200) "got $($r.StatusCode)"; Should ($r.Headers['Content-Type'] -match 'html') "ct=$($r.Headers['Content-Type'])" }
Case 'A09' '/artcraft/ 缺失文件 404' { $r = Req '/artcraft/nope.js'; Should ($r.StatusCode -eq 404) "got $($r.StatusCode)" }
Case 'A10' '/artcraft/ 路径穿越被拒' { $r = Req '/artcraft/..%2FCargo.toml'; Should ($r.StatusCode -ge 400) "got $($r.StatusCode)" }

# ---- B. meta 元信息 ----
Case 'B01' 'meta 白名单文件 200' {
  $r = Req "/api/doc/meta?url=$demoEnc"; Should ($r.StatusCode -eq 200) "got $($r.StatusCode): $($r.Content)"
  $j = $r.Content | ConvertFrom-Json
  Should ($j.ext -eq 'docx') "ext=$($j.ext)"
  Should ($j.supported -eq $true) 'supported=false'
  Should ($j.size -eq 1311881) "size=$($j.size)"
}
Case 'B02' 'meta 缺 url 参数 4xx' { $r = Req '/api/doc/meta'; Should ($r.StatusCode -ge 400) "got $($r.StatusCode)" }
Case 'B03' 'meta 空 url 400' { $r = Req '/api/doc/meta?url='; Should ($r.StatusCode -eq 400) "got $($r.StatusCode)" }
Case 'B04' 'meta 非法 url 400 invalid_target' {
  $r = Req ('/api/doc/meta?url=' + [uri]::EscapeDataString('not a url')); Should ($r.StatusCode -eq 400) "got $($r.StatusCode)"
  Should (($r.Content | ConvertFrom-Json).code -eq 'invalid_target') 'code 不对'
}
Case 'B05' 'meta http 协议被拒' {
  $r = Req ('/api/doc/meta?url=' + [uri]::EscapeDataString('http://calibre-ebook.com/downloads/demos/demo.docx'))
  Should ($r.StatusCode -eq 400) "got $($r.StatusCode)"
}
Case 'B06' 'meta 非白名单域名被拒' {
  $r = Req ('/api/doc/meta?url=' + [uri]::EscapeDataString('https://evil.example.com/a.docx'))
  Should ($r.StatusCode -eq 400) "got $($r.StatusCode)"
}
Case 'B07' 'meta 非标端口被拒' {
  $r = Req ('/api/doc/meta?url=' + [uri]::EscapeDataString('https://calibre-ebook.com:8443/downloads/demos/demo.docx'))
  Should ($r.StatusCode -eq 400) "got $($r.StatusCode)"
}
Case 'B08' 'meta 路径越界被拒' {
  $r = Req ('/api/doc/meta?url=' + [uri]::EscapeDataString('https://calibre-ebook.com/private/a.docx'))
  Should ($r.StatusCode -eq 400) "got $($r.StatusCode)"
}
Case 'B09' 'meta userinfo 被拒' {
  $r = Req ('/api/doc/meta?url=' + [uri]::EscapeDataString('https://user:pass@calibre-ebook.com/downloads/demos/demo.docx'))
  Should ($r.StatusCode -eq 400) "got $($r.StatusCode)"
}
Case 'B10' 'meta 上游 404 报错' {
  $r = Req ('/api/doc/meta?url=' + [uri]::EscapeDataString('https://calibre-ebook.com/downloads/demos/__nope__.docx'))
  Should ($r.StatusCode -ge 500) "got $($r.StatusCode)"
}

# ---- C. 流式代理 ----
$proxyFile = Join-Path $env:TEMP 'verify-proxy.docx'
Case 'C01' 'proxy 完整下载且字节数正确' {
  $r = Req "/api/doc/proxy?url=$demoEnc"
  Should ($r.StatusCode -eq 200) "got $($r.StatusCode)"
  [IO.File]::WriteAllBytes($proxyFile, $r.Content)
  Should ((Get-Item $proxyFile).Length -eq 1311881) "size=$((Get-Item $proxyFile).Length)"
}
Case 'C02' 'proxy Content-Type 正确' {
  $r = Req "/api/doc/proxy?url=$demoEnc"
  Should ($r.Headers['Content-Type'] -match 'wordprocessingml') "ct=$($r.Headers['Content-Type'])"
}
Case 'C03' 'proxy inline disposition' {
  $r = Req "/api/doc/proxy?url=$demoEnc"
  Should ($r.Headers['Content-Disposition'] -match 'inline; filename="demo.docx"') "cd=$($r.Headers['Content-Disposition'])"
}
Case 'C04' 'proxy download=true 改 attachment' {
  $r = Req "/api/doc/proxy?url=$demoEnc&download=true"
  Should ($r.Headers['Content-Disposition'] -match 'attachment') "cd=$($r.Headers['Content-Disposition'])"
}
Case 'C05' 'proxy 安全响应头' {
  $r = Req "/api/doc/proxy?url=$demoEnc"
  Should ($r.Headers['X-Content-Type-Options'] -eq 'nosniff') ''
}
Case 'C06' 'proxy 缺 url 4xx' { $r = Req '/api/doc/proxy'; Should ($r.StatusCode -ge 400) "got $($r.StatusCode)" }
Case 'C07' 'proxy 非白名单域名 400' {
  $r = Req ('/api/doc/proxy?url=' + [uri]::EscapeDataString('https://evil.example.com/a.docx'))
  Should ($r.StatusCode -eq 400) "got $($r.StatusCode)"
  Should (($r.Content | ConvertFrom-Json).code -eq 'invalid_target') ''
}
Case 'C08' 'proxy http 协议 400' {
  $r = Req ('/api/doc/proxy?url=' + [uri]::EscapeDataString('http://calibre-ebook.com/downloads/demos/demo.docx'))
  Should ($r.StatusCode -eq 400) "got $($r.StatusCode)"
}
Case 'C09' 'proxy 非标端口 400' {
  $r = Req ('/api/doc/proxy?url=' + [uri]::EscapeDataString('https://calibre-ebook.com:8443/downloads/demos/demo.docx'))
  Should ($r.StatusCode -eq 400) "got $($r.StatusCode)"
}
Case 'C10' 'proxy 路径越界 400' {
  $r = Req ('/api/doc/proxy?url=' + [uri]::EscapeDataString('https://calibre-ebook.com/private/a.docx'))
  Should ($r.StatusCode -eq 400) "got $($r.StatusCode)"
}
Case 'C11' 'proxy userinfo 400' {
  $r = Req ('/api/doc/proxy?url=' + [uri]::EscapeDataString('https://user:pass@calibre-ebook.com/downloads/demos/demo.docx'))
  Should ($r.StatusCode -eq 400) "got $($r.StatusCode)"
}

# ---- D. SSRF 边界 ----
Case 'D01' '127.0.0.1 直连被拒' {
  $r = Req ('/api/doc/proxy?url=' + [uri]::EscapeDataString('https://127.0.0.1/a.docx'))
  Should ($r.StatusCode -eq 400) "got $($r.StatusCode)"
}
Case 'D02' '::1 直连被拒' {
  $r = Req ('/api/doc/proxy?url=' + [uri]::EscapeDataString('https://[::1]/a.docx'))
  Should ($r.StatusCode -eq 400) "got $($r.StatusCode)"
}
Case 'D03' '169.254.169.254 元数据地址被拒' {
  $r = Req ('/api/doc/proxy?url=' + [uri]::EscapeDataString('https://169.254.169.254/latest/meta-data/'))
  Should ($r.StatusCode -eq 400) "got $($r.StatusCode)"
}
Case 'D04' '10 段私网被拒' {
  $r = Req ('/api/doc/proxy?url=' + [uri]::EscapeDataString('https://10.0.0.5/a.docx'))
  Should ($r.StatusCode -eq 400) "got $($r.StatusCode)"
}
Case 'D05' '192.168 私网被拒' {
  $r = Req ('/api/doc/proxy?url=' + [uri]::EscapeDataString('https://192.168.1.1/a.docx'))
  Should ($r.StatusCode -eq 400) "got $($r.StatusCode)"
}
Case 'D06' '子域名仿冒 host 被拒' {
  $r = Req ('/api/doc/proxy?url=' + [uri]::EscapeDataString('https://calibre-ebook.com.evil.com/a.docx'))
  Should ($r.StatusCode -eq 400) "got $($r.StatusCode)"
}
Case 'D07' '错误响应体是统一 JSON' {
  $r = Req ('/api/doc/proxy?url=' + [uri]::EscapeDataString('https://evil.example.com/a.docx'))
  $j = $r.Content | ConvertFrom-Json
  Should ($j.code -and $j.message) '缺少 code/message'
}

# ---- E. 完整内核编辑器 ----
Case 'E01' '独立编辑器入口可达' {
  $r = Req '/wordcraft/'; Should ($r.StatusCode -eq 200) "got $($r.StatusCode)"
}
Case 'E02' '编辑器 wasm 以正确 MIME 提供' {
  $js = (Req '/wordcraft/').Content
  $wasm = ([regex]::Match($js, '[\w./-]+\.wasm')).Value
  Should ($wasm.Length -gt 0) 'index.html 未引用 wasm'
  $r = Req "/wordcraft/$wasm"
  Should ($r.StatusCode -eq 200) "wasm $wasm 404"
  Should ($r.Headers['Content-Type'] -match 'wasm') "ct=$($r.Headers['Content-Type'])"
}

$summary = [pscustomobject]@{ pass = $script:pass; fail = $script:fail; total = ($script:pass + $script:fail); results = $script:results }
$summary | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $PSScriptRoot 'verify-results.json') -Encoding UTF8
Write-Host "`n===== 汇总: PASS=$($script:pass) FAIL=$($script:fail) ====="
exit ($(if ($script:fail -eq 0) { 0 } else { 1 }))
