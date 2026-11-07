//! 插件运行时（宿主层）：装载后端市场里已启用的插件 JS（`new Function` 直执行）。
//! 插件 → 内核走 `window.__craftKernelSend`（请求进队列，[`PluginRuntime::pump`] 每帧排空，
//! 回复经 `__craftKernelOnReply` 兑现 Promise）；注册事件进 [`PluginEvents`] 由内核逐帧应用。
//! 内核 → 插件（命令执行、面板输入）经 `window.__craftPluginDispatch` 直接分发；
//! 市场/宿主页启停变更后向本页 postMessage `wordcraft:pluginsChanged` 触发重载。

use std::cell::RefCell;
use std::rc::Rc;

use serde_json::{Value, json};
use wasm_bindgen::JsCast as _;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsValue;
use wordcraft_ui_egui::plugins::{PluginEvent, PluginEvents};
use wordcraft_ui_egui::{WordApp, control};

/// 当前内核名：manifest `apps` 声明按此过滤（缺省/空 = 全应用可用）。
const KERNEL: &str = "wordcraft";

/// 内核页面里的插件桥：Promise 化 send/reply、ctx 工厂、分发器与装载器。
const BOOTSTRAP: &str = r#"
(function () {
  if (window.__craftPluginsBridge) return;
  window.__craftPluginsBridge = true;
  var pending = {};       // 请求 id -> {resolve, reject}
  var seq = 0;
  var byId = {};          // 插件 id -> {name, commands:{}, panels:{}}
  var commands = {};      // 命令 id -> {plugin, run}
  var panelSends = {};    // 面板 id -> fn(text)

  function send(channel, params) {
    return new Promise(function (resolve, reject) {
      var id = 'r' + (++seq);
      pending[id] = { resolve: resolve, reject: reject };
      window.__craftKernelSend(JSON.stringify({ id: id, channel: channel, params: params || {} }));
    });
  }
  window.__craftKernelOnReply = function (raw) {
    var r = JSON.parse(raw);
    var p = pending[r.id];
    if (!p) return;
    delete pending[r.id];
    if (r.ok) p.resolve(r.result); else p.reject(new Error(r.error || 'kernel error'));
  };

  // 全量 announce：命令与面板列表进注册表（内核按 id 保持面板句柄）。
  function announce(pluginId) {
    var entry = byId[pluginId];
    if (!entry) return;
    var list = function (m) { return Object.keys(m).map(function (k) { return m[k]; }); };
    send('plugins.commands', { plugin: pluginId, commands: list(entry.commands) });
    send('plugins.panels', { plugin: pluginId, panels: list(entry.panels) });
  }

  // OpenAI 兼容 SSE：攒 delta.content，结束兑成完整文本；onChunk 逐块回调（流式渲染）。
  function aiChat(prompt, onChunk) {
    var messages = typeof prompt === 'string' ? [{ role: 'user', content: prompt }] : prompt;
    return fetch('/api/ai/chat', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ messages: messages, stream: true })
    }).then(function (resp) {
      if (!resp.ok) throw new Error('AI 网关返回 HTTP ' + resp.status);
      var reader = resp.body.getReader();
      var decoder = new TextDecoder();
      var buf = '';
      var full = '';
      function step() {
        return reader.read().then(function (r) {
          if (r.done) return full;
          buf += decoder.decode(r.value, { stream: true });
          var at;
          while ((at = buf.indexOf('\n\n')) >= 0) {
            var event = buf.slice(0, at + 2);
            buf = buf.slice(at + 2);
            event.split('\n').forEach(function (line) {
              if (line.indexOf('data:') !== 0) return;
              var data = line.slice(5).trim();
              if (!data || data === '[DONE]') return;
              var v;
              try { v = JSON.parse(data); } catch (e) { return; }
              if (v.error) throw new Error(v.error.message || '上游返回错误');
              var delta = v.choices && v.choices[0] && v.choices[0].delta && v.choices[0].delta.content;
              if (delta) {
                full += delta;
                if (onChunk) { try { onChunk(delta); } catch (e) {} }
              }
            });
          }
          return step();
        });
      }
      return step();
    });
  }

  function makeCtx(pluginId) {
    return {
      id: pluginId,
      registerCommand: function (spec) {
        byId[pluginId].commands[spec.id] = { id: spec.id, title: spec.title };
        commands[spec.id] = { plugin: pluginId, run: spec.run };
        announce(pluginId);
      },
      registerPanel: function (spec) {
        byId[pluginId].panels[spec.id] = { id: spec.id, title: spec.title, defaultWidth: spec.defaultWidth };
        panelSends[spec.id] = spec.onSend || function () {};
        announce(pluginId);
      },
      panel: {
        open: function (panelId) { send('panel.open', { panel: panelId }); },
        setState: function (panelId, state) { send('panel.state', { panel: panelId, state: state || {} }); }
      },
      doc: {
        getText: function () { return send('document.text', {}).then(function (r) { return r.selection ? '' : r.text; }); },
        getSelectedText: function () { return send('document.text', {}).then(function (r) { return r.selection ? r.text : ''; }); },
        insert: function (text) { return send('engine.execute', { command: 'edit.paste', params: { text: text } }); },
        replaceSelected: function (text) { return send('engine.execute', { command: 'edit.paste', params: { text: text } }); },
        execute: function (command, params) { return send('engine.execute', { command: command, params: params || {} }); },
        pages: function () { return send('document.pages', {}); }
      },
      ai: { chat: function (messages, onChunk) { return aiChat(messages, onChunk); } },
      ui: {
        toast: function (message) { send('ui.toast', { message: String(message) }); },
        dialog: function (title, text) { send('ui.dialog', { title: String(title), text: String(text) }); }
      }
    };
  }

  window.__craftPluginDispatch = function (raw) {
    var m = JSON.parse(raw);
    if (m.channel === 'command.run') {
      var cmd = commands[m.params.id];
      if (cmd && typeof cmd.run === 'function') {
        Promise.resolve().then(cmd.run).catch(function (e) {
          send('plugin.error', { plugin: cmd.plugin, message: String((e && e.message) || e) });
        });
      }
    } else if (m.channel === 'panel.send') {
      var onSend = panelSends[m.params.panel];
      if (onSend) onSend(m.params.text);
    }
  };

  window.__craftPluginLoad = function (specsJson) {
    var specs = JSON.parse(specsJson);
    byId = {}; commands = {}; panelSends = {};
    send('plugins.ready', {});
    specs.forEach(function (spec) { send('plugins.enabled', { id: spec.id, enabled: true }); });
    var chain = Promise.resolve();
    specs.forEach(function (spec) {
      chain = chain.then(function () {
        return fetch('/api/plugins/file?id=' + encodeURIComponent(spec.id)).then(function (r) {
          if (!r.ok) throw new Error('HTTP ' + r.status);
          return r.text();
        }).then(function (code) {
          byId[spec.id] = { name: spec.name, commands: {}, panels: {} };
          new Function('ctx', code)(makeCtx(spec.id));
          announce(spec.id);
        }).catch(function (e) {
          send('plugin.error', { plugin: spec.id, message: String((e && e.message) || e) });
        });
      });
    });
  };
})();
"#;

/// 宿主侧运行时：请求/回复队列 + 内核事件队列；见 [`start`] 与 [`PluginRuntime::pump`]。
pub struct PluginRuntime {
    requests: Rc<RefCell<Vec<String>>>,
    replies: Rc<RefCell<Vec<String>>>,
    events: PluginEvents,
    /// 各启用插件 manifest 声明的 hideTabs 并集（内核 tab 英文原名）；pump 每帧同步进
    /// `app.services.hidden_tabs`，内核据此隐藏 ribbon 页。
    hidden_tabs: Rc<RefCell<Vec<String>>>,
}

impl PluginRuntime {
    /// 装桥、监听市场变更并做首次装载。`events` 即 `WordApp::plugin_events`（同一 Arc）。
    pub fn start(events: PluginEvents, ctx: egui::Context) -> Rc<PluginRuntime> {
        let requests: Rc<RefCell<Vec<String>>> = Default::default();
        let replies: Rc<RefCell<Vec<String>>> = Default::default();
        let hidden_tabs: Rc<RefCell<Vec<String>>> = Default::default();
        let runtime = Rc::new(PluginRuntime { requests: requests.clone(), replies, events, hidden_tabs });
        let Some(window) = web_sys::window() else { return runtime };

        // 插件 JS → 内核的 send：一行 JSON 进请求队列，下一帧 pump 排空。
        let queue = requests.clone();
        let repaint = ctx.clone();
        let on_send = Closure::<dyn FnMut(String)>::new(move |raw: String| {
            queue.borrow_mut().push(raw);
            repaint.request_repaint();
        });
        let _ = js_sys::Reflect::set(&window, &JsValue::from_str("__craftKernelSend"), on_send.as_ref().unchecked_ref());
        // 桥与监听活到页面结束（与 host.rs 的 message 监听同一做法）。
        on_send.forget();

        let _ = js_sys::eval(BOOTSTRAP);

        // 宿主页/市场面板改了启停：postMessage 通知本页重载。
        let runtime_for_msgs = runtime.clone();
        let repaint = ctx.clone();
        let on_message = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |event: web_sys::MessageEvent| {
            if let Some(text) = event.data().as_string()
                && text == "wordcraft:pluginsChanged"
            {
                runtime_for_msgs.load();
                repaint.request_repaint();
            }
        });
        if window.add_event_listener_with_callback("message", on_message.as_ref().unchecked_ref()).is_ok() {
            on_message.forget();
        }

        runtime.load();
        runtime
    }

    /// 取市场清单与已安装状态，把启用的插件交给 `__craftPluginLoad` 装载。
    pub fn load(&self) {
        let hidden_tabs = self.hidden_tabs.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let specs = match fetch_json("/api/plugins").await {
                Ok(v) => {
                    let installed = v.get("installed").and_then(Value::as_array).cloned().unwrap_or_default();
                    let market = v.get("market").and_then(Value::as_array).cloned().unwrap_or_default();
                    installed
                        .iter()
                        .filter(|entry| entry.get("enabled").and_then(Value::as_bool).unwrap_or(false))
                        .filter_map(|entry| entry.get("id").and_then(Value::as_str))
                        .filter_map(|id| market.iter().find(|m| m.get("id").and_then(Value::as_str) == Some(id)))
                        // manifest apps 声明：缺省/空数组 = 全应用；否则需包含当前内核名。
                        .filter(|m| match m.get("apps").and_then(Value::as_array) {
                            Some(apps) if !apps.is_empty() => apps.iter().filter_map(Value::as_str).any(|a| a == KERNEL),
                            _ => true,
                        })
                        .map(|m| {
                            json!({
                                "id": m.get("id").and_then(Value::as_str).unwrap_or_default(),
                                "name": m.get("name").and_then(Value::as_str).unwrap_or_default(),
                                "hideTabs": m.get("hideTabs").cloned().unwrap_or(json!([])),
                            })
                        })
                        .collect::<Vec<_>>()
                }
                Err(e) => {
                    log::error!("plugins: 市场清单拉取失败：{e}");
                    Vec::new()
                }
            };
            // hideTabs 并集（File 不允许隐藏，这里与内核双重防护）。
            let mut hidden: Vec<String> = specs
                .iter()
                .filter_map(|s| s.get("hideTabs").and_then(Value::as_array))
                .flatten()
                .filter_map(Value::as_str)
                .filter(|t| *t != "File")
                .map(str::to_string)
                .collect();
            hidden.sort();
            hidden.dedup();
            *hidden_tabs.borrow_mut() = hidden;
            call_js("__craftPluginLoad", &json!(specs).to_string());
        });
    }

    /// 内核 → 插件（`Services::plugin_invoke`）：命令执行与面板输入。
    pub fn invoke(&self, channel: &str, params: Value) {
        call_js("__craftPluginDispatch", &json!({"channel": channel, "params": params}).to_string());
    }

    /// 每帧排空：请求分发进内核（control/事件/命令），回复兑现插件的 Promise。
    pub fn pump(&self, app: &mut WordApp, ctx: &egui::Context) {
        // 插件声明的隐藏 tab 同步进内核（装载晚于首帧，需每帧跟随）。
        app.services.hidden_tabs = self.hidden_tabs.borrow().clone();
        for raw in std::mem::take(&mut *self.requests.borrow_mut()) {
            let Ok(Value::Object(req)) = serde_json::from_str::<Value>(&raw) else { continue };
            let id = req.get("id").and_then(Value::as_str).unwrap_or_default().to_string();
            let channel = req.get("channel").and_then(Value::as_str).unwrap_or_default().to_string();
            let params = req.get("params").cloned().unwrap_or(json!({}));
            let reply = self.route(app, ctx, &channel, params);
            let body = match reply {
                Ok(result) => json!({"id": id, "ok": true, "result": result}),
                Err(error) => json!({"id": id, "ok": false, "error": error}),
            };
            self.replies.borrow_mut().push(body.to_string());
        }
        let replies = std::mem::take(&mut *self.replies.borrow_mut());
        for reply in replies {
            call_js("__craftKernelOnReply", &reply);
        }
    }

    /// 一个请求的内核侧处理：文档/命令走 control（同步出值），其余转事件队列。
    /// `Ok` 携带给插件的 result 载荷。
    fn route(&self, app: &mut WordApp, ctx: &egui::Context, channel: &str, params: Value) -> Result<Value, String> {
        // 远程操作期间收起浮动格式工具栏（AI 选中目标时不再弹出）。
        app.note_remote_activity(ctx);
        match channel {
            // control::handle 的 Outcome::Done 已是 {"ok", "result"/"error"} 形状。
            "engine.execute" | "document.text" | "document.pages" => {
                let (req, _rx) = control::ControlRequest::new(channel, params);
                match control::handle(app, ctx, &req) {
                    control::Outcome::Done(v) => {
                        if v.get("ok").and_then(Value::as_bool).unwrap_or(false) {
                            Ok(v.get("result").cloned().unwrap_or(json!({})))
                        } else {
                            Err(v.get("error").and_then(Value::as_str).unwrap_or("unknown error").to_string())
                        }
                    }
                    control::Outcome::Screenshot { .. } => Err("screenshot is not supported here".into()),
                }
            }
            // 打开面板走内核命令（注册表按面板 id 查句柄）。
            "panel.open" => {
                let panel = params.get("panel").and_then(Value::as_str).unwrap_or_default();
                app.execute("ui.plugin.openPane", json!({"panel": panel})).map(|_| json!({}))
            }
            "plugins.ready" => {
                self.push(PluginEvent::Ready);
                Ok(json!({}))
            }
            "plugins.enabled" => {
                let id = params.get("id").and_then(Value::as_str).unwrap_or_default().to_string();
                let enabled = params.get("enabled").and_then(Value::as_bool).unwrap_or(true);
                self.push(PluginEvent::Enabled { id, enabled });
                Ok(json!({}))
            }
            "plugins.commands" => {
                let plugin = params.get("plugin").and_then(Value::as_str).unwrap_or_default().to_string();
                let items = params.get("commands").cloned().unwrap_or(json!([]));
                let commands = serde_json::from_value(items).map_err(|e| format!("plugins.commands 解析失败：{e}"))?;
                self.push(PluginEvent::Commands { plugin, commands });
                Ok(json!({}))
            }
            "plugins.panels" => {
                let plugin = params.get("plugin").and_then(Value::as_str).unwrap_or_default().to_string();
                let items = params.get("panels").cloned().unwrap_or(json!([]));
                let panels = serde_json::from_value(items).map_err(|e| format!("plugins.panels 解析失败：{e}"))?;
                self.push(PluginEvent::Panels { plugin, panels });
                Ok(json!({}))
            }
            "panel.state" => {
                let panel = params.get("panel").and_then(Value::as_str).unwrap_or_default().to_string();
                let state = params.get("state").cloned().unwrap_or(json!({}));
                self.push(PluginEvent::PanelState { panel, state });
                Ok(json!({}))
            }
            "ui.toast" => {
                self.push(PluginEvent::Toast { message: params.get("message").and_then(Value::as_str).unwrap_or_default().to_string() });
                Ok(json!({}))
            }
            "ui.dialog" => {
                self.push(PluginEvent::Dialog {
                    title: params.get("title").and_then(Value::as_str).unwrap_or("插件").to_string(),
                    text: params.get("text").and_then(Value::as_str).unwrap_or_default().to_string(),
                });
                Ok(json!({}))
            }
            "plugin.error" => {
                self.push(PluginEvent::Error {
                    plugin: params.get("plugin").and_then(Value::as_str).unwrap_or_default().to_string(),
                    message: params.get("message").and_then(Value::as_str).unwrap_or_default().to_string(),
                });
                Ok(json!({}))
            }
            other => Err(format!("未知通道：{other}")),
        }
    }

    fn push(&self, event: PluginEvent) {
        self.events.lock().unwrap_or_else(|e| e.into_inner()).push(event);
    }
}

/// GET 一段同源 JSON。
async fn fetch_json(url: &str) -> Result<Value, String> {
    let js = |e: JsValue| e.as_string().unwrap_or_else(|| format!("{e:?}"));
    let window = web_sys::window().ok_or("no window")?;
    let value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_str(url)).await.map_err(js)?;
    let response = value.dyn_into::<web_sys::Response>().map_err(|_| "响应类型异常".to_string())?;
    if !response.ok() {
        return Err(format!("HTTP {}", response.status()));
    }
    let text = wasm_bindgen_futures::JsFuture::from(response.text().map_err(js)?).await.map_err(js)?;
    serde_json::from_str(&text.as_string().unwrap_or_default()).map_err(|e| format!("JSON 解析失败：{e}"))
}

/// POST 一段同源 JSON（如启停插件写 installed.json）。
pub(crate) async fn post_json(url: &str, body: &str) -> Result<(), String> {
    let js = |e: JsValue| e.as_string().unwrap_or_else(|| format!("{e:?}"));
    let window = web_sys::window().ok_or("no window")?;
    let mut init = web_sys::RequestInit::new();
    init.method("POST");
    init.body(Some(&JsValue::from_str(body)));
    let request = web_sys::Request::new_with_str_and_init(url, &init).map_err(js)?;
    request.headers().set("Content-Type", "application/json").ok();
    let value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request_and_init(&request, &init)).await.map_err(js)?;
    let response = value.dyn_into::<web_sys::Response>().map_err(|_| "响应类型异常".to_string())?;
    if !response.ok() {
        return Err(format!("HTTP {}", response.status()));
    }
    Ok(())
}

/// 调一个桥上的全局函数（不存在的情形只记日志，桥未装好前的回复直接丢弃）。
fn call_js(name: &str, arg: &str) {
    if let Some(window) = web_sys::window()
        && let Ok(func) = js_sys::Reflect::get(&window, &JsValue::from_str(name))
        && func.is_function()
    {
        let _ = js_sys::Function::from(func).call1(&window, &JsValue::from_str(arg));
    }
}
