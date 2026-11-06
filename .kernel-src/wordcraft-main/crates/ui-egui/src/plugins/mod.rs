//! 内核插件管理器 + 插件 API（cordis 风格）：内核只内置通用注册表、动态面板、
//! 声明式渲染与管理器 UI；插件本体是宿主层（web）装载的 JS，经 [`crate::Services::plugin_load`]
//! 装载，注册与运行事件经 [`PluginEvent`] 队列逐帧送回内核（与 `Services::inbox` 同一队列模式）。
//! 内核 → 插件方向走 [`crate::Services::plugin_invoke`]。

mod manager;
mod panes;
mod registry;
mod render;

pub use panes::{PanelState, Panes};
pub use registry::{PluginEntry, Registry};
pub(crate) use manager::show;
pub(crate) use render::panel_ui;

/// 插件命令声明（帮助菜单出现，`ui.plugin.run` 执行）。
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandSpec {
    pub id: String,
    pub title: String,
}

/// 插件面板声明（内核以 `Pane::Plugin(handle)` 打开并渲染）。
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelSpec {
    pub id: String,
    pub title: String,
    /// 停靠默认宽度；缺省 300。
    #[serde(default)]
    pub default_width: Option<f32>,
}

/// 宿主 → 内核的一条插件事件（队列模式，见模块文档）。
pub enum PluginEvent {
    /// 插件运行时已（重新）装载：清空注册表等待重新注册；旧面板句柄作废即关闭。
    Ready,
    /// 一个插件声明了它的命令。
    Commands { plugin: String, commands: Vec<CommandSpec> },
    /// 一个插件声明了它的面板；句柄由内核分配后回填注册表。
    Panels { plugin: String, panels: Vec<PanelSpec> },
    /// 面板状态补丁（浅合并到 per-window 面板状态）；`panel` 是面板 id 字符串，
    /// 排空时经注册表解析句柄（装载当帧注册表可能尚未建好，晚解析更稳）。
    PanelState { panel: String, state: serde_json::Value },
    /// 插件启停确认（宿主写完 installed.json 并重载后回填注册表）。
    Enabled { id: String, enabled: bool },
    /// 一次性提示（插件经 `ctx.ui.toast` 请求）。
    Toast { message: String },
    /// 一次性弹窗（插件经 `ctx.ui.dialog` 请求），关闭即丢弃。
    Dialog { title: String, text: String },
    /// 插件报错。
    Error { plugin: String, message: String },
}

/// 事件队列：宿主 push，内核每帧排空（`WordApp::drain_plugins`）。
pub type PluginEvents = std::sync::Arc<std::sync::Mutex<Vec<PluginEvent>>>;
