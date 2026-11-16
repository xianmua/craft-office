# ArtCraft Office API regression script (keep this file pure ASCII for PS 5.1)
# Usage: powershell -File scripts/verify.ps1
# Output: per-case [PASS]/[FAIL] lines + scripts/verify-results.json
param(
  [string]$Base = "http://127.0.0.1:7778"
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
    if ($null -eq $resp) { throw "request error: $($_.Exception.Message)" }
    # Windows PowerShell 5.1: non-2xx throws; pull status/body from the exception
    $status = [int]$resp.StatusCode
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

# ---- A. health & static pages ----
Case 'A01' 'health 200' {
  $r = Req '/api/health'; Should ($r.StatusCode -eq 200) "got $($r.StatusCode)"
  $j = $r.Content | ConvertFrom-Json
  Should $j.ok 'ok is not true'
}
Case 'A02' 'health key fields' {
  $j = (Req '/api/health').Content | ConvertFrom-Json
  Should ($j.service -eq 'artcraft-office') 'service field missing'
  Should ($j.max_bytes -gt 0) 'max_bytes missing'
  Should ($j.allow_hosts.Count -ge 2) 'allow_hosts too few'
}
Case 'A03' 'GET / serves SPA' { $r = Req '/'; Should ($r.StatusCode -eq 200) ''; Should ($r.Content -match 'id="app"') 'no #app mount' }
Case 'A04' 'GET /office serves SPA' { $r = Req '/office'; Should ($r.StatusCode -eq 200) ''; Should ($r.Content -match 'id="app"') '' }
Case 'A05' 'SPA fallback for any non-api path' { $r = Req '/some/deep/route'; Should ($r.StatusCode -eq 200) ''; Should ($r.Content -match 'id="app"') '' }
Case 'A06' 'unknown /api/* returns JSON 404' { $r = Req '/api/nope'; Should ($r.StatusCode -eq 404) "got $($r.StatusCode)" }
Case 'A07' 'existing static asset reachable' { $asset = ((Req '/office').Content | Select-String -Pattern 'assets/index-[^"]+\.js').Matches.Value; Should ($asset.Length -gt 0) 'no JS asset reference'; if (-not $asset.StartsWith('/')) { $asset = '/' + $asset }; $r = Req $asset; Should ($r.StatusCode -eq 200) "JS asset $asset 404" }
Case 'A08' 'missing asset falls back to SPA (200 HTML, not JS)' { $r = Req '/assets/__nope__.js'; Should ($r.StatusCode -eq 200) "got $($r.StatusCode)"; Should ($r.Headers['Content-Type'] -match 'html') "ct=$($r.Headers['Content-Type'])" }
Case 'A09' '/artcraft/ missing file 404' { $r = Req '/artcraft/nope.js'; Should ($r.StatusCode -eq 404) "got $($r.StatusCode)" }
Case 'A10' '/artcraft/ path traversal rejected' { $r = Req '/artcraft/..%2FCargo.toml'; Should ($r.StatusCode -ge 400) "got $($r.StatusCode)" }

# ---- B. meta ----
Case 'B01' 'meta whitelisted file 200' {
  $r = Req "/api/doc/meta?url=$demoEnc"; Should ($r.StatusCode -eq 200) "got $($r.StatusCode): $($r.Content)"
  $j = $r.Content | ConvertFrom-Json
  Should ($j.ext -eq 'docx') "ext=$($j.ext)"
  Should ($j.supported -eq $true) 'supported=false'
  Should ($j.size -eq 1311881) "size=$($j.size)"
}
Case 'B02' 'meta without url 4xx' { $r = Req '/api/doc/meta'; Should ($r.StatusCode -ge 400) "got $($r.StatusCode)" }
Case 'B03' 'meta empty url 400' { $r = Req '/api/doc/meta?url='; Should ($r.StatusCode -eq 400) "got $($r.StatusCode)" }
Case 'B04' 'meta invalid url 400 invalid_target' {
  $r = Req ('/api/doc/meta?url=' + [uri]::EscapeDataString('not a url')); Should ($r.StatusCode -eq 400) "got $($r.StatusCode)"
  Should (($r.Content | ConvertFrom-Json).code -eq 'invalid_target') 'wrong code'
}
Case 'B05' 'meta http protocol rejected' {
  $r = Req ('/api/doc/meta?url=' + [uri]::EscapeDataString('http://calibre-ebook.com/downloads/demos/demo.docx'))
  Should ($r.StatusCode -eq 400) "got $($r.StatusCode)"
}
Case 'B06' 'meta non-whitelisted host rejected' {
  $r = Req ('/api/doc/meta?url=' + [uri]::EscapeDataString('https://evil.example.com/a.docx'))
  Should ($r.StatusCode -eq 400) "got $($r.StatusCode)"
}
Case 'B07' 'meta non-standard port rejected' {
  $r = Req ('/api/doc/meta?url=' + [uri]::EscapeDataString('https://calibre-ebook.com:8443/downloads/demos/demo.docx'))
  Should ($r.StatusCode -eq 400) "got $($r.StatusCode)"
}
Case 'B08' 'meta path escape rejected' {
  $r = Req ('/api/doc/meta?url=' + [uri]::EscapeDataString('https://calibre-ebook.com/private/a.docx'))
  Should ($r.StatusCode -eq 400) "got $($r.StatusCode)"
}
Case 'B09' 'meta userinfo rejected' {
  $r = Req ('/api/doc/meta?url=' + [uri]::EscapeDataString('https://user:pass@calibre-ebook.com/downloads/demos/demo.docx'))
  Should ($r.StatusCode -eq 400) "got $($r.StatusCode)"
}
Case 'B10' 'meta upstream 404 errors' {
  $r = Req ('/api/doc/meta?url=' + [uri]::EscapeDataString('https://calibre-ebook.com/downloads/demos/__nope__.docx'))
  Should ($r.StatusCode -ge 500) "got $($r.StatusCode)"
}

# ---- C. streaming proxy ----
$proxyFile = Join-Path $env:TEMP 'verify-proxy.docx'
Case 'C01' 'proxy full download with exact size' {
  $r = Req "/api/doc/proxy?url=$demoEnc"
  Should ($r.StatusCode -eq 200) "got $($r.StatusCode)"
  [IO.File]::WriteAllBytes($proxyFile, $r.Content)
  Should ((Get-Item $proxyFile).Length -eq 1311881) "size=$((Get-Item $proxyFile).Length)"
}
Case 'C02' 'proxy Content-Type correct' {
  $r = Req "/api/doc/proxy?url=$demoEnc"
  Should ($r.Headers['Content-Type'] -match 'wordprocessingml') "ct=$($r.Headers['Content-Type'])"
}
Case 'C03' 'proxy inline disposition' {
  $r = Req "/api/doc/proxy?url=$demoEnc"
  Should ($r.Headers['Content-Disposition'] -match 'inline; filename="demo.docx"') "cd=$($r.Headers['Content-Disposition'])"
}
Case 'C04' 'proxy download=true switches to attachment' {
  $r = Req "/api/doc/proxy?url=$demoEnc&download=true"
  Should ($r.Headers['Content-Disposition'] -match 'attachment') "cd=$($r.Headers['Content-Disposition'])"
}
Case 'C05' 'proxy security headers' {
  $r = Req "/api/doc/proxy?url=$demoEnc"
  Should ($r.Headers['X-Content-Type-Options'] -eq 'nosniff') ''
}
Case 'C06' 'proxy without url 4xx' { $r = Req '/api/doc/proxy'; Should ($r.StatusCode -ge 400) "got $($r.StatusCode)" }
Case 'C07' 'proxy non-whitelisted host 400' {
  $r = Req ('/api/doc/proxy?url=' + [uri]::EscapeDataString('https://evil.example.com/a.docx'))
  Should ($r.StatusCode -eq 400) "got $($r.StatusCode)"
  Should (($r.Content | ConvertFrom-Json).code -eq 'invalid_target') ''
}
Case 'C08' 'proxy http protocol 400' {
  $r = Req ('/api/doc/proxy?url=' + [uri]::EscapeDataString('http://calibre-ebook.com/downloads/demos/demo.docx'))
  Should ($r.StatusCode -eq 400) "got $($r.StatusCode)"
}
Case 'C09' 'proxy non-standard port 400' {
  $r = Req ('/api/doc/proxy?url=' + [uri]::EscapeDataString('https://calibre-ebook.com:8443/downloads/demos/demo.docx'))
  Should ($r.StatusCode -eq 400) "got $($r.StatusCode)"
}
Case 'C10' 'proxy path escape 400' {
  $r = Req ('/api/doc/proxy?url=' + [uri]::EscapeDataString('https://calibre-ebook.com/private/a.docx'))
  Should ($r.StatusCode -eq 400) "got $($r.StatusCode)"
}
Case 'C11' 'proxy userinfo 400' {
  $r = Req ('/api/doc/proxy?url=' + [uri]::EscapeDataString('https://user:pass@calibre-ebook.com/downloads/demos/demo.docx'))
  Should ($r.StatusCode -eq 400) "got $($r.StatusCode)"
}

# ---- D. SSRF boundaries ----
Case 'D01' '127.0.0.1 direct rejected' {
  $r = Req ('/api/doc/proxy?url=' + [uri]::EscapeDataString('https://127.0.0.1/a.docx'))
  Should ($r.StatusCode -eq 400) "got $($r.StatusCode)"
}
Case 'D02' '::1 direct rejected' {
  $r = Req ('/api/doc/proxy?url=' + [uri]::EscapeDataString('https://[::1]/a.docx'))
  Should ($r.StatusCode -eq 400) "got $($r.StatusCode)"
}
Case 'D03' '169.254.169.254 metadata rejected' {
  $r = Req ('/api/doc/proxy?url=' + [uri]::EscapeDataString('https://169.254.169.254/latest/meta-data/'))
  Should ($r.StatusCode -eq 400) "got $($r.StatusCode)"
}
Case 'D04' '10.x private net rejected' {
  $r = Req ('/api/doc/proxy?url=' + [uri]::EscapeDataString('https://10.0.0.5/a.docx'))
  Should ($r.StatusCode -eq 400) "got $($r.StatusCode)"
}
Case 'D05' '192.168 private net rejected' {
  $r = Req ('/api/doc/proxy?url=' + [uri]::EscapeDataString('https://192.168.1.1/a.docx'))
  Should ($r.StatusCode -eq 400) "got $($r.StatusCode)"
}
Case 'D06' 'subdomain spoof host rejected' {
  $r = Req ('/api/doc/proxy?url=' + [uri]::EscapeDataString('https://calibre-ebook.com.evil.com/a.docx'))
  Should ($r.StatusCode -eq 400) "got $($r.StatusCode)"
}
Case 'D07' 'error body is unified JSON' {
  $r = Req ('/api/doc/proxy?url=' + [uri]::EscapeDataString('https://evil.example.com/a.docx'))
  $j = $r.Content | ConvertFrom-Json
  Should ($j.code -and $j.message) 'code/message missing'
}

# ---- E. kernel editor ----
Case 'E01' 'standalone editor entry reachable' {
  $r = Req '/wordcraft/'; Should ($r.StatusCode -eq 200) "got $($r.StatusCode)"
}
Case 'E02' 'editor wasm served with correct MIME' {
  $js = (Req '/wordcraft/').Content
  $wasm = ([regex]::Match($js, '[\w./-]+\.wasm')).Value
  Should ($wasm.Length -gt 0) 'index.html does not reference wasm'
  $r = Req "/wordcraft/$wasm"
  Should ($r.StatusCode -eq 200) "wasm $wasm 404"
  Should ($r.Headers['Content-Type'] -match 'wasm') "ct=$($r.Headers['Content-Type'])"
}

$summary = [pscustomobject]@{ pass = $script:pass; fail = $script:fail; total = ($script:pass + $script:fail); results = $script:results }
$summary | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $PSScriptRoot 'verify-results.json') -Encoding UTF8
Write-Host "`n===== SUMMARY: PASS=$($script:pass) FAIL=$($script:fail) ====="
exit ($(if ($script:fail -eq 0) { 0 } else { 1 }))
