//! 通用声明式插件面板渲染：面板内容由插件经 `panel.setState` 推送的 JSON 决定，
//! 内核约定这几个字段：`messages: [{role, text}]` 消息流；`busy` 生成中（自动重绘）；
//! `error` 错误提示；`name` 助手名；`placeholder` 输入框提示。消息流为通栏纯文本
//! （无气泡）；生成中的末条助手消息实时增长并带光标。输入框默认 1 行（回车换行、
//! 随内容长高），Ctrl+回车或点「发送」提交；发送经
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
    let name = state.get("name").and_then(Value::as_str).unwrap_or("助手").to_string();
    ui.add_space(4.0);
    // 输入区默认 1 行高，随换行自动长高（最多 6 行，再多内部滚动），给消息流让位避免内容被盖住。
    let rows = (slot.prompt.matches('\n').count() + 1).clamp(1, 6);
    let input_height = rows as f32 * 15.0 + 16.0;
    let log_height = (ui.available_height() - input_height).max(60.0);
    ScrollArea::vertical().auto_shrink([false; 2]).stick_to_bottom(true).max_height(log_height).min_scrolled_height(60.0).show(ui, |ui| {
        if messages.is_empty() {
            ui.label(
                egui::RichText::new("用一句话下指令\n回复直接写进文档")
                    .font(regular(11.0))
                    .color(t.text_dim),
            );
            return;
        }
        let last = messages.len() - 1;
        for (i, message) in messages.iter().enumerate() {
            let role = message.get("role").and_then(Value::as_str).unwrap_or("assistant");
            let text = message.get("text").and_then(Value::as_str).unwrap_or_default();
            let author = if role == "user" { "你" } else { name.as_str() };
            ui.label(egui::RichText::new(author).font(regular(10.0)).color(t.text_dim));
            // 流式末条：空文本先给省略号，有内容后缀光标；其余按原文展示。
            let streaming = busy && i == last && role != "user";
            let shown = if streaming {
                if text.is_empty() { "…".to_string() } else { format!("{text} |") }
            } else {
                text.to_string()
            };
            if shown.is_empty() {
                ui.label(egui::RichText::new(" ").font(regular(10.0)));
            } else {
                ui.add(egui::Label::new(egui::RichText::new(&shown)).wrap());
            }
            ui.add_space(10.0);
        }
    });
    ui.add_space(2.0);
    ui.separator();
    ui.add_space(4.0);
    let placeholder = state.get("placeholder").and_then(Value::as_str).unwrap_or("输入内容发给插件…");
    let mut send = false;
    ui.horizontal(|ui| {
        let response = ui.add(
            egui::TextEdit::multiline(&mut slot.prompt)
                .desired_width(ui.available_width() - 52.0)
                .desired_rows(rows)
                .char_limit(4000)
                .hint_text(placeholder),
        );
        // 回车换行；Ctrl+回车或点「发送」提交，发送后保持输入焦点。
        send = (response.has_focus()
            && ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.command))
            || ui
                .add_enabled(
                    !busy && !slot.prompt.trim().is_empty(),
                    egui::Button::new("发送"),
                )
                .clicked();
        if send {
            response.request_focus();
        }
    });
    if send && !busy && !slot.prompt.trim().is_empty() {
        // 载荷带面板 id（非句柄），与 `ui.plugin.panelSend`、插件侧约定一致。
        let panel_id = app.plugins.panel(handle).map(|s| s.id.clone());
        if let (Some(invoke), Some(panel)) = (app.services.plugin_invoke.as_ref(), panel_id) {
            invoke("panel.send", serde_json::json!({"panel": panel, "text": slot.prompt.trim()}));
        }
        slot.prompt.clear();
    }
    // 状态行只放状态词。
    if busy {
        ui.add_space(2.0);
        ui.label(egui::RichText::new("生成中").font(regular(10.0)).color(t.text_dim));
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
    }
    if !error.is_empty() {
        ui.add_space(4.0);
        ui.colored_label(ui.visuals().error_fg_color, &error);
    }
}
