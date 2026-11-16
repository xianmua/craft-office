# 构建与运行

## 依赖
- Node.js >= 20
- Rust >= 1.80（`cargo` 在 PATH 中）

## Windows 一键开发（两个终端）

```powershell
# 终端 1：后端（默认 0.0.0.0:7778）
cd backend
cargo run

# 终端 2：前端 dev server（7777，/api 与 /artcraft 代理到 7778）
cd frontend
npm install
npm run dev
```

打开 http://127.0.0.1:7777/office?url=https://calibre-ebook.com/downloads/demos/demo.docx

## 生产构建

```powershell
cd frontend
npm ci
npm run build          # 产物落到 backend/static

cd ..\backend
cargo build --release
.\target\release\artcraft-office.exe
```

打开 http://127.0.0.1:8080/office?url=...

## 常用脚本

| 脚本 | 作用 |
| --- | --- |
| `scripts\dev-backend.ps1` | 启动后端（可从任意目录执行） |
| `scripts\dev-frontend.ps1` | 启动前端 dev server |
| `scripts\build.ps1` | 前端 + 后端全量构建 |
| `scripts\publish.ps1` | 打部署包（exe + static + plugins + samples + 配置模板 → dist\*.zip） |
| `scripts\deepseek-mcp.mjs` | DeepSeek ↔ WordCraft MCP 桥（无头文档代理，见下文） |
| `scripts\build-web-manual.ps1` | wordcraft wasm 装配部署（trunk 卡死的手动管线） |
| `scripts\build-deckcraft-web.ps1` | deckcraft wasm 装配部署（同上） |
| `scripts\inject-kernel-page.mjs` | 内核页部署期注入（幂等）：直达链接 `/<kernel>/?url=` 重定向到 /office、gridcraft 默认中文（原 0006 补丁） |

## 内核源码还原与 wasm 构建

`.kernel-src/` 不入库（体积大），定制改动全部固化为 `patches/` 下的增量补丁。从零还原步骤：

1. 优先解包本地归档（若有）：`.kernel-archives\*.zip` 解压到 `.kernel-src\` 即得含补丁的源码；否则获取上游内核源码压缩包，解压到 `.kernel-src\wordcraft-main`、`.kernel-src\gridcraft-main`、`.kernel-src\deckcraft-main`（wordcraft/gridcraft 还需 `.kernel-src\craft-i18n-main`），再执行第 2 步重放补丁（解包归档的源码已含补丁，跳过）
2. 重放补丁（在仓库根目录按编号执行）：

   ```powershell
   git apply patches\deckcraft-0001-host-bridge.patch
   ```

3. 构建 wasm 并部署到 `backend\static\<kernel>\`：

   ```powershell
   # wordcraft / gridcraft 需先设置翻译目录，否则界面退化为纯英文
   $env:CRAFT_I18N_DIR = "D:<你的路径>\craft-office\.kernel-src\craft-i18n-main"
   cargo build --release -p wordcraft-web --target wasm32-unknown-unknown
   powershell -File scripts\build-web-manual.ps1        # deckcraft 用 build-deckcraft-web.ps1
   node scripts\inject-kernel-page.mjs                  # 任意内核 (重)部署后必跑，补直达链接重定向与 gridcraft 中文默认
   ```

   gridcraft 暂无独立脚本，复制 `build-deckcraft-web.ps1` 替换内核名/路径即可。

## DeepSeek MCP 桥（无头文档代理）

内核原生 MCP 服务器 + DeepSeek function calling：`node scripts\deepseek-mcp.mjs "指令"`（单轮）或不带参数进 REPL。模型在 `mcp-workspace\` 内创建/编辑/导出文档（读写沙箱），17 个工具（新建/打开/保存/排版/表格/渲染页图/自检等）。依赖 `vendor\wordcraft-cli\wordcraft-cli.exe`（重建：解包 `.kernel-archives\wordcraft-main.zip` 后 `cargo build --release -p wordcraft-cli`，把 exe 拷入 vendor）与 `backend\.env` 的 DEEPSEEK_* 配置。
