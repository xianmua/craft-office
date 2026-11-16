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

// 命令目录懒加载（wordcraft-cli commands --json 导出，655 条含参数签名）；失败降级为空表。
var catalogCache = null;
function catalog() {
  if (catalogCache !== null) return Promise.resolve(catalogCache);
  return fetch("/api/plugins/file?id=ai-assistant&file=commands.json")
    .then(function (r) {
      if (!r.ok) throw new Error("commands.json 加载失败 HTTP " + r.status);
      return r.json();
    })
    .then(function (j) {
      catalogCache = Array.isArray(j) ? j : [];
      return catalogCache;
    })
    .catch(function () {
      catalogCache = [];
      return [];
    });
}

// 中文意图 → 命令 id 关键词：按用户消息命中情况注入对应命令族（id 子串匹配）。
var INTENTS = [
  ["表格|单元格|行高|列宽|合并|拆分", "table"],
  ["图片|照片|图像|截图", "picture"],
  ["页眉|页脚", "hf header footer"],
  ["页码", "pageNumber"],
  ["水印", "watermark"],
  ["批注|评论|注释", "comment"],
  ["修订|改痕|痕迹", "trackChanges tracking"],
  ["脚注|尾注", "footnote endnote citation bibliography"],
  ["目录", "toc"],
  ["引用|参考文献|书目", "citation bibliography references"],
  ["链接|超链接|网址", "link"],
  ["书签|交叉引用", "bookmark"],
  ["公式", "equation"],
  ["图表", "chart"],
  ["形状|图形", "shape smartArt"],
  ["艺术字", "wordArt"],
  ["文本框", "textBox"],
  ["封面", "coverPage"],
  ["字体|加粗|斜体|下划线|删除线|字号|颜色", "font format.char format.text"],
  ["段落|缩进|对齐|行距|间距", "para"],
  ["项目符号|编号|列表", "bullets numbering list"],
  ["样式|标题级别", "style para.style outline.level"],
  ["主题|设计|背景", "design theme"],
  ["查找|替换", "find replace"],
  ["撤销|重做", "undo redo"],
  ["打印|导出|另存为|PDF", "file.print file.export file.save"],
  ["页面|纸张|边距|横向|竖向|分栏|分页|分节", "layout"],
  ["选中|选区|选择", "select"],
  ["视图|缩放|窗口|阅读", "view"],
  ["审阅|校对|拼写", "review proof"],
  ["邮件|合并域", "mailings merge"],
  ["域|日期|时间|自动更新", "fields"],
  ["绘图|手绘|墨迹", "draw"],
];

// 按意图从目录挑命令拼成紧凑清单；上限 140 条防 prompt 膨胀。
function commandRef(text) {
  if (!catalogCache || !catalogCache.length) return "";
  var keys = [];
  INTENTS.forEach(function (row) {
    if (new RegExp(row[0]).test(text)) keys = keys.concat(row[1].split(" "));
  });
  if (!keys.length) return "";
  var lines = [];
  for (var i = 0; i < catalogCache.length && lines.length < 140; i++) {
    var c = catalogCache[i];
    var id = c.id.toLowerCase();
    for (var k = 0; k < keys.length; k++) {
      if (id.indexOf(keys[k].toLowerCase()) >= 0) {
        lines.push("- " + c.id + " " + String(c.params || "{}").replace(/\s+/g, " ").slice(0, 110) + " // " + c.label);
        break;
      }
    }
  }
  if (!lines.length) return "";
  return "\n\n【本条消息可用命令】仅列与本请求相关的命令，需执行时输出 <cmd>，格式 {\"command\": id, \"params\": {…}}；清单外没有的命令不要编造：\n" + lines.join("\n");
}

// 技能指南：按当前内核加载 skills/<目录>/SKILL.md（wordcraft→word、gridcraft→excel、deckcraft→pptx），
// 每会话只拉取一次并缓存。excel/pptx 的命令细节待阶段2c 导出命令目录后回填。
var KERNEL_SKILL = { wordcraft: "word", gridcraft: "excel", deckcraft: "pptx" };
var skillCache = null;
function kernelSkill() {
  if (!skillCache) {
    var kernel = (location.pathname.match(/wordcraft|gridcraft|deckcraft/) || [])[0] || "wordcraft";
    skillCache = fetch("/api/plugins/file?id=ai-assistant&file=skills/" + (KERNEL_SKILL[kernel] || "word") + "/SKILL.md")
      .then(function (r) {
        return r.ok ? r.text() : null;
      })
      .catch(function () {
        return null;
      });
  }
  return skillCache;
}

// 技能文本包上【技能指南】段引；拉取失败返回空串不阻塞对话。
function wrapSkill(doc) {
  return doc ? "\n\n【技能指南】\n\n" + doc : "";
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
    .replace(/<write[^>]*>[\s\S]*?<\/write>/g, function (m) {
      return m.replace(/<\/?write[^>]*>/g, "").trim();
    })
    .trim();
}

// 流式过程中的展示文本：剥完整标记块，再剥截断的半个标签，避免闪烁。
function displayLive(reply) {
  return reply
    .replace(/<cmd>[\s\S]*?<\/cmd>/g, "")
    .replace(/<write[^>]*>/g, "\n")
    .replace(/<\/write>/g, "")
    .replace(/<\/?[a-z]*$/, "")
    .trim();
}

function writeContent(reply) {
  var m = reply.match(/<write([^>]*)>([\s\S]*?)<\/write>/);
  if (!m) return null;
  var page = (m[1].match(/page="(\d+)"/) || [])[1];
  var pos = (m[1].match(/pos="(start|end)"/) || [])[1] || "start";
  return { text: m[2], page: page ? parseInt(page, 10) : null, pos: pos };
}

// 最近一次上下文的分页数据与各行文本：宿主自己算页定位 off，模型不算字符数。
var lastPages = null;
var lastBlocks = [];

// <write page pos> → 精确光标 {block, off}；数据缺失或页码越界返回 null。
function pageTarget(w) {
  if (!w.page || !lastPages || !Array.isArray(lastPages.first)) return null;
  var first = lastPages.first;
  var count = lastPages.count || first.length;
  if (w.page < 1 || w.page > count) return null;
  if (w.pos === "start") return { block: first[w.page - 1], off: 0 };
  var endBlock = w.page < count ? first[w.page] - 1 : lastBlocks.length - 1;
  return { block: endBlock, off: (lastBlocks[endBlock] || "").length };
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
  if (busy) return;
  busy = true;
  state.busy = true;
  state.error = "";
  state.messages.push({ role: "user", text: text });
  var live = state.messages.push({ role: "assistant", text: "" }) - 1; // 占位助手消息下标，流式更新
  render();

  catalog()
    .then(function () {
      return Promise.all([systemPrompt(), kernelSkill().then(wrapSkill)]);
    })
    .then(function (loaded) {
      var system = loaded[0];
      return Promise.all([ctx.doc.getSelectedText(), ctx.doc.getText(), ctx.doc.pages()]).then(
        function (parts) {
          var selection = parts[0].trim();
          hadSelection = !!selection;
          lastPages = parts[2];
          lastBlocks = parts[1] ? parts[1].split("\n") : [];
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
          var messages = [{ role: "system", content: system + loaded[1] + commandRef(text) }].concat(history.slice(-8));
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
      var w = writeContent(reply);
      var cmds = parseCmds(reply);
      // 写回：带 page 属性时先由宿主定位光标再插入；选区替换仅限无 page 的普通写回。
      var write;
      if (w === null) {
        write = Promise.resolve();
      } else if (hadSelection && !w.page) {
        write = ctx.doc.replaceSelected(w.text);
      } else {
        var at = w.page ? pageTarget(w) : null;
        write = (at ? ctx.doc.execute("caret.set", { pos: at }).catch(function () {}) : Promise.resolve()).then(
          function () {
            return ctx.doc.insert(w.text);
          },
        );
      }
      // 先写回文本，再按序执行命令（定位由命令自行完成）。
      function finish() {
        var shown = displayText(reply);
        // 纯命令型回复（模型没附正文）给一句完成提示，避免消息空白。
        state.messages[live].text = shown || (cmds.length ? "已完成文档修改" : shown);
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
