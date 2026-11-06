//! App 级插件注册表：装载后的插件条目（命令 + 面板）。面板句柄单调递增不复用，
//! 插件重载后句柄作废，旧面板随之关闭（预期行为，见方案）。

use super::{CommandSpec, PanelSpec};

/// 一个已注册插件。
#[derive(Clone, Debug, Default)]
pub struct PluginEntry {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub commands: Vec<CommandSpec>,
    /// `(面板句柄, 声明)`；句柄在注册时分配。
    pub panels: Vec<(u16, PanelSpec)>,
}

#[derive(Default)]
pub struct Registry {
    pub plugins: Vec<PluginEntry>,
    /// 下一个面板句柄；单调递增，重载不复用。
    next_handle: u16,
    /// 插件管理器窗口是否开着（`ui.plugins`）。
    pub manager_open: bool,
    /// 待展示的插件弹窗（`ctx.ui.dialog`），关闭即丢弃。
    pub dialogs: Vec<(String, String)>,
}

impl Registry {
    /// 分配一个新面板句柄。
    fn allocate(&mut self) -> u16 {
        let h = self.next_handle;
        self.next_handle = self.next_handle.saturating_add(1);
        h
    }

    /// 条目下标，缺失则建一个（`name` 暂用 id）。
    fn ensure(&mut self, id: &str) -> usize {
        if let Some(at) = self.plugins.iter().position(|p| p.id == id) {
            return at;
        }
        self.plugins.push(PluginEntry { id: id.to_string(), name: id.to_string(), ..Default::default() });
        self.plugins.len() - 1
    }

    /// 重载：清空条目；句柄计数与管理器开合保留。
    pub fn clear(&mut self) {
        self.plugins.clear();
    }

    /// 一个插件声明了它的命令。
    pub fn set_commands(&mut self, id: &str, commands: Vec<CommandSpec>) {
        let at = self.ensure(id);
        self.plugins[at].commands = commands;
    }

    /// 一个插件声明了它的面板；为每个面板分配句柄（同 id 面板保持原句柄，
    /// 全量 announce 重复注册时不顶掉已开面板）。
    pub fn add_panels(&mut self, id: &str, panels: Vec<PanelSpec>) {
        let at = self.ensure(id);
        let old = std::mem::take(&mut self.plugins[at].panels);
        let slots = panels
            .into_iter()
            .map(|spec| {
                let handle = old.iter().find(|(_, s)| s.id == spec.id).map(|(h, _)| *h).unwrap_or_else(|| self.allocate());
                (handle, spec)
            })
            .collect::<Vec<_>>();
        self.plugins[at].panels = slots;
    }

    /// 启停确认回填。
    pub fn set_enabled(&mut self, id: &str, enabled: bool) {
        let at = self.ensure(id);
        self.plugins[at].enabled = enabled;
    }

    /// 句柄 → 面板声明。
    pub fn panel(&self, handle: u16) -> Option<&PanelSpec> {
        self.plugins.iter().flat_map(|p| p.panels.iter()).find(|(h, _)| *h == handle).map(|(_, spec)| spec)
    }

    /// 面板 id → 句柄（`ui.plugin.openPane` 用）。
    pub fn panel_handle(&self, id: &str) -> Option<u16> {
        self.plugins.iter().flat_map(|p| p.panels.iter()).find(|(_, spec)| spec.id == id).map(|(h, _)| *h)
    }

    /// 面板 id → 所属插件 id（`ui.plugin.panelSend` 用）。
    pub fn panel_plugin(&self, id: &str) -> Option<&str> {
        self.plugins.iter().find(|p| p.panels.iter().any(|(_, spec)| spec.id == id)).map(|p| p.id.as_str())
    }
}
