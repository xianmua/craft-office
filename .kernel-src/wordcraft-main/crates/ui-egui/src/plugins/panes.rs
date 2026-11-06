//! 插件面板的 per-window 运行时状态：每个窗口各自开着哪些插件面板，及其
//! 声明式状态（插件经 `panel.setState` 浅合并推送）与输入框文字。

use serde_json::Value;

/// 一个打开插件面板的运行时状态。
#[derive(Clone, Debug, Default)]
pub struct PanelState {
    /// 插件推送的声明式状态（消息流、busy、error 等），渲染约定见 `render`。
    pub state: Value,
    /// 底部输入框里的文字。
    pub prompt: String,
}

/// 一个窗口开着的插件面板：`(面板句柄, 状态)`。
#[derive(Clone, Debug, Default)]
pub struct Panes {
    pub open: Vec<(u16, PanelState)>,
}

impl Panes {
    pub fn is_open(&self, handle: u16) -> bool {
        self.open.iter().any(|(h, _)| *h == handle)
    }

    pub fn open(&mut self, handle: u16) {
        if !self.is_open(handle) {
            self.open.push((handle, PanelState::default()));
        }
    }

    pub fn close(&mut self, handle: u16) {
        self.open.retain(|(h, _)| *h != handle);
    }

    pub fn get(&self, handle: u16) -> Option<&PanelState> {
        self.open.iter().find(|(h, _)| *h == handle).map(|(_, s)| s)
    }

    pub fn get_mut(&mut self, handle: u16) -> Option<&mut PanelState> {
        self.open.iter_mut().find(|(h, _)| *h == handle).map(|(_, s)| s)
    }

    /// 浅合并一条状态补丁：两侧都是对象时逐键覆盖，否则整体替换。
    pub fn apply_state(&mut self, handle: u16, patch: &Value) {
        let Some(slot) = self.open.iter_mut().find(|(h, _)| *h == handle) else { return };
        let state = &mut slot.1.state;
        if let (Some(map), Some(object)) = (state.as_object_mut(), patch.as_object()) {
            for (k, v) in object {
                map.insert(k.clone(), v.clone());
            }
        } else {
            *state = patch.clone();
        }
    }
}
