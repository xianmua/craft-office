#!/usr/bin/env node
// 内核页部署期注入（零内核改动）：
// 1) 直达链接：/wordcraft/?url=<remote> 等在 wasm 启动前重定向到统一入口 /office?url=<remote>
//    （早注入于 <head>，先于模块脚本执行，避免双内核实例并发启动）。/office 与
//    ?host=parent 宿主模式不受影响。
// 2) gridcraft 默认中文（原 0006 补丁的 HTML 注入，迁移到部署期）。
// 幂等：按标记跳过/清理已注入内容。用法：node scripts/inject-kernel-page.mjs [staticDir]
import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const STATIC = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", process.argv[2] ?? "backend/static");
const KERNELS = ["wordcraft", "gridcraft", "deckcraft", "pdfcraft"];
const MARKER = "artcraft-direct";

// 旧版注入清理：自嵌 iframe 引导（已废弃）与无标记的旧中文默认脚本。
const LEGACY = /<script id="artcraft-direct">[\s\S]*?<\/script>\n?|<script>\s*\(function \(\) \{\s*try \{\s*var KEY = 'gridcraft\.ui';[\s\S]*?<\/script>\n?/g;

const REDIRECT = (kernel) => `<!-- ArtCraft: /${kernel}/?url= redirects to the unified /office entry before wasm boots -->
<script id="${MARKER}">
(function () {
  try {
    var sp = new URLSearchParams(location.search);
    var u = sp.get("url");
    if (u && sp.get("host") !== "parent") {
      location.replace("/office?url=" + encodeURIComponent(u));
    }
  } catch (e) {}
})();
</script>`;

// gridcraft 默认中文（等价于已删除的 patches/0006，对 rebuild 自带的脚本幂等跳过）。
// 语言码必须是 'zh'：gridcraft Language 枚举 serde lowercase，'zh-hans' 解析失败会回退英文。
const ZH_DEFAULT = `<script id="artcraft-zh">
(function () {
  try {
    var KEY = 'gridcraft.ui';
    var prefs = {};
    try { prefs = JSON.parse(localStorage.getItem(KEY) || '{}') || {}; } catch (e) { prefs = {}; }
    // 'zh-hans' 是旧注入写错的码，一并迁移为 'zh'。
    if (!prefs.language || prefs.language === 'auto' || prefs.language === 'zh-hans') {
      prefs.language = 'zh';
      localStorage.setItem(KEY, JSON.stringify(prefs));
    }
  } catch (e) {}
})();
</script>`;

for (const kernel of KERNELS) {
  const file = path.join(STATIC, kernel, "index.html");
  let html;
  try {
    html = readFileSync(file, "utf8");
  } catch {
    console.log(`SKIP ${file} (not found)`);
    continue;
  }
  const orig = html;
  html = html.replace(LEGACY, "");
  if (!html.includes(MARKER) && html.includes("<head>")) {
    html = html.replace("<head>", "<head>\n" + REDIRECT(kernel));
  }
  if (kernel === "gridcraft" && !html.includes("artcraft-zh")) {
    if (html.includes("</title>")) {
      html = html.replace(/<\/title>/, "</title>\n" + ZH_DEFAULT);
    } else {
      console.log(`WARN gridcraft: no </title>, zh-default skipped`);
    }
  }
  if (html === orig) {
    console.log(`OK   ${kernel} (already injected)`);
  } else {
    writeFileSync(file, html);
    console.log(`DONE ${kernel}`);
  }
}
