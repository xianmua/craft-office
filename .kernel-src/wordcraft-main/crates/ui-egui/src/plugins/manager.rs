//! 内核插件管理器：每个已注册插件一行（启停复选），外加打开市场与重新加载。
//! 帮助菜单「插件管理」（`ui.plugins`）打开。启停经 `Services::plugin_set_enabled`
//! 交宿主（写 installed.json 并重载），确认经 `PluginEvent::Enabled` 回填注册表。

use egui::{Context, Ui};

use crate::WordApp;
use crate::theme::Tokens;

pub(crate) fn show(app: &mut WordApp, ctx: &Context) {
    // 弹窗与插件管理窗口相互独立，即便管理器没开也要照常渲染。
    if app.plugins.manager_open {
        let mut open = true;
        egui::Window::new("插件管理")
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| crate::rtl::root(ui, |ui| body(app, ui)));
        if !open {
            app.plugins.manager_open = false;
        }
    }
    dialogs(app, ctx);
}

/// 插件经 `ctx.ui.dialog` 请求的一次性弹窗：关闭即从队列丢弃。
fn dialogs(app: &mut WordApp, ctx: &Context) {
    if app.plugins.dialogs.is_empty() {
        return;
    }
    let pending = std::mem::take(&mut app.plugins.dialogs);
    app.plugins.dialogs = pending
        .into_iter()
        .filter(|(title, text)| {
            let mut open = true;
            egui::Window::new(title.clone()).open(&mut open).resizable(false).collapsible(false).show(ctx, |ui| {
                crate::rtl::root(ui, |ui| ui.add(egui::Label::new(egui::RichText::new(text.clone())).wrap()));
            });
            open
        })
        .collect();
}

fn body(app: &mut WordApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    if app.plugins.plugins.is_empty() {
        ui.label(egui::RichText::new("还没有已装载的插件.").color(t.text_dim));
        ui.add_space(6.0);
    }
    // 先取出快照再逐行画，避免闭包里再借 `app`。
    let snapshot: Vec<(String, String, bool)> =
        app.plugins.plugins.iter().map(|p| (p.id.clone(), if p.name.is_empty() { p.id.clone() } else { p.name.clone() }, p.enabled)).collect();
    for (id, name, enabled) in snapshot {
        let mut on = enabled;
        ui.horizontal(|ui| {
            if ui.add(egui::Checkbox::new(&mut on, &name)).changed() {
                if let Some(set) = app.services.plugin_set_enabled.as_ref() {
                    set(&id, on);
                }
                if let Some(at) = app.plugins.plugins.iter().position(|p| p.id == id) {
                    app.plugins.plugins[at].enabled = on;
                }
            }
            ui.label(egui::RichText::new(id).small().color(t.text_dim));
        });
    }
    ui.separator();
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        if let Some(market) = app.services.host_plugins.as_ref()
            && ui.button("打开插件市场").clicked()
        {
            market();
        }
        if let Some(load) = app.services.plugin_load.as_ref()
            && ui.button("重新加载").clicked()
        {
            load();
        }
    });
}
