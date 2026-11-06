//! 通用声明式插件面板渲染：面板内容由插件经 `panel.setState` 推送的 JSON 决定，
//! 内核约定这几个字段：`messages: [{role, text}]` 消息流；`busy` 生成中（自动重绘）；
//! `error` 错误提示；`placeholder` 输入框提示。底部输入框固定存在；发送经
//! `Services::plugin_invoke("panel.send", …)` 交回插件。

use egui::{ScrollArea, Ui};
use serde_json::Value;

use crate::WordApp;
use crate::theme::{Tokens, regular};

pub(crate) fn panel_ui(app: &mut WordApp, ui: &mut Ui, handle: u16) {
    let Some(slot) = app.plugin_panes.get_mut(handle) else {
        return;
    };
    let t = Tokens::get(ui.ctx());
    let state = slot.state.clone();
    let messages = state.get("messages").and_then(Value::as_array).cloned().unwrap_or_default();
    let busy = state.get("busy").and_then(Value::as_bool).unwrap_or(false);
    let error = state.get("error").and_then(Value::as_str).unwrap_or_default().to_string();
    ui.add_space(2.0);
    let input_height = 90.0;
    let log_height = (ui.available_height() - input_height).max(60.0);
    ScrollArea::vertical().auto_shrink([false; 2]).stick_to_bottom(true).max_height(log_height).min_scrolled_height(60.0).show(ui, |ui| {
        if messages.is_empty() {
            ui.label(egui::RichText::new("插件面板").font(regular(11.0)).color(t.text_dim));
            return;
        }
        for message in &messages {
            let role = message.get("role").and_then(Value::as_str).unwrap_or("assistant");
            let text = message.get("text").and_then(Value::as_str).unwrap_or_default();
            let author = if role == "user" { "你" } else { "插件" };
            ui.label(egui::RichText::new(author).font(regular(11.0)).color(t.text_dim));
            if text.is_empty() {
                ui.label(egui::RichText::new("正在生成…").color(t.text_dim));
            } else {
                ui.add(egui::Label::new(egui::RichText::new(text)).wrap());
            }
            ui.add_space(8.0);
        }
    });
    ui.add_space(2.0);
    ui.separator();
    ui.add_space(6.0);
    let placeholder = state.get("placeholder").and_then(Value::as_str).unwrap_or("输入内容发给插件…");
    let prompt = &mut slot.prompt;
    ui.add(egui::TextEdit::multiline(prompt).desired_rows(2).char_limit(4000).hint_text(placeholder));
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let text = slot.prompt.trim().to_string();
        if ui.add_enabled(!busy && !text.is_empty(), egui::Button::new("发送")).clicked() {
            // 载荷带面板 id（非句柄），与 `ui.plugin.panelSend`、插件侧约定一致。
            let panel_id = app.plugins.panel(handle).map(|s| s.id.clone());
            if let (Some(invoke), Some(panel)) = (app.services.plugin_invoke.as_ref(), panel_id) {
                invoke("panel.send", serde_json::json!({"panel": panel, "text": text}));
            }
            slot.prompt.clear();
        }
        if busy {
            ui.label(egui::RichText::new("生成中…").font(regular(11.0)).color(t.text_dim));
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
        }
    });
    if !error.is_empty() {
        ui.add_space(4.0);
        ui.colored_label(ui.visuals().error_fg_color, &error);
    }
}
