#!/usr/bin/env node
// DeepSeek ↔ WordCraft MCP 桥：起 `wordcraft-cli mcp`（stdio JSON-RPC），把 MCP 工具映射为
// DeepSeek function calling，循环执行到模型给出最终答复。内核原生能力，零内核补丁。
//
// 用法：
//   node scripts/deepseek-mcp.mjs "新建一份报价单文档…"   # 单轮：执行完退出
//   node scripts/deepseek-mcp.mjs                        # REPL：逐条指令对话
// 选项：
//   --cli <path>   wordcraft-cli 路径（默认 vendor/wordcraft-cli/wordcraft-cli.exe）
//   --work <dir>   自动化读写根目录（默认 ./mcp-workspace，模型只能读写这里）
// 配置：DEEPSEEK_API_KEY / DEEPSEEK_BASE_URL / DEEPSEEK_MODEL（环境变量优先，其次读 backend/.env）
import { spawn } from "node:child_process";
import { createInterface } from "node:readline";
import { existsSync, mkdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

// ---------- 参数 ----------
function parseArgs(argv) {
  const out = { positional: [] };
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === "--cli") out.cli = argv[++i];
    else if (argv[i] === "--work") out.work = argv[++i];
    else out.positional.push(argv[i]);
  }
  return out;
}
const args = parseArgs(process.argv.slice(2));
const CLI = args.cli ?? path.join(ROOT, "vendor", "wordcraft-cli", "wordcraft-cli.exe");
const WORK = path.resolve(args.work ?? path.join(ROOT, "mcp-workspace"));

// ---------- 配置（环境变量 > backend/.env）----------
function loadEnv() {
  const envFile = path.join(ROOT, "backend", ".env");
  const map = {};
  if (existsSync(envFile)) {
    for (const line of readFileSync(envFile, "utf8").split(/\r?\n/)) {
      const m = line.match(/^\s*([A-Z0-9_]+)\s*=\s*(.*)\s*$/);
      if (m) map[m[1]] = m[2].replace(/^["']|["']$/g, "");
    }
  }
  return {
    key: process.env.DEEPSEEK_API_KEY || map.DEEPSEEK_API_KEY || "",
    base: (process.env.DEEPSEEK_BASE_URL || map.DEEPSEEK_BASE_URL || "https://api.deepseek.com").replace(/\/+$/, ""),
    model: process.env.DEEPSEEK_MODEL || map.DEEPSEEK_MODEL || "deepseek-chat",
  };
}
const CFG = loadEnv();
if (!CFG.key) {
  console.error("缺少 DEEPSEEK_API_KEY（环境变量或 backend/.env）");
  process.exit(1);
}
if (!existsSync(CLI)) {
  console.error(`找不到 ${CLI}（重建方式见 docs/BUILD.md「内核源码还原」）`);
  process.exit(1);
}
mkdirSync(WORK, { recursive: true });

// ---------- MCP stdio 客户端 ----------
class Mcp {
  constructor() {
    this.proc = spawn(CLI, ["mcp", "--author", "DeepSeek (mcp)", "--automation-read-root", WORK, "--automation-write-root", WORK], { stdio: ["pipe", "pipe", "inherit"] });
    this.nextId = 1;
    this.pending = new Map();
    this.ready = false;
    this.buf = "";
    this.proc.stdout.on("data", (d) => this.#onData(d));
    this.proc.on("exit", (code) => {
      for (const [, rej] of this.pending) rej(new Error(`MCP 进程退出（code ${code}）`));
      this.pending.clear();
    });
  }
  #onData(d) {
    this.buf += d.toString();
    let i;
    while ((i = this.buf.indexOf("\n")) >= 0) {
      const line = this.buf.slice(0, i).trim();
      this.buf = this.buf.slice(i + 1);
      if (!line) continue;
      let msg;
      try { msg = JSON.parse(line); } catch { continue; }
      if (msg.id !== undefined && this.pending.has(msg.id)) {
        const { res, rej } = this.pending.get(msg.id);
        this.pending.delete(msg.id);
        msg.error ? rej(new Error(msg.error.message ?? JSON.stringify(msg.error))) : res(msg.result);
      }
    }
  }
  request(method, params, timeoutMs = 180000) {
    const id = this.nextId++;
    return new Promise((res, rej) => {
      const timer = setTimeout(() => { this.pending.delete(id); rej(new Error(`${method} 超时`)); }, timeoutMs);
      this.pending.set(id, { res: (v) => { clearTimeout(timer); res(v); }, rej: (e) => { clearTimeout(timer); rej(e); } });
      this.proc.stdin.write(JSON.stringify({ jsonrpc: "2.0", id, method, params }) + "\n");
    });
  }
  notify(method, params) {
    this.proc.stdin.write(JSON.stringify({ jsonrpc: "2.0", method, params }) + "\n");
  }
  async start() {
    await this.request("initialize", { protocolVersion: "2024-11-05", capabilities: {}, clientInfo: { name: "deepseek-mcp-bridge", version: "0.1" } }, 30000);
    this.notify("notifications/initialized", {});
    const { tools } = await this.request("tools/list", {}, 30000);
    this.ready = true;
    return tools;
  }
  call(name, args2) {
    return this.request("tools/call", { name, arguments: args2 ?? {} });
  }
}

// ---------- DeepSeek 对话循环 ----------
const SYSTEM = [
  "你是无头文档代理：通过 WordCraft MCP 工具创建和编辑文档文件（docx/pdf 等），不能操作用户屏幕上打开的窗口。",
  `你的所有文件读写仅限目录：${WORK}。保存文件用 save_document / exportPdf，路径必须写该目录下的完整绝对路径（相对路径会被沙箱拒绝）。`,
  "先规划再动手；写完后用 get_text 或 render_page 自检；完成或遇到工具报错都如实告知，不要编造结果。用中文答复。",
].join("\n");

async function deepseek(messages, tools) {
  const r = await fetch(`${CFG.base}/chat/completions`, {
    method: "POST",
    headers: { "Content-Type": "application/json", Authorization: `Bearer ${CFG.key}` },
    body: JSON.stringify({ model: CFG.model, messages, tools }),
  });
  if (!r.ok) throw new Error(`DeepSeek HTTP ${r.status}: ${(await r.text()).slice(0, 300)}`);
  return (await r.json()).choices[0].message;
}

function toolResultText(result) {
  if (!result) return "(无返回)";
  if (result.isError) return "工具报错: " + (result.content ?? []).map((c) => c.text ?? "").join("\n");
  return (result.content ?? []).map((c) => c.text ?? JSON.stringify(c)).join("\n") || "(成功，无文本返回)";
}

async function runTurn(mcp, tools, history, instruction) {
  history.push({ role: "user", content: instruction });
  for (let round = 0; round < 16; round++) {
    const msg = await deepseek(history, tools);
    if (!msg.tool_calls?.length) {
      history.push({ role: "assistant", content: msg.content });
      return msg.content;
    }
    history.push({ role: "assistant", content: msg.content || "", tool_calls: msg.tool_calls });
    for (const tc of msg.tool_calls) {
      const fn = tc.function;
      let result;
      try {
        result = await mcp.call(fn.name, fn.arguments ? JSON.parse(fn.arguments) : {});
      } catch (e) {
        result = { isError: true, content: [{ type: "text", text: String(e.message ?? e) }] };
      }
      const text = toolResultText(result);
      console.log(`  [工具] ${fn.name} ${fn.arguments ?? ""} -> ${text.slice(0, 160).replace(/\s+/g, " ")}`);
      history.push({ role: "tool", tool_call_id: tc.id, content: text.slice(0, 30000) });
    }
  }
  return "(达到工具轮次上限，已停止)";
}

// ---------- 入口 ----------
const mcp = new Mcp();
console.log(`MCP: ${CLI}\n工作目录: ${WORK}\n模型: ${CFG.model}`);
const tools = await mcp.start();
console.log(`已连接，${tools.length} 个工具可用\n`);
const dsTools = tools.map((t) => ({ type: "function", function: { name: t.name, description: t.description ?? "", parameters: t.inputSchema ?? { type: "object" } } }));
const history = [{ role: "system", content: SYSTEM }];

if (args.positional.length) {
  const reply = await runTurn(mcp, dsTools, history, args.positional.join(" "));
  console.log(`\n${reply}`);
  mcp.proc.kill();
} else {
  const rl = createInterface({ input: process.stdin, output: process.stdout });
  const ask = () => rl.question("你> ", async (line) => {
    const text = line.trim();
    if (!text) return ask();
    if (text === "exit" || text === "quit") { mcp.proc.kill(); rl.close(); return; }
    try {
      const reply = await runTurn(mcp, dsTools, history, text);
      console.log(`AI> ${reply}\n`);
    } catch (e) {
      console.log(`错误> ${e.message ?? e}\n`);
    }
    ask();
  });
  ask();
}
