# ArtCraft 单人在线 Office 开发文档

> 版本：v0.1（单人模式 MVP）
> 日期：2026-11-05
> 目标：访问 `http://{host}/office?url=https://xxx/doc.docx` 即在浏览器内打开在线文档（docx / xlsx / pptx）

---

## 1. 项目定位与范围

### 1.1 一句话目标
基于 **ArtCraft（WordCraft / GridCraft / DeckCraft）WASM 内核**，做一个**单人在线 Office 文档查看 / 编辑**服务，通过 URL 参数直接加载远程 Office 文件。

### 1.2 技术栈

| 层 | 选型 | 版本建议 |
| --- | --- | --- |
| 编辑器内核 | ArtCraft WASM（三件套分别对应 docx/xlsx/pptx） | 跟随官方 release |
| 后端 | Rust + actix-web | actix-web 4.9 |
| 前端 | Vue 3 + Vite | Vue 3.5 / Vite 6 |
| UI 组件 | TDesign Vue Next | 1.x |
| 原子化 CSS | UnoCSS | 66.x |
| 文档代理 | actix-web + reqwest（流式转发） | reqwest 0.12 |

### 1.3 本期做 / 不做

| 做 | 不做（后续版本） |
| --- | --- |
| `?url=` 加载远程 docx/xlsx/pptx | 多人实时协同（Yjs/CRDT） |
| 后端跨域代理拉取远程文件 | 云端持久保存、版本历史 |
| 三件套按扩展名自动路由 | 账号体系 / 多租户 / 权限 |
| 本地导出下载 | 远程 MCP 网关、Agent 读写 |
| 只读模式开关 | 宏、透视表、复杂动画 |

> **重要前提**：ArtCraft 目前处于 Alpha 阶段，复杂样式（嵌套合并单元格、复杂图表、旧版 pptx 动画）保真度不稳定，上线必须做**文件大小上限**与**预览降级**。

---

## 2. 架构设计

```
浏览器
  │  GET /office?url=https://xxx/sheet.xlsx
  ▼
actix-web (Rust)
  ├─ /office          → 返回 Vue SPA（index.html）
  ├─ /api/doc/proxy   → 远程文档流式代理（解决 CORS / 鉴权 / 私有存储）
  ├─ /api/doc/meta    → 返回文件类型、大小、是否只读
  └─ /static          → 前端产物 + artcraft wasm 胶水文件
  ▼
ArtCraft WASM（浏览器内运行，本地 CPU 解析，不上传文件内容）
```

### 2.1 为什么必须有后端代理

远程 Office 文件直链经常存在以下问题，浏览器端 `fetch` 会失败：

1. 目标存储（OSSS/OSS/S3/CDN）未配置 `Access-Control-Allow-Origin`
2. 直链是**签名临时 URL**，带防盗链 `Referer` 校验
3. 直链需要 `Authorization` Header，不能暴露到前端
4. 文件较大，浏览器直接下载再转 ArrayBuffer 内存翻倍

→ 所以采用**后端流式代理 + 同源返回**，前端只面向自己的域名。

### 2.2 数据流时序

```
1. 前端解析 location.search 中的 url
2. 请求 /api/doc/meta?url=xxx  → 得到 { ext, size, mime }
3. 按 ext 选择 WordCraft / GridCraft / DeckCraft
4. 请求 /api/doc/proxy?url=xxx → 流式返回 ArrayBuffer
5. 传入 WASM 实例 → 渲染
6. 用户编辑 → 导出 blob → 本地下载（本期无云端回写）
```

---

## 3. 目录结构

```
artcraft-office/
├── backend/                     # Rust actix-web
│   ├── Cargo.toml
│   ├── .env.example
│   ├── config.toml              # 白名单、大小上限
│   └── src/
│       ├── main.rs              # 启动、App 装配
│       ├── routes/
│       │   ├── mod.rs
│       │   ├── page.rs          # /office 页面入口
│       │   └── doc.rs           # /api/doc/proxy、/api/doc/meta
│       ├── guard.rs             # SSRF 白名单 + 内网地址拦截
│       └── error.rs             # 统一错误响应
│   └── static/                  # 前端 build 产物（构建时生成）
│
├── frontend/                    # Vue3 + TDesign + UnoCSS
│   ├── package.json
│   ├── vite.config.ts
│   ├── uno.config.ts
│   ├── index.html
│   └── src/
│       ├── main.ts
│       ├── App.vue
│       ├── views/OfficeView.vue
│       ├── components/
│       │   ├── EditorHost.vue     # WASM 宿主容器
│       │   └── DocToolbar.vue     # TDesign 工具栏
│       ├── composables/
│       │   └── useDocSource.ts    # url 解析、类型判定、加载
│       ├── api/doc.ts
│       └── artcraft/loader.ts     # WASM 加载器（唯一耦合点）
│
├── vendor/artcraft/             # 存放官方 wasm/js 产物（不提交二进制）
│   ├── artcraft.js
│   └── artcraft_bg.wasm
└── README.md
```

---

## 4. 后端实现（actix-web）

### 4.1 Cargo.toml

```toml
[package]
name = "artcraft-office"
version = "0.1.0"
edition = "2021"

[dependencies]
actix-web      = "4.9"
actix-files    = "0.6"
actix-cors     = "0.7"
reqwest        = { version = "0.12", features = ["stream", "rustls-tls"], default-features = false }
tokio          = { version = "1", features = ["full"] }
serde          = { version = "1", features = ["derive"] }
serde_json     = "1"
url            = "2.5"
futures-util   = "0.3"
thiserror      = "2"
tracing        = "0.1"
tracing-subscriber = "0.3"
dotenvy        = "0.15"
ipnet          = "2"          # 内网网段判断
```

### 4.2 配置模型（config.toml）

```toml
[server]
bind = "0.0.0.0:8080"
public_base_url = "http://example.com"

[doc]
max_bytes = 52428800          # 单文件上限 50MB
timeout_secs = 30
allow_scheme = ["https"]      # 生产建议只放 https

[[doc.allow_hosts]]
host = "cdn.example.com"
paths = ["/docs/*"]

[[doc.allow_hosts]]
host = "my-bucket.oss-cn-guangzhou.aliyuncs.com"
paths = ["/*"]
```

### 4.3 SSRF 防护（guard.rs，核心安全模块）

代理接口天然是 SSRF 高危点，**必须**做三重校验：

```rust
use ipnet::IpNet;
use std::net::IpAddr;
use url::Url;

const DENY_NETS: &[&str] = &[
    "10.0.0.0/8",
    "172.16.0.0/12",
    "192.168.0.0/16",
    "127.0.0.0/8",
    "169.254.0.0/16",
    "100.64.0.0/10",
    "::1/128",
    "fc00::/7",
    "fe80::/10",
];

pub struct UrlGuard {
    allow_scheme: Vec<String>,
    allow_hosts: Vec<(String, Vec<String>)>, // host -> path patterns
}

pub enum GuardError {
    BadScheme,
    HostNotAllowed,
    PathNotAllowed,
    InternalAddress,
    BadUrl,
}

impl UrlGuard {
    /// 第一层：scheme + host + path 白名单
    pub fn check_static(&self, raw: &str) -> Result<Url, GuardError> {
        let u = Url::parse(raw).map_err(|_| GuardError::BadUrl)?;

        if !self.allow_scheme.iter().any(|s| s == u.scheme()) {
            return Err(GuardError::BadScheme);
        }
        let host = u.host_str().ok_or(GuardError::BadUrl)?.to_lowercase();
        let entry = self
            .allow_hosts
            .iter()
            .find(|(h, _)| *h == host)
            .ok_or(GuardError::HostNotAllowed)?;

        let path = u.path();
        let ok = entry.1.iter().any(|pat| match_path(pat, path));
        if !ok {
            return Err(GuardError::PathNotAllowed);
        }
        Ok(u)
    }

    /// 第二层：拒绝私有 / 保留地址（防 DNS 指向内网）
    pub fn check_ip(&self, ip: IpAddr) -> Result<(), GuardError> {
        for cidr in DENY_NETS {
            if let Ok(net) = cidr.parse::<IpNet>() {
                if net.contains(&ip) {
                    return Err(GuardError::InternalAddress);
                }
            }
        }
        Ok(())
    }
}

fn match_path(pattern: &str, path: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => path.starts_with(prefix),
        None => pattern == path,
    }
}
```

> **落地要求**：解析 DNS 后要再次校验 IP（reqwest 默认会跟随重定向，需 `redirect::Policy::none()` 或限制跳数，且每一跳重新校验）。

### 4.4 文档路由（routes/doc.rs）

```rust
use actix_web::{get, web, HttpRequest, HttpResponse, ResponseError};
use futures_util::TryStreamExt;
use reqwest::redirect::Policy;
use std::sync::Arc;

#[derive(serde::Deserialize)]
pub struct DocQuery {
    pub url: String,
    #[serde(default)]
    pub download: bool,
}

#[derive(serde::Serialize)]
pub struct DocMeta {
    pub ext: String,
    pub mime: String,
    pub size: Option<u64>,
    pub supported: bool,
}

/// 元信息：只做 HEAD，不下载正文
#[get("/api/doc/meta")]
pub async fn doc_meta(
    q: web::Query<DocQuery>,
    guard: web::Data<Arc<UrlGuard>>,
    cfg: web::Data<Arc<AppConfig>>,
) -> actix_web::Result<HttpResponse> {
    let target = guard.check_static(&q.url).map_err(bad_request)?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(cfg.doc.timeout_secs))
        .redirect(Policy::none())
        .build()
        .map_err(server_error)?;

    let resp = client
        .head(target.clone())
        .send()
        .await
        .map_err(bad_gateway)?;

    // HEAD 不被支持时降级为 GET + Range: bytes=0-0
    let size = resp
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());

    let ext = detect_ext(target.path());
    let supported = matches!(ext.as_str(), "docx" | "xlsx" | "pptx");

    Ok(HttpResponse::Ok().json(DocMeta {
        mime: mime_of(&ext).to_string(),
        ext,
        size,
        supported,
    }))
}

/// 流式代理正文
#[get("/api/doc/proxy")]
pub async fn doc_proxy(
    req: HttpRequest,
    q: web::Query<DocQuery>,
    guard: web::Data<Arc<UrlGuard>>,
    cfg: web::Data<Arc<AppConfig>>,
) -> actix_web::Result<HttpResponse> {
    let target = guard.check_static(&q.url).map_err(bad_request)?;

    // 大小上限放在网关层 + 这里双重限制
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(cfg.doc.timeout_secs))
        .redirect(Policy::none())
        .build()
        .map_err(server_error)?;

    let resp = client.get(target.clone()).send().await.map_err(bad_gateway)?;

    if let Some(len) = resp.content_length() {
        if len > cfg.doc.max_bytes {
            return Err(actix_web::error::ErrorPayloadTooLarge("file too large"));
        }
    }

    let ext = detect_ext(target.path());
    let mime = mime_of(&ext);
    let filename = target
        .path_segments()
        .and_then(|s| s.last())
        .unwrap_or("document")
        .to_string();

    let disposition = if q.download {
        format!("attachment; filename=\"{filename}\"")
    } else {
        format!("inline; filename=\"{filename}\"")
    };

    let stream = resp.bytes_stream().map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::Other, e)
    });

    Ok(HttpResponse::Ok()
        .insert_header(("Content-Type", mime))
        .insert_header(("Content-Disposition", disposition))
        .insert_header(("Cache-Control", "private, max-age=300"))
        .insert_header(("X-Content-Type-Options", "nosniff"))
        .streaming(stream))
}

fn detect_ext(path: &str) -> String {
    path.rsplit('.')
        .next()
        .unwrap_or("")
        .to_lowercase()
}

fn mime_of(ext: &str) -> &'static str {
    match ext {
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "pdf"  => "application/pdf",
        _ => "application/octet-stream",
    }
}

fn bad_request<E: std::fmt::Debug>(_e: E) -> actix_web::Error {
    actix_web::error::ErrorBadRequest("invalid document url")
}
fn bad_gateway<E: std::fmt::Debug>(_e: E) -> actix_web::Error {
    actix_web::error::ErrorBadGateway("fetch remote document failed")
}
fn server_error<E: std::fmt::Debug>(_e: E) -> actix_web::Error {
    actix_web::error::ErrorInternalServerError("server error")
}
```

### 4.5 入口与静态托管（main.rs）

```rust
use actix_files as fs;
use actix_web::{middleware, web, App, HttpServer};
use std::sync::Arc;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt().with_env_filter("info").init();

    let cfg = Arc::new(AppConfig::load("config.toml"));
    let guard = Arc::new(UrlGuard::from(&cfg));

    let bind = cfg.server.bind.clone();
    tracing::info!("listening on http://{bind}");

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(cfg.clone()))
            .app_data(web::Data::new(guard.clone()))
            .wrap(middleware::Logger::default())
            .wrap(middleware::Compress::default())
            // 静态资源：前端产物 + artcraft wasm
            .service(fs::Files::new("/assets", "./static/assets").prefer_utf8(true))
            .service(fs::Files::new("/artcraft", "./static/artcraft")
                .mime_override(artcraft_wasm_mime))
            // SPA 入口
            .route("/office", web::get().to(page::office_page))
            .route("/", web::get().to(page::office_page))
            .service(doc::doc_meta)
            .service(doc::doc_proxy)
            .default_service(web::get().to(page::spa_fallback))
    })
    .bind(bind)?
    .run()
    .await
}

/// WASM 必须返回 application/wasm，否则浏览器拒绝
fn artcraft_wasm_mime() -> actix_files::MimeOverride { /* ... */ }
```

### 4.6 关键响应头

| Header | 值 | 说明 |
| --- | --- | --- |
| `Content-Type` | 按扩展名 | 必填 |
| `Content-Disposition` | `inline` / `attachment` | 控制预览或下载 |
| `X-Content-Type-Options` | `nosniff` | 防 MIME 嗅探 |
| `Cache-Control` | `private, max-age=300` | 短期缓存，避免签名 URL 过期后仍命中 |
| WASM 资源 | `application/wasm` | **不设会导致 `instantiateStreaming` 失败** |

---

## 5. 前端实现（Vue3 + TDesign + UnoCSS）

### 5.1 vite.config.ts

```ts
import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import UnoCSS from 'unocss/vite'
import td from 'vite-plugin-tdesign-vue-next'

export default defineConfig({
  plugins: [vue(), UnoCSS(), td()],
  server: {
    port: 5173,
    proxy: {
      '/api': { target: 'http://127.0.0.1:8080', changeOrigin: true },
      '/artcraft': { target: 'http://127.0.0.1:8080', changeOrigin: true },
    },
  },
  build: {
    outDir: '../backend/static',
    emptyOutDir: true,
    assetsDir: 'assets',
  },
  optimizeDeps: { exclude: ['@artcraft/editor'] }, // wasm 包不要预构建
})
```

### 5.2 uno.config.ts

```ts
import { defineConfig, presetUno, presetAttributify, presetIcons } from 'unocss'

export default defineConfig({
  presets: [presetUno(), presetAttributify(), presetIcons()],
  shortcuts: {
    'flex-center': 'flex items-center justify-center',
    'page-shell': 'h-screen w-screen flex flex-col overflow-hidden',
  },
})
```

### 5.3 URL 解析 composable（useDocSource.ts）

```ts
import { ref, computed } from 'vue'

export type DocKind = 'docx' | 'xlsx' | 'pptx' | 'unknown'

const EXT_MAP: Record<string, DocKind> = {
  docx: 'docx', doc: 'docx',
  xlsx: 'xlsx', xls: 'xlsx', csv: 'xlsx',
  pptx: 'pptx', ppt: 'pptx',
}

export function useDocSource() {
  const remoteUrl = ref('')
  const kind = ref<DocKind>('unknown')
  const errorMsg = ref('')

  function parse() {
    const sp = new URLSearchParams(location.search)
    const raw = sp.get('url') ?? ''
    if (!raw) {
      errorMsg.value = '缺少 url 参数，示例：/office?url=https://xxx/a.docx'
      return
    }
    try {
      new URL(raw) // 合法性校验
    } catch {
      errorMsg.value = 'url 参数不合法'
      return
    }
    remoteUrl.value = raw
    const clean = raw.split('#')[0].split('?')[0]
    const ext = clean.slice(clean.lastIndexOf('.') + 1).toLowerCase()
    kind.value = EXT_MAP[ext] ?? 'unknown'
    if (kind.value === 'unknown') {
      errorMsg.value = `暂不支持的文件类型：.${ext}`
    }
  }

  return { remoteUrl, kind, errorMsg, parse, supported: computed(() => kind.value !== 'unknown') }
}
```

### 5.4 文档接口（api/doc.ts）

```ts
export interface DocMeta {
  ext: string
  mime: string
  size: number | null
  supported: boolean
}

export async function fetchMeta(url: string): Promise<DocMeta> {
  const r = await fetch(`/api/doc/meta?url=${encodeURIComponent(url)}`)
  if (!r.ok) throw new Error(`meta 请求失败：${r.status}`)
  return r.json()
}

export async function fetchBinary(url: string, signal?: AbortSignal): Promise<ArrayBuffer> {
  const r = await fetch(`/api/doc/proxy?url=${encodeURIComponent(url)}`, { signal })
  if (!r.ok) throw new Error(`文件加载失败：${r.status}`)
  return r.arrayBuffer()
}
```

### 5.5 WASM 加载器（artcraft/loader.ts）

> ⚠️ **唯一强耦合官方 API 的文件**。ArtCraft 三件套对外入口以官方仓库/官网为准，这里做**适配层隔离**，后续官方 API 变更只改这一处。

```ts
type Kind = 'docx' | 'xlsx' | 'pptx'

export interface EditorInstance {
  destroy(): void
  export(): Promise<Blob>
  setReadOnly(v: boolean): void
}

export interface CreateOptions {
  container: HTMLElement
  binary: ArrayBuffer
  fileName: string
  readOnly: boolean
}

/**
 * 三件套统一入口。实际方法名按官方 release 核对：
 *  - WordCraft  → docx
 *  - GridCraft  → xlsx
 *  - DeckCraft  → pptx
 */
export async function createEditor(kind: Kind, opts: CreateOptions): Promise<EditorInstance> {
  const sdk = await loadSdk()               // 从 /artcraft/artcraft.js 挂载全局
  const maker = kind === 'docx' ? sdk.WordCraft
              : kind === 'xlsx' ? sdk.GridCraft
              : sdk.DeckCraft

  if (!maker) throw new Error(`内核缺失：${kind}`)

  return maker.create({
    container: opts.container,
    binary: opts.binary,
    fileName: opts.fileName,
    readOnly: opts.readOnly,
  })
}

let sdkPromise: Promise<any> | null = null
function loadSdk(): Promise<any> {
  if (!sdkPromise) {
    sdkPromise = new Promise((resolve, reject) => {
      const s = document.createElement('script')
      s.src = '/artcraft/artcraft.js'
      s.onload = () => resolve((window as any).ArtCraft)
      s.onerror = () => reject(new Error('ArtCraft SDK 加载失败'))
      document.head.appendChild(s)
    })
  }
  return sdkPromise
}
```

### 5.6 主视图（OfficeView.vue）

```vue
<template>
  <div class="page-shell bg-white">
    <t-navbar :title="title" theme="light" @left-click="onBack">
      <template #right>
        <t-link theme="primary" @click="download">
          <t-icon name="download" /> 导出
        </t-link>
      </template>
    </t-navbar>

    <t-loading :loading="loading" text="文档加载中..." class="flex-1">
      <div v-if="errorMsg" class="h-full flex-center flex-col gap-3 text-gray-500">
        <t-icon name="error-circle" size="3xl" />
        <span>{{ errorMsg }}</span>
      </div>
      <EditorHost
        v-else-if="ready"
        ref="hostRef"
        :kind="kind"
        :binary="binary!"
        class="w-full h-full"
      />
    </t-loading>

    <DocToolbar
      v-if="ready"
      :read-only="readOnly"
      @toggle-readonly="toggleReadOnly"
      @reload="load"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import EditorHost from '@/components/EditorHost.vue'
import DocToolbar from '@/components/DocToolbar.vue'
import { useDocSource } from '@/composables/useDocSource'
import { fetchBinary, fetchMeta } from '@/api/doc'

const hostRef = ref<InstanceType<typeof EditorHost>>()
const { remoteUrl, kind, errorMsg: urlErr, parse } = useDocSource()

const binary = ref<ArrayBuffer | null>(null)
const loading = ref(false)
const ready = ref(false)
const readOnly = ref(true)

const errorMsg = computed(() => urlErr.value)
const title = computed(() => decodeURIComponent(remoteUrl.value.split('/').pop() || '在线文档'))
const fileName = computed(() => remoteUrl.value.split('/').pop() || 'document')

async function load() {
  if (errorMsg.value) return
  loading.value = true
  ready.value = false
  try {
    const meta = await fetchMeta(remoteUrl.value)
    if (!meta.supported) throw new Error(`不支持该格式：.${meta.ext}`)
    binary.value = await fetchBinary(remoteUrl.value)
    ready.value = true
  } catch (e: any) {
    errorMsg.value = e?.message ?? '加载失败'
    urlErr.value = errorMsg.value
  } finally {
    loading.value = false
  }
}

async function download() {
  const blob = await hostRef.value?.exportFile()
  if (!blob) return
  const a = document.createElement('a')
  a.href = URL.createObjectURL(blob)
  a.download = fileName.value
  a.click()
  URL.revokeObjectURL(a.href)
}

function toggleReadOnly(v: boolean) {
  readOnly.value = v
  hostRef.value?.setReadOnly(v)
}

onMounted(() => {
  parse()
  load()
})
</script>
```

### 5.7 编辑器宿主（EditorHost.vue）

```vue
<template>
  <div ref="el" class="w-full h-full" />
</template>

<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount } from 'vue'
import { createEditor, type EditorInstance } from '@/artcraft/loader'

const props = defineProps<{ kind: 'docx' | 'xlsx' | 'pptx'; binary: ArrayBuffer }>()
const el = ref<HTMLElement>()
let inst: EditorInstance | null = null

onMounted(async () => {
  if (!el.value) return
  inst = await createEditor(props.kind, {
    container: el.value,
    binary: props.binary,
    fileName: 'document',
    readOnly: true,
  })
})

onBeforeUnmount(() => {
  inst?.destroy()
  inst = null
})

defineExpose({
  exportFile: () => inst?.export(),
  setReadOnly: (v: boolean) => inst?.setReadOnly(v),
})
</script>
```

---

## 6. 部署方案

### 6.1 构建

```bash
# 前端
cd frontend && npm ci && npm run build          # 产物落到 backend/static

# 后端
cd backend && cargo build --release
```

### 6.2 Nginx 前置（推荐）

```nginx
server {
    listen 80;
    server_name office.example.com;

    # WASM 必须正确 MIME
    location ~ \.wasm$ {
        root /var/www/artcraft-office/static;
        types { application/wasm wasm; }
        add_header Cache-Control "public, max-age=31536000, immutable";
    }

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;

        # 大文件流式转发
        proxy_buffering off;
        proxy_request_buffering off;
        client_max_body_size 50m;
        proxy_read_timeout 60s;
    }
}
```

### 6.3 systemd 服务

```ini
[Unit]
Description=ArtCraft Office
After=network.target

[Service]
Type=simple
User=www-data
WorkingDirectory=/opt/artcraft-office
ExecStart=/opt/artcraft-office/artcraft-office
Restart=always
Environment="RUST_LOG=info"

[Install]
WantedBy=multi-user.target
```

### 6.4 验证清单

```bash
# 1. 页面可达
curl -I http://127.0.0.1:8080/office?url=https://cdn.example.com/a.docx

# 2. 代理可用（应返回正确 Content-Type）
curl -I "http://127.0.0.1:8080/api/doc/proxy?url=https://cdn.example.com/a.docx"

# 3. 元信息
curl "http://127.0.0.1:8080/api/doc/meta?url=https://cdn.example.com/a.xlsx"

# 4. SSRF 拦截必须生效
curl "http://127.0.0.1:8080/api/doc/proxy?url=http://127.0.0.1:22/"   # 应返回 400
curl "http://127.0.0.1:8080/api/doc/proxy?url=http://169.254.169.254/" # 应返回 400
```

---

## 7. 访问示例

```
http://office.example.com/office?url=https://cdn.example.com/demo.docx
http://office.example.com/office?url=https://cdn.example.com/demo.xlsx
http://office.example.com/office?url=https://cdn.example.com/demo.pptx

# 强制下载模式
http://office.example.com/office?url=https://cdn.example.com/demo.docx&download=1
```

---

## 8. 风险与已知问题

| 编号 | 风险 | 影响 | 应对 |
| --- | --- | --- | --- |
| R1 | ArtCraft 处于 Alpha | 复杂样式渲染错误、丢格式 | 文件大小限制 + 提示用户 + 保留降级到只读预览 |
| R2 | 无原生协同能力 | 无法多人同时编辑 | 本期不做；后续需自研 Yjs 同步层（重工程） |
| R3 | 代理接口 SSRF | 可探测内网 | 三重校验：scheme/host 白名单 + IP 黑名单 + 禁重定向 |
| R4 | WASM 体积大（几十 MB） | 首屏慢 | gzip/brotli + 预加载 + CDN 静态化 |
| R5 | 大文件内存 | 浏览器 OOM | 前端上限 50MB，超大文件走服务端转 PDF 预览 |
| R6 | 签名直链过期 | 打开即 403 | meta 探测失败即提示刷新来源链接 |

---

## 9. 迭代路线

| 版本 | 内容 | 预估 |
| --- | --- | --- |
| v0.1 | 本期：url 加载 + 三件套 + 代理 + 只读/导出 | 已完成设计 |
| v0.2 | 云端保存：导出 blob 回传后端 → MinIO/S3 → 生成持久 url | 3–5 天 |
| v0.3 | 文档列表 + 简单账号（可选 JWT） | 5–7 天 |
| v0.4 | MCP 网关：把 ArtCraft 能力封装为 Agent 可调工具（读、写、替换章节） | 7–10 天 |
| v0.5 | PDF 导出、全文检索 | 5 天 |
| v1.0 | 多人协同（Yjs + command 拦截），高风险重构 | 需独立评估 |

---

## 10. 待确认事项（开发启动前）

1. **ArtCraft 官方 API 具体形态**：是否提供 npm 包 / 全局 `ArtCraft` 对象 / ES module；三件套的 `create()` 参数与导出方法名需以实际 release 核对。
2. **WASM 加载方式**：是否支持 `initStreaming`、是否需要单独加载 `_bg.wasm`。
3. **许可确认**：仓库 LICENSE 复核（MIT / Apache-2.0），确认二次分发与 SaaS 商用允许。
4. **文件来源白名单**：确定允许代理的域名清单。

---

## 附录 A：多端兼容（可选）

工艺上与 A3S 的差异点：A3S 原生 Yjs 协同但界面复刻度低；ArtCraft 界面复刻高但零协同。若后续切换内核，只需替换 `artcraft/loader.ts`，上层 `OfficeView.vue` 与后端代理完全复用。
