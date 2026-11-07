// AI 助手插件：对话面板。提示词外置在 agent.md（经 /api/plugins/file?file= 拉取并缓存）。
// 写回协议：<write>纯文本</write> 替换选区或插入光标处；<cmd>[{"command":..}]</cmd> 经
// ctx.doc.execute 按序执行内核命令；全文上下文带 [行号] 前缀（行号=block），支持「第 x 行加粗」类定位。
var history = []; // OpenAI 兼容历史（不含 system），一轮两条
var busy = false;
var promptCache = null;
var state = {
  messages: [],
  busy: false,
  name: "AI 助手",
  placeholder: "输入指令，如：帮我在第 2 行把速度加粗",
};

function render() {
  ctx.panel.setState("assistant", state);
}

// 提示词懒加载并缓存；失败时抛错进面板 error 区。
function systemPrompt() {
  if (promptCache) return Promise.resolve(promptCache);
  return fetch("/api/plugins/file?id=ai-assistant&file=agent.md")
    .then(function (r) {
      if (!r.ok) throw new Error("agent.md 加载失败 HTTP " + r.status);
      return r.text();
    })
    .then(function (t) {
      promptCache = t;
      return t;
    });
}

// 全文加 [行号] 前缀（行号=block），供模型按行定位。
function numberLines(text) {
  return text
    .split("\n")
    .map(function (line, i) {
      return "[" + i + "] " + line;
    })
    .join("\n");
}

// 页映射压成「页:起始block」列表，随全文上下文给模型。
function pageMap(pages) {
  if (!pages || !Array.isArray(pages.first)) return "";
  return pages.first
    .map(function (b, i) {
      return i + 1 + ":" + b;
    })
    .join(",");
}

// 剥掉 <write>/<cmd> 标记后的展示文本。
function displayText(reply) {
  return reply
    .replace(/<cmd>[\s\S]*?<\/cmd>/g, "")
    .replace(/<write>[\s\S]*?<\/write>/g, function (m) {
      return m.replace(/<\/?write>/g, "").trim();
    })
    .trim();
}

// 流式过程中的展示文本：剥完整标记块，再剥截断的半个标签，避免闪烁。
function displayLive(reply) {
  return reply
    .replace(/<cmd>[\s\S]*?<\/cmd>/g, "")
    .replace(/<write>/g, "\n")
    .replace(/<\/write>/g, "")
    .replace(/<\/?[a-z]*$/, "")
    .trim();
}

function writeContent(reply) {
  var m = reply.match(/<write>([\s\S]*?)<\/write>/);
  return m ? m[1] : null;
}

// 解析全部 <cmd> 块为按序执行的命令列表。
function parseCmds(reply) {
  var out = [];
  reply.replace(/<cmd>([\s\S]*?)<\/cmd>/g, function (m, body) {
    try {
      var arr = JSON.parse(body);
      if (Array.isArray(arr)) out = out.concat(arr);
    } catch (e) {}
    return m;
  });
  return out;
}

function runCmds(cmds) {
  var chain = Promise.resolve();
  cmds.forEach(function (c) {
    chain = chain.then(function () {
      return ctx.doc.execute(c.command, (c && c.params) || {});
    });
  });
  return chain;
}

var hadSelection = false; // 本次回答是否有选区上下文：决定写回是替换还是插入

function handle(text) {
  console.log('[ai-assistant] handle:', text, 'busy=', busy)
  if (busy) return;
  busy = true;
  state.busy = true;
  state.error = "";
  state.messages.push({ role: "user", text: text });
  var live = state.messages.push({ role: "assistant", text: "" }) - 1; // 占位助手消息下标，流式更新
  render();

  systemPrompt()
    .then(function (system) {
      return Promise.all([ctx.doc.getSelectedText(), ctx.doc.getText(), ctx.doc.pages()]).then(
        function (parts) {
          var selection = parts[0].trim();
          hadSelection = !!selection;
          var user = text;
          if (selection) {
            user += "\n\n【参考内容（选区）】\n" + selection.slice(0, 6000);
          } else if (parts[1].trim()) {
            user +=
              "\n\n【参考内容（全文）】\n" +
              numberLines(parts[1]).slice(0, 6000) +
              "\n【分页】共 " +
              parts[2].count +
              " 页，页:起始block = " +
              pageMap(parts[2]);
          }
          history.push({ role: "user", content: user });
          var messages = [{ role: "system", content: system }].concat(history.slice(-8));
          var full = "";
          return ctx.ai.chat(messages, function (delta) {
            full += delta;
            state.messages[live].text = displayLive(full);
            render();
          });
        },
      );
    })
    .then(function (reply) {
      history.push({ role: "assistant", content: reply });
      var content = writeContent(reply);
      var cmds = parseCmds(reply);
      var write =
        content === null
          ? Promise.resolve()
          : hadSelection
            ? ctx.doc.replaceSelected(content)
            : ctx.doc.insert(content);
      // 先写回文本，再按序执行命令（定位由命令自行完成）。
      function finish() {
        state.messages[live].text = displayText(reply);
        busy = false;
        state.busy = false;
        render();
      }
      return write
        .then(function () {
          return runCmds(cmds);
        })
        .then(finish, function (e) {
          state.error = String((e && e.message) || e);
          finish();
        });
    })
    .catch(function (e) {
      state.messages[live].text = "";
      busy = false;
      state.busy = false;
      state.error = String((e && e.message) || e);
      render();
    });
}

ctx.registerPanel({
  id: "assistant",
  title: "AI 助手",
  defaultWidth: 340,
  onSend: function (text) {
    handle(text);
  },
});
ctx.registerCommand({
  id: "ai-assistant.open",
  title: "AI 助手",
  run: function () {
    ctx.panel.open("assistant");
  },
});
