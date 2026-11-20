# 构建与运行

## 依赖
- Node.js >= 20
- Rust >= 1.80（`cargo` 在 PATH 中）
- CJK 兜底字体（不入库）：`backend/static/fonts/SourceHanSansCN-{Regular,Bold}.otf`。
  下载 `https://registry.npmmirror.com/source-han-sans-cn/-/source-han-sans-cn-1.0.0.tgz`
  解包取 `package/SourceHanSansCN-{Regular,Bold}.otf` 放入 `backend/static/fonts/`。
  缺失时 wordcraft 中文正文显示为方块（外壳 `font.add` 自动安装，见 `shellAssistant.ts` 的 `ensureCjkFont`）。

## 旧格式自动转换（可选，doc/xls/ppt/csv → OOXML）

后端采用双引擎，启动时自动探测，检测到任一引擎即支持旧格式文档加载（自动转换为 docx/xlsx/pptx 再进内核），都没有则维持「格式不受支持」的拒绝提示。

**引擎 1（优先）：Node sidecar `vendor/convert-svc/`**——@deepseek-ai/libreoffice-kit（DeepSeek 官方，MPL 2.0）包装的本地 HTTP 服务，免装 LibreOffice，稳态转换约 1-3s。组装步骤（不入库）：

1. `mkdir vendor/convert-svc`，把 Node >= 22.19 的 `node.exe` 拷入（本机可用 `mise` 安装的 node）。
2. 任意目录 `npm install @deepseek-ai/libreoffice-kit`（自动带平台引擎包），把生成的 `node_modules` 整目录拷入 `vendor/convert-svc/`。
3. 可精简：删除 `node_modules/@deepseek-ai/libreoffice-kit-wasm/`（Windows 上原生引擎 win32-x64 优先，wasm 仅是 fallback，可省约 145MB）。
4. `svc.js` 已入库（sidecar 服务本体），无需额外获取。

**引擎 2（兜底）：LibreOffice `vendor/libreoffice/`**——可选。推荐便携版（MPL 2.0），解压到该目录即可，稳态转换约 7.5s。当前仓库未放置该目录（sidecar 已覆盖主力格式）；补充放置后 csv→xlsx 也能转换（sidecar 的 kit 不支持 csv 输入，doc/xls/ppt 则两引擎均可）。

探测顺序与回退：

1. 优先走 sidecar（`vendor/convert-svc/svc.js` + node 可执行，node 先找目录内自带再找 PATH）；sidecar 启动失败会在本进程内标记不可用并回退。
2. sidecar 未配置或单次转换失败时回退 soffice：环境变量 `CRAFT_SOFFICE` → 便携版 `vendor/libreoffice/` → PATH → 常见安装路径（Windows: `C:\Program Files\LibreOffice\program`）。

sidecar 进程由后端懒启动、随主进程退出（stdin 管道断开自动自杀，无孤儿残留）。发布时两个目录存在即整目录随包拷贝（publish.ps1 自动处理）。转换产物缓存在文档缓存目录（key 用转换后扩展名），同一链接二次打开秒回；转换仅用于加载展示，不回写原文档。

## Windows 一键开发（两个终端）

```powershell
# 终端 1：后端（默认 0.0.0.0:7778）
cd backend
cargo run

# 终端 2：前端 dev server（7777，/api 与内核静态目录代理到 7778）
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
