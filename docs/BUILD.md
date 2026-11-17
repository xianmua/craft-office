# 构建与运行

## 依赖
- Node.js >= 20
- Rust >= 1.80（`cargo` 在 PATH 中）
- CJK 兜底字体（不入库）：`backend/static/fonts/SourceHanSansCN-{Regular,Bold}.otf`。
  下载 `https://registry.npmmirror.com/source-han-sans-cn/-/source-han-sans-cn-1.0.0.tgz`
  解包取 `package/SourceHanSansCN-{Regular,Bold}.otf` 放入 `backend/static/fonts/`。
  缺失时 wordcraft 中文正文显示为方块（外壳 `font.add` 自动安装，见 `shellAssistant.ts` 的 `ensureCjkFont`）。

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
| `scripts\build-gridcraft-web.ps1` | gridcraft wasm 装配部署（同上） |
| `scripts\inject-kernel-page.mjs` | 内核页部署期注入（幂等）：直达链接 `/<kernel>/?url=` 重定向到 /office、gridcraft 默认中文（原 0006 补丁） |

## 内核源码还原与 wasm 构建

`.kernel-src/` 不入库（体积大）。2026-11-17 起三内核均为零补丁快照（wordcraft 升级到含官方页面命令 API 的 main，插件系统由 /office 外壳运行时承接），历史 wordcraft 补丁封存在 `patches/retired/`。从零还原步骤：

1. 解包归档：`.kernel-archives\*.zip` 解压到 `.kernel-src\` 即得源码（wordcraft/gridcraft 还需 `.kernel-src\craft-i18n-main`）。
   - 注意：wordcraft-main.zip 是旧快照（不含页面命令 API）；新快照从 GitHub 重新获取（wordcraft 新快照的 craft-libs 依赖 rev 较多，cargo 需联网拉取 git 依赖）。
2. 构建 wasm 并部署到 `backend\static\<kernel>\`：

   ```powershell
   # wordcraft / gridcraft 需先设置翻译目录，否则界面退化为纯英文
   $env:CRAFT_I18N_DIR = "D:<你的路径>\craft-office\.kernel-src\craft-i18n-main"
   cargo build --release -p wordcraft-web --target wasm32-unknown-unknown   # 在 .kernel-src\wordcraft-main 下
   powershell -File scripts\build-web-manual.ps1        # gridcraft 用 build-gridcraft-web.ps1，deckcraft 用 build-deckcraft-web.ps1
   node scripts\inject-kernel-page.mjs                  # 任意内核 (重)部署后必跑，补直达链接重定向与 gridcraft 中文默认
   ```

3. wordcraft 依赖 craft-libs 的多个 git rev；github.com 主站被墙时 codeload.github.com 通常仍可用，可手动 `git fetch https://github.com/storytold/craft-libs <rev>:refs/commit/<rev>` 重试补齐缺失 rev 后再构建。

## DeepSeek MCP 桥（无头文档代理）

内核原生 MCP 服务器 + DeepSeek function calling：`node scripts\deepseek-mcp.mjs "指令"`（单轮）或不带参数进 REPL。模型在 `mcp-workspace\` 内创建/编辑/导出文档（读写沙箱），17 个工具（新建/打开/保存/排版/表格/渲染页图/自检等）。依赖 `vendor\wordcraft-cli\wordcraft-cli.exe`（重建：解包 `.kernel-archives\wordcraft-main.zip` 后 `cargo build --release -p wordcraft-cli`，把 exe 拷入 vendor）与 `backend\.env` 的 DEEPSEEK_* 配置。
