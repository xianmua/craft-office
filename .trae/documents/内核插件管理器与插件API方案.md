# 内核插件管理器 + 插件 API（cordis 风格插件市场方案）

## Context

用户目标：内核只内置一小块通用「插件管理器 + 插件 API」，之后所有外加功能（包括刚做的 AI 对话助手）全部以 JS 插件形式接入，装上即用；插件市场只是分发渠道，类似 cordis/Koishi 的插件体系。

已确认决策：
- 插件 JS 在 wordcraft 页面主线程 `new Function` 直执行（不做 iframe 沙箱）
- 插件启用/禁用状态存后端 `installed.json`
- 原生 AI 助手窗格（ai_pane.rs，对话流 + `<write>` 自动写回）迁成插件

现状要点（已核实）：
- 内核 `Pane` 枚举硬编码 12 窗格（panes.rs:52），`from_key` 只认静态 ALL；命令注册表编译期固化
- `Services`（services.rs）全是 `Option<Box<dyn Fn>>` 宿主钩子 + `Arc<Mutex<Vec>>` 队列回复模式（AiReplies 为范例）
- 控制协议 `control::handler::handle()`（`document.inspect`/`engine.execute`）仅桌面 TCP 有，web 端无任何 wasm-bindgen 导出
- Vue 宿主已有 PluginMarket.vue + api/plugins.ts（4 端点）；后端 actix-web，`installed.json` 现为纯 id 数组，示例插件用已删除的旧 `hostApi.*` API
- 市场安装链路已通：内核帮助菜单「插件管理」→ `wordcraft:plugins` postMessage → Vue 弹窗

## 架构

```
插件 JS ──ctx.*──> 宿主运行时(web/plugins.rs) ──window.__craftKernel.send(json)──> wasm 请求队列
wasm 每帧排空 ──> control::handle(engine.execute / document.*) ──> 回复队列 ──> __craftKernelOnReply ──> resolve Promise
内核（面板状态/toast/命令注册/启停）── Services 钩子 ──> 宿主运行时分发回插件回调
AI 聊天不走内核桥：插件 JS 同源直接 fetch('/api/ai/chat') 读 SSE
```

两处关键选型（已验证代码依据）：
1. **动态面板用 `Pane::Plugin(u16)` 枚举带 payload**，不用旁路列表——`craft_ui::panes::PaneHost::show<P: Clone+Hash+Debug>` 的 `open: &[P]` 是单一切片，同变体无法区分两次出现；u16 是 Copy，枚举仍 Copy，冲击最小
2. **桥用 js_sys 运行时注册 `window.__craftKernel`**，不用 `#[wasm_bindgen]` 导出宏——bin crate 里一行 JS 源码字符串即可，闭包捕获 wasm 队列无竞争（单线程）

## 阶段 1：后端插件目录升级（独立可验证）

文件：
- `backend/src/routes/plugins.rs`：`installed.json` 改 `[{id, enabled}]`（兼容旧 `[]`/`["id"]` 双读，缺省 enabled=true）；新增 `POST /api/plugins/enable {id, enabled}`；写文件改 tmp+rename 原子替换
- `backend/plugins/market.json`：加 `ai-assistant` 条目
- `backend/plugins/installed.json`：预置 `[{"id":"ai-assistant","enabled":true}]`
- `backend/plugins/ai-summary.js`、`doc-stats.js`：从旧 `hostApi.*` 迁到新 `ctx.*` API（API 见阶段 3）
- `backend/plugins/ai-assistant.js`：先占位，阶段 4 补全

验证：`cargo check`（backend）；起服务 `GET /api/plugins` 看新结构。

## 阶段 2：内核 plugins 模块 + 动态面板

### 2.1 新增 `crates/ui-egui/src/plugins/` 模块

```
plugins/mod.rs       公共类型 + re-export
plugins/registry.rs  app 级插件注册表：plugins: Vec<PluginEntry{id,name,enabled,commands,panels}> + 句柄分配（单调递增不复用）
plugins/panes.rs     per-window 面板运行时状态：open: Vec<(u16, PanelState{state:Value, prompt:String})>，apply_state 浅合并
plugins/manager.rs   内核插件管理器 UI（egui Window：启停 checkbox + 打开市场 + 重新加载）
plugins/render.rs    通用声明式面板渲染（消息列表 + 输入框 + busy/error）
```

核心类型（仿 AiReplies 队列模式）：
```rust
pub enum PluginEvent { Ready, Commands{plugin, commands: Vec<CommandSpec{id,title}>},
    Panels{plugin, panels: Vec<PanelSpec{id,title,default_width}>}, PanelState{panel,state:Value},
    Toast{message}, Enabled{id,enabled}, Error{plugin,message} }
pub type PluginEvents = Arc<Mutex<Vec<PluginEvent>>>;
```

### 2.2 Services 新钩子（services.rs）

```rust
plugin_load: Option<Box<dyn Fn()>>,                    // 装载插件，结果经 PluginEvents
plugin_set_enabled: Option<Box<dyn Fn(&str, bool)>>,   // 启停，宿主写 installed.json 并重载
plugin_invoke: Option<Box<dyn Fn(&str, Value)>>,       // 内核→插件：命令执行/面板事件
```

`WordApp` 加 `plugin_events: Option<PluginEvents>` + `plugins: Registry`；`window_ui.rs` 加 per-window `plugin_panes: Panes`（swap 穷举编译器兜底）；`app.rs logic()` 加 `drain_plugins`。

### 2.3 panes.rs 动态面板

- `Pane` 加变体 `Plugin(u16)`；`ALL` 12 个不动
- `key()` 改 `Cow<'static, str>`（或 `key_in(reg)`）：静态走 Borrowed，插件走 `format!("plugin:{panel_id}")`
- `PaneContent::title/body/default_state` 特判 `Plugin(h)` → 注册表标题 / `render::panel_ui` / 宽度夹到 `200..=520`
- `from_key_in(reg, key)`：静态匹配 + `"plugin:"` 前缀查注册表；`state.rs::order/open` 换用
- `lenient_order/lenient_places` 放行 `plugin:` 前缀 key（卸载残留无害，运行时查不到自动丢弃）
- `close_command()` 返回 None，`host.rs::close()` 特判关闭 `plugin_panes`

### 2.4 内核命令与入口

`app/commands.rs` ui_command 新增：`ui.plugins`（开管理器）、`ui.plugin.enable/reload/run/openPane/panelSend`；`ribbon/review.rs::help()`：Plugins 按钮改 `ui.plugins`，其下新增插件命令组（遍历注册表渲染，点击即 `ui.plugin.run`）。

验证：`cargo check -p wordcraft-ui-egui` + `cargo test -p wordcraft-ui-egui`（navigation_tests / swap_window_round_trip 兜底）+ 桌面跑 12 窗格无回归。

## 阶段 3：web 层桥 + 插件运行时

### 3.1 新增 `apps/wordcraft-web/src/web/plugins.rs`

```rust
pub struct PluginRuntime { requests, replies, ctx }
impl PluginRuntime {
    pub fn start(ctx) -> Rc<RefCell<Self>>;  // 注册 window.__craftKernel / 拉清单 / new Function 执行
    pub fn pump(&mut self, app: &mut WordApp); // 每帧：排空请求→control::handle→写回复→调 onReply
    fn invoke(&self, channel, payload);        // Services::plugin_invoke 实现
}
```

桥通道（复用 control 协议）：`engine.execute`、`document.text`（control/handler.rs 新增方法：有选区返回 `{selection, text}`，桌面 MCP 同受益）；通知类：`plugins.register`、`panel.setState`、`ui.toast`、`panel.open`。

### 3.2 接线

- `web.rs services()`：注入 `plugin_load` / `plugin_set_enabled`（POST enable 后重载）/ `plugin_invoke`；`WebShell.logic()` 开头 `pump()`（先于 app.logic）
- 装/卸后内核重载：新增 postMessage `wordcraft:pluginsChanged`（loader.ts + PluginMarket.vue → 内核监听 → plugin_load）

### 3.3 插件 ctx API（JS 侧）

```js
ctx.registerCommand({id, title, run})        // 帮助菜单出现命令
ctx.registerPanel({id, title, defaultWidth}) // 注册声明式面板
ctx.panel.open(id) / ctx.panel.setState(id, {...}) / ctx.on('panel.send', fn)
ctx.doc.getText() / getSelectedText() / insert(text) / replaceSelected(text)  // Promise
ctx.ai.chat(prompt) -> Promise<string>       // 直接 fetch /api/ai/chat 读 SSE
ctx.ui.toast(msg)
```

验证：trunk 构建（`$env:CRAFT_I18N_DIR="d:\Works\my\craft-office\.kernel-src\craft-i18n-main"`）后浏览器直开，控制台确认 `window.__craftKernel`、doc-stats 命令可跑。

## 阶段 4：AI 助手迁插件 + 清理

- `backend/plugins/ai-assistant.js` 完整实现：原 ai_pane.rs 的 SYSTEM/历史/`<write>` 逻辑平移为 JS（约 120 行）；`registerPanel('assistant')`；流式期间逐增量 `panel.setState({messages, busy:true})`；Done 后解析 `<write>` → `ctx.doc.replaceSelected/insert`
- **删除原生 AI**：`ai_pane.rs`、`web/ai.rs`、`panes.rs::Assistant` 变体（12→11）、`ui.aiAssistant` 命令、`services.ai_chat` 钩子、`WordApp.ai_assist/ai_replies` 字段、review.rs AI 组
- Vue：`api/plugins.ts` 加 `enablePlugin` + installed 新类型；`PluginMarket.vue` 加启停开关 + changed 通知内核

## 验证（端到端）

1. `cargo check --workspace` + `cargo test -p wordcraft-ui-egui`（内核）
2. trunk 全量构建（带 CRAFT_I18N_DIR）→ 部署 backend/static/wordcraft（删旧哈希产物）
3. Playwright（chrome-headless-shell，坐标系数 1.40625）：打开 /office 示例文档 → 帮助菜单见「AI 助手」「字数统计」插件命令 → 字数统计出结果 → AI 助手开出右侧插件面板 → 选中文本输入「润色这段话」→ 流式回复 → 文档选区被替换 → 市场卸载插件后命令消失
4. 回归：原生 12 窗格（删 Assistant 后 11）启停/停靠/浮动

## 风险点

- `Pane::Plugin(u16)` 句柄单调分配不复用，插件重载换句柄 → 面板强制关闭（预期行为，UI 提示）
- 桥请求下一帧才执行，ctx API 全 Promise 化；防插件回调里无限 setState 触发永久重绘
- installed.json 并发写 → tmp+rename；主线程执行无沙箱 → 市场文档声明「插件=可信代码」
- 删原生 AI 窗格牵动 swap 穷举/测试，放最后独立阶段，编译器逐个点名
