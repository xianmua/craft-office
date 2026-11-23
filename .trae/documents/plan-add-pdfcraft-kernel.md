# 新增 pdfcraft 内核（PDF 处理能力）

## Context

项目已接入同上游（storytold/ArtCraft 四件套）的 wordcraft/gridcraft/deckcraft 三个 wasm 内核，用户要求新增第四个内核 **pdfcraft**（对标 Acrobat 的纯 Rust PDF 阅读编辑器）处理 PDF。

上游调研结论（2026-11-23）：
- 独立仓库 `github.com/storytold/pdfcraft`，最新 release v0.21.1，极活跃，MIT OR Apache-2.0
- `apps/pdfcraft-web` 存在，Web UI 走 pdfcraft-ui-egui（egui canvas，同 gridcraft 形态）
- **官方宿主桥完整**：内嵌 `mod host`，基于 craft-libs 的 craft-web-host（rev fd5e2fef，与 gridcraft 同一契约），事件前缀 `pdfcraft:`（ready/open/save/dirty/failed），iframe 需 `?host=parent`
- **window.pdfcraft 六方法**（request/execute/commands/docInspect/uiInspect/screenshot）可用，但需 URL 追加 `?control` 才安装
- 集成了 craft-i18n（中文目录齐全），但 web 构建不嵌中文字体 → UI 中文可能方块，构建后实测再处理
- 上游自评 alpha（Acrobat 功能覆盖 57%）；web 端打印/Reopen 不可用；编辑既有文本（CJK）薄弱

本地现状：pdf 现在在前后端两处白名单外，`/office?url=xx.pdf` 直接报「暂不支持的文件类型」。

## 实施步骤

### Phase 1 内核跑通（构建 + 部署 + 前后端接线）

1. **获取内核快照**：下载 pdfcraft main 快照打包为 `.kernel-archives/pdfcraft-main.zip`，解压到 `.kernel-src/pdfcraft-main`（同现有三内核的归档还原模式，见 docs/BUILD.md 内核章节）
2. **新建 `scripts/build-pdfcraft-web.ps1`**（模板复制 [build-gridcraft-web.ps1](d:/Works/my/craft-office/scripts/build-gridcraft-web.ps1)，本就是手动复刻 trunk 的流程）：
   - `CRAFT_I18N_DIR` 指向 `.kernel-src/craft-i18n-main` + `cargo build --release -p pdfcraft-web --target wasm32-unknown-unknown`
   - wasm-bindgen → wasm-opt -Oz → 装配 dist → 生成带 SRI 的 index.html → robocopy /MIR 到 `backend/static/pdfcraft`
   - wasm 产物名 / canvas id 按 pdfcraft-web 实际源码调整（构建前读 `apps/pdfcraft-web/src/main.rs` 与 index.html 模板确认）
   - 保持纯 ASCII（PS 5.1 BOM-less 解析问题，同模板注释）
3. **`scripts/inject-kernel-page.mjs`**：`KERNELS` 数组加 `"pdfcraft"`（直达链接重定向）；中文默认注入段目前仅 gridcraft，pdfcraft 若 i18n key 相同可后续追加
4. **后端 `backend/src/main.rs`**：仿 L67-87 加 `/pdfcraft` 静态服务段
5. **后端 `backend/src/routes/doc.rs`**：
   - `is_ooxml`（L35）加 `"pdf"` 直通（pdf 原生格式无需转换，`mime_of` 已含 pdf），函数名改为 `is_native_supported` 或加注释说明
   - pdf 不走 convert 链路（`effective_ext` 天然直通）
6. **前端 `frontend/src/composables/useDocSource.ts`**：`DocKind` 加 `'pdf'`，`EXT_MAP` 加 `pdf: 'pdf'`，两处报错文案更新为「docx / xlsx / pptx / pdf」
7. **前端 `frontend/src/artcraft/loader.ts`**：
   - `KERNEL_OF` 加 `pdf: 'pdfcraft'`
   - `kernelFrameUrl` 为 pdfcraft 追加 `&control`（六方法 opt-in 必需；用小映射表按内核附加参数）
8. **OfficeView.vue**：确认 `KERNEL_OF` 驱动的分支对 pdf 无硬编码遗漏（保存链路 onKernelSave 是通用的；编辑类 gate 按 write 能力走 kernels.json 数据驱动）

### Phase 2 AI 助手接入（外壳声明式，参照 gridcraft 模式）

9. **`assets/plugins/ai-assistant/kernels.json`** 加 pdfcraft 条目：
   - `prompt: "agent-pdf.md"`、`skills: ["pdf/SKILL.md"]`、`catalog: "commands-pdfcraft.json"`、`context: "pdf"`、`write: true`、`cjkFont: false`（PDF 自嵌字体，中文文档实测后如需再开）
   - `intents` 按 PDF 场景写（页面提取/合并/拆分/旋转/注释/表单/加密/书签/截图导出）
10. **新建 `assets/plugins/ai-assistant/agent-pdf.md`**（提示词）与 **`assets/skills/pdf/SKILL.md`**（技能说明，含 pdfcraft 命令习惯用法）
11. **`frontend/src/artcraft/shellAssistant.ts`**：`docContext` 加 `pdfContext` 格式化器（基于 `docInspect()` 实际返回结构压文本；部署后实测再定字段）
12. **命令目录导出**：浏览器 `commands()` 落盘 `commands-pdfcraft.json`（同 gridcraft 导出方式，97 个 agent 工具/约 290 条命令）

### Phase 3 文档与样例

13. **docs/BUILD.md**：内核构建章节补 pdfcraft（归档名、构建命令、部署步骤）
14. **samples/demo.pdf**：补一个中文测试样例（可用 python 生成或 pdfcraft `cargo xtask demo-pdf`）

## 风险与对策

| 风险 | 对策 |
|------|------|
| web 端 UI 中文方块（上游明示不嵌中文字体） | 构建后实测；参照 gridcraft 的 craft-i18n 接入方式排查字体嵌入；实在不行 UI 暂时英文可用 |
| pdfcraft 独立仓库，craft-libs 依赖走 git rev，cargo 拉取可能受网络影响 | 构建实测；必要时走已有代理/镜像配置（三内核构建时 cargo 网络可用） |
| `?control` 保留无障碍树可能影响性能 | 实测大文档打开速度；必要时评估 uiInspect 降级 |
| wasm 体积（OCR worker/模型） | 部署后检查 backend/static/pdfcraft 体积；OCR 在 web 端按需加载则无碍 |
| alpha 成熟度，CJK 文本编辑薄弱 | AI 助手提示词里明确能力边界（读取/注释/表单/页面整理为主，改正文不承诺） |

## 验证

1. `cargo build --release -p pdfcraft-web --target wasm32-unknown-unknown`（在 .kernel-src/pdfcraft-main，CRAFT_I18N_DIR 已设）→ 跑 build-pdfcraft-web.ps1 → `node scripts/inject-kernel-page.mjs`
2. 后端 `cargo test`（27 项存量测试不回归）+ 启动后端
3. 浏览器验证：`/office?url=/samples/demo.pdf` → 内核 ready、文档打开、中文渲染正常、`window.pdfcraft.commands()` 可用
4. 保存链路：修改注释/页面后触发 save 事件 → 悬浮保存成功
5. AI 助手：「这份 PDF 有几页」「把前 3 页提取出来」等指令实测（SSE 流式 + execute）
6. 回归：word/grid/deck 三内核各打开一个样例文档确认无影响
