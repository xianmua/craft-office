# ArtCraft 单人在线 Office（craft-office）

基于 ArtCraft（WordCraft / GridCraft / DeckCraft）内核的**单人在线 Office** 服务：
访问 `http://{host}/office?url=https://xxx/demo.docx` 即可在浏览器里打开 docx / xlsx / pptx。

版本：v0.1（单人 MVP）｜后端 Rust + actix-web，前端 Vue 3 + Vite + TDesign + UnoCSS。

## 现在能做什么

| 能力 | 状态 |
| --- | --- |
| `?url=` 加载远程 docx / xlsx / pptx | ✅ |
| 后端跨域 / 签名直链流式代理（同源、不上传文件内容） | ✅ |
| 按扩展名自动路由三件套 | ✅（内核接入后生效） |
| SSRF 防护（scheme+host+path+端口白名单、DNS 解析期 IP 校验、禁重定向） | ✅ |
| 本地导出下载 / 下载原文件 | ✅（导出依赖内核） |
| 只读 / 编辑切换 | ✅（切换依赖内核） |
| 内核缺失时降级只读预览（浏览器本地解 OOXML，展示正文 / 表格 / 幻灯片文本） | ✅ 本版新增 |
| 多人协同、云端保存、账号体系 | ❌ v0.2+ |

> 内核本身不在本仓库内（也无 npm 包），接入方式见 [vendor/artcraft/README.md](vendor/artcraft/README.md)。
> 未接入内核时页面不会白屏，而是走降级预览：结构解析在浏览器本地完成，文件内容不经过任何第三方服务。

## 快速开始

前置：Node ≥ 20、Rust ≥ 1.80。

```powershell
# 1) 前端依赖 + 构建（产物直接落到 backend/static）
cd frontend
npm install
npm run build

# 2) 后端编译并启动（默认 0.0.0.0:8080）
cd ..\backend
cargo run
```

打开：<http://127.0.0.1:8080/office?url=https://calibre-ebook.com/downloads/demos/demo.docx>

开发模式（前端热更新，`/api`、`/artcraft` 代理到 8080）：

```powershell
cd backend;  cargo run
cd frontend; npm run dev      # http://127.0.0.1:5173/office?url=...
```

更多：见 [docs/BUILD.md](docs/BUILD.md)；原始设计文档：[docs/ArtCraft在线Office开发文档.md](docs/ArtCraft在线Office开发文档.md)。

## 目录结构

```
craft-office/
├── backend/                  # Rust + actix-web
│   ├── config.toml           # 白名单、大小上限、端口
│   ├── src/
│   │   ├── main.rs           # 启动、App 装配、静态托管
│   │   ├── config.rs         # 配置模型（fail-closed 校验）
│   │   ├── guard.rs          # SSRF：白名单 + DNS/IP 校验（含单元测试）
│   │   ├── error.rs          # 统一 JSON 错误体
│   │   └── routes/
│   │       ├── page.rs       # /office、/api/health、/artcraft/*、SPA 兜底
│   │       └── doc.rs        # /api/doc/meta、/api/doc/proxy（流式 + 限流）
│   └── static/               # 前端构建产物（构建时生成）
├── frontend/                 # Vue 3 + TDesign + UnoCSS
│   └── src/
│       ├── views/OfficeView.vue
│       ├── components/EditorHost.vue | DocToolbar.vue | FallbackPreview.vue
│       ├── composables/useDocSource.ts
│       ├── api/doc.ts
│       └── artcraft/loader.ts   # ← 唯一强耦合官方内核 API 的文件
│       └── artcraft/ooxml.ts    # 降级预览的 OOXML 解析
├── vendor/artcraft/          # 内核产物放置说明（不提交二进制）
├── docs/                     # 设计文档与构建说明
└── scripts/                  # 启动 / 构建脚本
```

## 接口

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET | `/office?url=…` | SPA 入口 |
| GET | `/api/doc/meta?url=…` | 返回 `{ext, mime, size, supported}`，只做 HEAD / Range 探测 |
| GET | `/api/doc/proxy?url=…[&download=1]` | 流式代理正文，`inline` / `attachment` |
| GET | `/api/health` | 服务状态 + `editor_kernel_ready` |

## 安全模型（为什么必须有后端代理）

远程 Office 直链常见四类问题：跨域未放行、签名 URL + Referer 防盗链、需要 Authorization、文件大导致内存翻倍。
因此文件统一经后端流式转发，前端只面向自己的域名。

代理是 SSRF 高危点，`backend/src/guard.rs` 做了三重校验：

1. **静态白名单**：scheme（默认仅 https）、host、path 前缀、端口（默认仅 443），并拒绝带 userinfo 的 URL；
2. **解析期 IP 校验**：`allow_hosts` 命中 ≠ 安全，DNS 仍可能解析到内网。解析后逐个地址比对内网 / 保留 / 组播网段（含 `169.254.169.254`、`100.64.0.0/10`），并只用通过校验的地址；
3. **地址固定 + 禁重定向**：用 `ClientBuilder::resolve(host, addr)` 把域名钉死在已校验 IP 上（防 DNS 重绑定），`redirect::Policy::none()` 保证不被 302 到内网。

另外：流式转发时边转发边计数，`Content-Length` 可伪造，超限即中断；流式请求不设整体时限，改用连接超时 + 读空闲超时，避免慢速大文件被误杀。

## 与原始开发文档的差异

| 项 | 文档方案 | 本实现 | 原因 |
| --- | --- | --- | --- |
| TDesign 按需引入 | `vite-plugin-tdesign-vue-next` | `app.use(TDesign)` 全局注册 | 该 vite 插件在 npm 上无对应包，避免装不上 |
| 内核集成 | 直接调 `ArtCraft.WordCraft.create()` | 保留该契约，另加可插拔 + 降级预览 | 官方无 npm 包，且浏览器产物是完整应用而非 SDK |
| 元信息 | 仅 HEAD | HEAD 失败降级 `GET + Range: bytes=0-0` 并解析 `Content-Range` | 不少 CDN 对 HEAD 返回 405 |
| 代理限流 | 仅看 `Content-Length` | 额外做流式计数上限 | 头部长度可由上游伪造 |
| 代理超时 | `Client.timeout(30s)`（整个请求总时限） | meta 用总时限；流式转发改为「连接 10s + 读空闲超时」 | 总时限会把「慢但正常」的大文件流直接掐断（实测 1.3MB 只传到 868KB 就 `error decoding response body`） |
| 端口控制 | 文档未提 | `allow_ports` 默认 `[443]` | 防止代理被当作任意端口探测器 |
| 健康检查 | 文档未提 | 新增 `/api/health` | 让前端判断内核是否就位，避免无谓的 404 |
| actix-cors | 文档列入依赖 | 保留依赖但未挂载中间件 | 前端与 API 同源，跨域中间件反而放宽攻击面 |

## 已验证

**构建与单测**

- `cargo test`：12 项全通过（白名单/协议/端口/路径/userinfo、内网与保留网段判定、扩展名、MIME、`Content-Range` 解析、文件名净化）；
- `cargo build --release` 通过；`npm run build`（含 `vue-tsc` 类型检查）通过，产物输出到 `backend/static`。

**接口实测**（服务监听 `0.0.0.0:8080`，用 `https://calibre-ebook.com/downloads/demos/demo.docx`，1311881 字节）

| 项 | 结果 |
| --- | --- |
| `GET /api/health` | 200，`editor_kernel_ready=false` |
| `GET /office?url=...` | 200，返回 SPA 入口（464 字节） |
| `GET /api/doc/meta?url=...` | 200，`{"ext":"docx","size":1311881,"supported":true}` |
| `GET /api/doc/proxy?url=...` | 200，`Content-Type` 为 docx MIME、`Content-Disposition: inline; filename="demo.docx"`，落盘 **1311881 字节**（与源文件逐字节一致） |
| SSRF 六种场景 | 全部 400 + `code=invalid_target`：非白名单域名、`http` 协议、端口 8443、路径越界、URL 含 userinfo、`127.0.0.1` 直连 |

**浏览器端到端**（Chrome，`http://127.0.0.1:8080/office?url=...`）

- docx：降级预览真实渲染出 calibre 示例文档正文（标题、段落、加粗/斜体描述段等均按段落顺序展示），顶部提示「未检测到 ArtCraft 内核，当前为降级只读预览」；
- xlsx：解析出 2 个工作表（表名、共享字符串、inlineStr、数字单元格），表头 A/B/C 与行号正确，越界行列触发 `已裁剪`标记；
- pptx：逐页抽取文本框内容并按「第 N 页」卡片渲染。
- 过程中修掉的真实缺陷：代理原用 30s **总时限**，慢上游（约 50KB/s）传 1.3MB 时在 868KB 处被掐断，日志为 `Response payload stream error ... "error decoding response body"`；改为 `connect_timeout(10s) + read_timeout` 后同一文件完整落盘。

```powershell
# 页面
curl.exe -I "http://127.0.0.1:8080/office?url=https://calibre-ebook.com/downloads/demos/demo.docx"
# 元信息
curl.exe "http://127.0.0.1:8080/api/doc/meta?url=https://calibre-ebook.com/downloads/demos/demo.docx"
# 代理下载并核对字节数（Windows）
curl.exe -s -o "$env:TEMP\proxy-demo.docx" "http://127.0.0.1:8080/api/doc/proxy?url=https://calibre-ebook.com/downloads/demos/demo.docx"
(Get-Item "$env:TEMP\proxy-demo.docx").Length   # 期望 1311881
# SSRF 必须被拦截（400 / invalid_target）
curl.exe "http://127.0.0.1:8080/api/doc/proxy?url=http://127.0.0.1:22/"
curl.exe "http://127.0.0.1:8080/api/doc/proxy?url=https://evil.example.org/a.docx"
```

> 未验证/未覆盖：真实 ArtCraft 内核（本机无产物，无 npm 包）下的编辑、`save()` 导出与只读切换，仅在适配层做了类型与契约准备；超大文件（接近 50MB）在浏览器内的解压耗时未做压测。

## 已知风险

| 编号 | 风险 | 应对 |
| --- | --- | --- |
| R1 | ArtCraft 仍在 alpha，复杂样式保真度不稳 | 50MB 上限 + 降级只读预览 + 提示 |
| R2 | 无协同能力 | 本期不做；后续需自研 Yjs 同步层 |
| R3 | 代理 SSRF | 三重校验 + 流式限流（见上） |
| R4 | WASM 体积大（几十 MB） | 建议 gzip / brotli + CDN 静态化 |
| R5 | 大文件内存 | 前端上限 50MB，更大文件建议服务端转 PDF 预览 |
| R6 | 签名直链过期 | meta 失败即提示更换链接 |

## 后续路线

| 版本 | 内容 |
| --- | --- |
| v0.2 | 导出 blob 回传后端 → S3/MinIO → 持久 url |
| v0.3 | 文档列表 + 简单账号 |
| v0.4 | MCP 网关，把 ArtCraft 能力封装为 Agent 可调工具 |
| v0.5 | PDF 导出、全文检索 |
| v1.0 | 多人协同（Yjs + command 拦截） |
