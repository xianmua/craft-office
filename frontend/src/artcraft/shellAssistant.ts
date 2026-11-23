/**
 * 外壳插件运行时（阶段 6 gridcraft → 阶段 7 deckcraft/wordcraft）：
 * AI 助手从内核侧迁到 /office Vue 层——提示词/注册表经 /api/plugins/file 拉取，技能/命令目录经 /api/skills、/api/assets/file 拉取（assets/ 共享目录），
 * 文档上下文与命令执行走官方页面命令 API（iframe contentWindow.window.<app>，
 * 经 KernelFrame.call 薄代理）。协议与内核侧插件一致：<cmd>[…]</cmd> 按序执行；
 * wordcraft 另承接 <write> 写回（含 page 定位，原内核插件宿主职责）。
 */

import { computed, ref } from 'vue'
import type { ChatMessage } from '@/api/ai'
import { chatStream, type AiEngine } from '@/api/ai'
import { apiUrl, kernelUrl } from '@/api/base'
import { colName } from '@/artcraft/ooxml'

/** 内核页面命令 API 桥：见 KernelFrame.call */
export type KernelBridge = (method: string, ...args: unknown[]) => Promise<unknown>

/** 内核 id 即 ShellKernel（数据驱动，注册表见 assets/plugins/ai-assistant/kernels.json） */
export type ShellKernel = string

/** 声明式内核描述：接新内核只需在 kernels.json 加条目 + 放 prompt/skill/catalog 文件，前端零改码 */
export interface KernelDesc {
  id: string
  /** 插件内 system prompt 文件 */
  prompt: string
  /** assets/skills/ 共享技能库下的技能文件 */
  skills: string[]
  /** 插件内命令目录 json */
  catalog: string
  /** 文档上下文格式化器（内置四种：grid=表格数据 / deck=幻灯片形状 / word=全文块+分页 / pdf=文档与视图概览） */
  context: 'grid' | 'deck' | 'word' | 'pdf'
  /** 是否承接 <write> 写回（word 型全文编辑器） */
  write: boolean
  /** 是否需要安装 CJK 兜底字体 */
  cjkFont: boolean
  /** 内核自身把已装字体持久化在 IndexedDB（跨刷新自动恢复），装过一次后跳过重装 */
  cjkFontKeep?: boolean
  /** 助手示例指令文案 */
  hints: string
  /** 中文意图 → 命令 id 关键词（子串匹配） */
  intents: Array<[string, string]>
}

const kernelDescs = ref(new Map<string, KernelDesc>())
/** 已注册内核 id（响应式：kernels.json 异步加载完成后更新，OfficeView gate 依赖它） */
export const shellKernelIds = computed(() => [...kernelDescs.value.keys()])

/** 取内核描述；未加载/未注册返回 undefined */
export function kernelDesc(kernel: ShellKernel): KernelDesc | undefined {
  return kernelDescs.value.get(kernel)
}

/** 模块加载即拉注册表（失败则无内核可用，重试由页面刷新兜底） */
void fetch(`${apiUrl('plugins/file')}?id=ai-assistant&file=${encodeURIComponent('kernels.json')}`)
  .then((r) => {
    if (!r.ok) throw new Error(`kernels.json HTTP ${r.status}`)
    return r.json()
  })
  .then((list: KernelDesc[]) => {
    kernelDescs.value = new Map(list.map((d) => [d.id, d]))
  })
  .catch((e) => console.error('[shellAssistant] 内核注册表加载失败', e))

interface CmdEntry {
  id: string
  label: string
  location: string
  params: string
  shortcut: string
}

function pluginFile(file: string): Promise<string> {
  return fetch(`${apiUrl(`plugins/file`)}?id=ai-assistant&file=${encodeURIComponent(file)}`).then((r) => {
    if (!r.ok) throw new Error(`插件文件 ${file} 加载失败 HTTP ${r.status}`)
    return r.text()
  })
}

/** 共享资源文件：assets/ 目录（命令目录等跨插件资源），经 GET /api/assets/file 两级查找 */
function resourceFile(file: string): Promise<string> {
  return fetch(`${apiUrl('assets/file')}?path=${encodeURIComponent(file)}`).then((r) => {
    if (!r.ok) throw new Error(`资源文件 ${file} 加载失败 HTTP ${r.status}`)
    return r.text()
  })
}

/** 共享技能库：assets/skills/ 目录，与插件解耦，任意 AI 消费方经 GET /api/skills 引用 */
function skillFile(file: string): Promise<string> {
  return fetch(`${apiUrl('skills')}?file=${encodeURIComponent(file)}`).then((r) => {
    if (!r.ok) throw new Error(`技能文件 ${file} 加载失败 HTTP ${r.status}`)
    return r.text()
  })
}

// ---- 懒加载缓存：系统提示词 / 技能指南 / 命令目录（按内核） ----

const promptCache = new Map<ShellKernel, Promise<string>>()
function systemPrompt(kernel: ShellKernel): Promise<string> {
  let p = promptCache.get(kernel)
  if (!p) {
    const file = kernelDesc(kernel)?.prompt
    p = file ? pluginFile(file) : Promise.resolve('')
    promptCache.set(kernel, p)
  }
  return p
}

const skillCache = new Map<ShellKernel, Promise<string>>()
/** 技能指南：只注入当前内核的 skill（全量注入实测 2/3 为无关内核技能，纯耗 token） */
function skillGuide(kernel: ShellKernel): Promise<string> {
  let p = skillCache.get(kernel)
  if (!p) {
    const files = kernelDesc(kernel)?.skills ?? []
    p = Promise.all(
      files.map((f) => skillFile(f).then((t) => `<!-- skill: ${f} -->\n${t}`).catch(() => '')),
    ).then((parts) => parts.filter(Boolean).join('\n\n'))
    skillCache.set(kernel, p)
  }
  return p
}

const catalogCache = new Map<ShellKernel, Promise<CmdEntry[]>>()
async function catalog(kernel: ShellKernel): Promise<CmdEntry[]> {
  if (!catalogCache.has(kernel)) {
    const file = kernelDesc(kernel)?.catalog
    const p = file
      ? resourceFile(`commands/${file}`)
          .then((t) => JSON.parse(t))
          .then((j) => (Array.isArray(j) ? j : []))
          .catch(() => [] as CmdEntry[])
      : Promise.resolve([] as CmdEntry[])
    catalogCache.set(kernel, p)
  }
  return catalogCache.get(kernel)!
}

/** 按意图从目录挑相关命令拼成清单注入 system prompt；上限 40 条（实测足够覆盖单条消息需求，防 token 膨胀）。 */
async function commandRef(kernel: ShellKernel, text: string): Promise<string> {
  const [entries, intents] = await Promise.all([catalog(kernel), Promise.resolve(kernelDesc(kernel)?.intents ?? [])])
  if (!entries.length || !intents.length) return ''
  const keys: string[] = []
  for (const [re, ids] of intents) {
    if (new RegExp(re).test(text)) keys.push(...ids.split(' '))
  }
  if (!keys.length) return ''
  const lines: string[] = []
  for (const c of entries) {
    if (lines.length >= 40) break
    const id = c.id.toLowerCase()
    if (keys.some((k) => id.includes(k.toLowerCase()))) {
      // pdfcraft 目录条目无 params 字段（纯 UI 命令），兜底为空串
      lines.push(`- ${c.id} ${(c.params ?? '').replace(/\s+/g, ' ').slice(0, 110)} // ${c.label}`)
    }
  }
  if (!lines.length) return ''
  return `\n\n【本条消息可用命令】仅列与本请求相关的命令，需执行时输出 <cmd>，格式 {"command": id, "params": {…}}；清单外没有的命令不要编造：\n${lines.join('\n')}`
}

// ---- 文档上下文：按内核取概览 + 当前内容 ----

function a1ToCol(s: string): number {
  let n = 0
  for (const ch of s) n = n * 26 + (ch.charCodeAt(0) - 64)
  return n
}

/** sheet.read 的 values 二维数组压成「A1=值」逐行文本，超出上限截断。 */
function valuesToText(values: unknown[][], range: string): string {
  const m = range.match(/([A-Z]+)(\d+)/)
  const col0 = m ? a1ToCol(m[1]) : 1
  const row0 = m ? parseInt(m[2], 10) : 1
  const lines: string[] = []
  let cells = 0
  for (let r = 0; r < values.length; r++) {
    const row = values[r] ?? []
    for (let c = 0; c < row.length; c++) {
      const v = row[c]
      if (v === null || v === undefined || v === '') continue
      if (cells >= 400) return lines.join('\n') + '\n…（已截断）'
      cells++
      lines.push(`${colName(col0 + c)}${row0 + r}=${v}`)
    }
  }
  return lines.join('\n')
}

/** deckcraft 上下文：幻灯片概览（docInspect）+ 当前页形状文本（slide.inspect） */
async function deckContext(bridge: KernelBridge): Promise<string> {
  let inspect: Record<string, unknown> = {}
  try {
    inspect = (await bridge('docInspect')) as Record<string, unknown>
  } catch {
    return ''
  }
  const slides = Array.isArray(inspect.slides) ? inspect.slides : []
  const overview = slides
    .map((s, i) => {
      const o = s as Record<string, unknown>
      const title = o.title ? `《${o.title}》` : ''
      return `${i + 1}.${title}${o.layout}(${o.shapes}个形状)`
    })
    .join('；')
  const head = `【演示文稿概览】${inspect.title ?? ''} 共${slides.length}页：${overview}`

  let data = ''
  try {
    const cur = (await bridge('execute', 'slide.inspect', {})) as {
      index?: number
      shapes?: Array<{ id?: number; name?: string; text?: string; kind?: string }> | null
    } | null
    if (cur?.shapes?.length) {
      const rows = cur.shapes
        .map((s) => `- [id=${s.id}] ${s.name ?? s.kind ?? ''}${s.text ? `：${s.text}` : '（无文字）'}`)
        .join('\n')
      data = `\n【当前页（第${(cur.index ?? 0) + 1}页）形状】\n${rows}`
    }
  } catch {
    /* 读不出时只给概览 */
  }
  return head + data
}

/** gridcraft 上下文：工作簿概览（docInspect）+ 当前表数据（sheet.read） */
async function gridContext(bridge: KernelBridge): Promise<string> {
  let inspect: Record<string, unknown> = {}
  try {
    inspect = (await bridge('docInspect')) as Record<string, unknown>
  } catch {
    return ''
  }
  const sheets = Array.isArray(inspect.sheets) ? inspect.sheets : []
  const overview = sheets
    .map((s, i) => {
      const o = s as Record<string, unknown>
      const cur = i === inspect.activeSheet ? '（当前）' : ''
      return `${i + 1}.${o.name}${cur} 已用区域:${o.usedRange} 单元格:${o.cells}`
    })
    .join('；')
  const head = `【工作簿概览】${inspect.title ?? ''} ${overview}；活动单元格:${inspect.activeCell} 选区:${inspect.selection}`

  let data = ''
  try {
    const read = (await bridge('execute', 'sheet.read', {})) as { range?: string; values?: unknown[][] } | null
    if (read?.values?.length) {
      data = `\n【当前表数据（区域 ${read.range ?? ''}）】\n` + valuesToText(read.values, read.range ?? 'A1')
    }
  } catch {
    /* 读不出数据时只给概览 */
  }
  return head + data
}

async function docContext(kernel: ShellKernel, bridge: KernelBridge): Promise<string> {
  const kind = kernelDesc(kernel)?.context
  if (kind === 'deck') return deckContext(bridge)
  if (kind === 'word') return wordContext(bridge)
  if (kind === 'grid') return gridContext(bridge)
  if (kind === 'pdf') return pdfContext(bridge)
  return ''
}

/** pdfcraft 上下文：ui.state（docInspect）压成文档列表 + 当前视图摘要（不含文本内容）。 */
async function pdfContext(bridge: KernelBridge): Promise<string> {
  let st: {
    documents?: Array<{ name?: string; pages?: number; dirty?: boolean; active?: boolean }>
    active?: { page?: number; selected_pages?: number[]; organize?: boolean; find_open?: boolean } | null
  } = {}
  try {
    st = (await bridge('docInspect')) as typeof st
  } catch {
    return ''
  }
  const docs = st.documents ?? []
  if (!docs.length) return ''
  const list = docs
    .map((d) => `${d.name ?? '未命名'}（${d.pages ?? '?'}页${d.dirty ? '，有未保存修改' : ''}${d.active ? '，当前' : ''}）`)
    .join('；')
  const a = st.active
  let view = ''
  if (a) {
    const parts = [`当前页：第${a.page ?? '?'}页`]
    if (a.selected_pages?.length) parts.push(`选中的页：${a.selected_pages.join(',')}`)
    if (a.organize) parts.push('组织模式（页面网格）')
    view = `；【当前视图】${parts.join('，')}`
  }
  return `【PDF 概览】${list}${view}`
}

/** wordcraft 写回上下文（对齐旧内核侧 main.js）：最近一次的全文块文本与分页起始块。 */
let wordBlocks: string[] = []
let wordFirstBlocks: number[] = []
let wordHadSelection = false

/** wordcraft 上下文：document.inspect 全文（[行号] 逐块）+ document.layout 分页映射。 */
async function wordContext(bridge: KernelBridge): Promise<string> {
  wordHadSelection = false
  let insp: {
    blocks?: Array<{ index: number; type: string; text?: string; cells?: string[][] }>
    pages?: number
    words?: number
    selection?: { text?: string }
  } = {}
  try {
    insp = (await bridge('docInspect')) as typeof insp
  } catch {
    return ''
  }
  const blocks = insp.blocks ?? []
  wordBlocks = blocks.map((b) =>
    b.type === 'table' ? (b.cells ?? []).map((row) => row.join(' | ')).join('\n') : (b.text ?? ''),
  )
  const selection = (insp.selection?.text ?? '').trim()
  wordHadSelection = !!selection

  const head = `【文档概览】共${insp.pages ?? '?'}页 ${insp.words ?? '?'}词`
  if (selection) return `${head}\n【参考内容（选区）】\n${selection.slice(0, 6000)}`
  const numbered = wordBlocks.map((t, i) => `[${i}] ${t}`).join('\n')
  if (!numbered.trim()) return head

  let pageMap = ''
  try {
    const l = (await bridge('execute', 'document.layout', {})) as {
      pages?: Array<{ number: number; firstBlock?: number }>
    } | null
    if (l?.pages?.length) {
      wordFirstBlocks = l.pages.map((p) => p.firstBlock ?? 0)
      pageMap = `\n【分页】共 ${l.pages.length} 页，页:起始block = ${l.pages.map((p) => `${p.number}:${p.firstBlock ?? 0}`).join(',')}`
    }
  } catch {
    wordFirstBlocks = []
  }
  return `${head}\n【参考内容（全文）】\n${numbered.slice(0, 6000)}${pageMap}`
}

// ---- 对话执行：解析 <cmd> 并经页面命令 API 按序执行 ----

interface CmdOp {
  command: string
  params?: Record<string, unknown>
}

function parseCmds(reply: string): CmdOp[] {
  const out: CmdOp[] = []
  reply.replace(/<cmd>([\s\S]*?)<\/cmd>/g, (_m, body: string) => {
    try {
      const arr = JSON.parse(body)
      if (Array.isArray(arr)) out.push(...arr)
    } catch {
      /* 残缺 JSON 忽略 */
    }
    return _m
  })
  return out
}

async function runCmds(
  cmds: CmdOp[],
  bridge: KernelBridge,
  signal?: AbortSignal,
  onActivity?: (label: string) => void,
): Promise<void> {
  for (const [i, c] of cmds.entries()) {
    if (signal?.aborted) throw new DOMException('aborted', 'AbortError')
    onActivity?.(`执行 ${c.command}（${i + 1}/${cmds.length}）`)
    const result = await bridge('execute', c.command, c.params ?? {})
    // execute 返回 {ok:false,error} 形状时抛出，中断后续命令
    if (result && typeof result === 'object' && (result as Record<string, unknown>).ok === false) {
      throw new Error(String((result as Record<string, unknown>).error ?? `命令 ${c.command} 执行失败`))
    }
  }
  if (cmds.length) onActivity?.('')
}

/** 一轮完整对话：组消息 → 流式请求 → 解析并执行 <cmd>（word 版另处理 <write> 写回）。
 * onReason 回调思考增量；onActivity 上报当前小动作（思考中 / 执行命令），空串表示动作结束。 */
export async function converse(opts: {
  kernel: ShellKernel
  bridge: KernelBridge
  history: ChatMessage[]
  text: string
  onDelta: (liveText: string) => void
  onReason?: (full: string) => void
  onActivity?: (label: string) => void
  signal?: AbortSignal
  engine?: AiEngine
}): Promise<string> {
  const [system, skill, cmdRef, context] = await Promise.all([
    systemPrompt(opts.kernel),
    skillGuide(opts.kernel),
    commandRef(opts.kernel, opts.text),
    docContext(opts.kernel, opts.bridge),
  ])
  const user = opts.text + (context ? `\n\n${context}` : '')
  const messages: ChatMessage[] = [
    { role: 'system', content: system + (skill ? `\n\n【技能指南】\n\n${skill}` : '') + cmdRef },
    ...opts.history.slice(-8),
    { role: 'user', content: user },
  ]

  let full = ''
  let reason = ''
  opts.onActivity?.('思考中') // DSH agent 模式思考阶段无增量输出，请求发出即显示
  const reply = await chatStream(
    messages,
    (delta) => {
      full += delta
      opts.onDelta(liveDisplay(full))
      opts.onActivity?.('') // 正文开始输出，动作提示让位
    },
    opts.signal,
    opts.engine,
    (delta) => {
      reason += delta
      opts.onReason?.(reason)
      opts.onActivity?.('思考中')
    },
    // DSH agent 流的工具调用过程（delta.tool_calls / 顶层 dsh 结果）→ 活动小字
    (label) => {
      opts.onActivity?.(label)
    },
  )

  const cmds = parseCmds(reply)
  opts.history.push({ role: 'user', content: user }, { role: 'assistant', content: reply })
  // word 型内核先写回文本再执行命令（对齐旧内核侧顺序，定位由命令自行完成）
  if (kernelDesc(opts.kernel)?.write) {
    await runWrite(reply, opts.bridge, opts.onActivity)
  }
  await runCmds(cmds, opts.bridge, opts.signal, opts.onActivity)
  return reply
}

interface WriteOp {
  text: string
  page: number | null
  pos: 'start' | 'end'
}

function parseWrite(reply: string): WriteOp | null {
  const m = reply.match(/<write([^>]*)>([\s\S]*?)<\/write>/)
  if (!m) return null
  const page = (m[1].match(/page="(\d+)"/) || [])[1]
  const pos = (m[1].match(/pos="(start|end)"/) || [])[1]
  return { text: m[2], page: page ? parseInt(page, 10) : null, pos: pos === 'end' ? 'end' : 'start' }
}

/** <write page pos> → 精确光标 {block, off}；分页数据缺失或页码越界返回 null。 */
function pageTarget(w: WriteOp): { block: number; off: number } | null {
  if (!w.page || !wordFirstBlocks.length) return null
  const count = wordFirstBlocks.length
  if (w.page < 1 || w.page > count) return null
  if (w.pos === 'start') return { block: wordFirstBlocks[w.page - 1], off: 0 }
  const endBlock = w.page < count ? wordFirstBlocks[w.page] - 1 : wordBlocks.length - 1
  return { block: endBlock, off: (wordBlocks[endBlock] ?? '').length }
}

/** 解析 <write> 写回标记并转命令执行（wordcraft 无内核插件宿主后由外壳承接）：
 * 带 page 属性时先由外壳定位光标再插入；选区替换仅限无 page 的普通写回。 */
async function runWrite(reply: string, bridge: KernelBridge, onActivity?: (label: string) => void): Promise<void> {
  const w = parseWrite(reply)
  if (!w) return
  onActivity?.('写入文档')
  if (wordHadSelection && !w.page) {
    // 替换选区：删掉选区内容，光标落在选区起点后插入
    await bridge('execute', 'edit.cut', {}).catch(() => {})
  } else {
    const at = w.page ? pageTarget(w) : null
    if (at) await bridge('execute', 'caret.set', { pos: at }).catch(() => {})
  }
  await bridge('execute', 'text.insert', { text: w.text })
}

/** 流式中的展示文本：剥完整 <cmd>/<write> 块与截断的半个标签，避免闪烁 */
function liveDisplay(reply: string): string {
  return reply
    .replace(/<cmd>[\s\S]*?<\/cmd>/g, '')
    .replace(/<write[^>]*>[\s\S]*?<\/write>/g, '')
    .replace(/<cmd>[^<]*$/, '')
    .replace(/<write[^>]*>[^<]*$/, '')
    .replace(/<\/write>/g, '')
    .trim()
}

/** 最终展示文本：liveDisplay 为空时回退 <write> 正文 / 命令条数摘要，避免纯操作回复不显示 */
export function replySummary(reply: string): string {
  const shown = liveDisplay(reply)
  if (shown) return shown
  const wm = reply.match(/<write[^>]*>([\s\S]*?)<\/write>/)
  if (wm && wm[1].trim()) return wm[1].trim()
  const n = (reply.match(/<cmd>/g) || []).length
  return n ? `已执行 ${n} 条操作。` : ''
}

/** 内核启动即装的 CJK 字体（wasm 进程无系统中文字体，缺字显示方块；文件见 backend/static/fonts，来源见 docs/BUILD.md）。
 * 内核按文档声明的字体名与字体内部 family 名精确匹配（无别名/尾部匹配），故用 scripts/make-cjk-fonts.py
 * 把思源 SC 的内部名改成「宋体」「黑体」「等线」「微软雅黑」「Inter」再装：宋体精确覆盖衬线正文，黑体精确覆盖标题/表头，
 * 等线（Word 2016+ 中文默认字体）覆盖文档内嵌 EMF 图片里的文字，微软雅黑覆盖常见声明字体，Inter 覆盖 deckcraft 主题默认字体（排最前优先装），其余声明名按缺字回退。 */
const CJK_FONT_FILES = [
  'InterCJK-Regular.otf',
  'InterCJK-Bold.otf',
  'SongTi-Regular.otf',
  'SongTi-Bold.otf',
  'HeiTi-Regular.otf',
  'HeiTi-Bold.otf',
  'DengXian-Regular.otf',
  'DengXian-Bold.otf',
  'WeiRuanYaHei-Regular.otf',
  'WeiRuanYaHei-Bold.otf',
]

/** CJK 字体 base64（按文件名缓存） */
const cjkFontCache = new Map<string, string>()
async function cjkFontBase64(file: string): Promise<string> {
  const hit = cjkFontCache.get(file)
  if (hit) return hit
  const res = await fetch(kernelUrl(`fonts/${file}`))
  if (!res.ok) throw new Error(`font fetch ${res.status}`)
  const buf = new Uint8Array(await res.arrayBuffer())
  let s = ''
  const CHUNK = 0x8000
  for (let i = 0; i < buf.length; i += CHUNK) {
    s += String.fromCharCode(...buf.subarray(i, i + CHUNK))
  }
  const b64 = btoa(s)
  cjkFontCache.set(file, b64)
  return b64
}

/** 内核就绪后安装中文字体（wasm 进程每次刷新重建；声明 cjkFont 的内核才装；失败静默不影响主流程）。
 * 返回时机：优先装完清单前两个字体（InterCJK 主题兜底）即返回让文档尽快打开，其余后台补装；
 * 内核带 cjkFontKeep（自身把字体持久化在 IndexedDB，跨刷新自动恢复）时装过一次即跳过。 */
export async function ensureCjkFont(kernel: ShellKernel, bridge: KernelBridge | null): Promise<void> {
  const desc = kernelDesc(kernel)
  if (!bridge || !desc?.cjkFont) return
  const mark = `cjk-fonts-ok:${kernel}`
  if (desc.cjkFontKeep && localStorage.getItem(mark)) return
  const first = CJK_FONT_FILES.slice(0, 2)
  const rest = CJK_FONT_FILES.slice(2)
  // 优先装主题兜底字体，装完即返回（调用方随后 openDoc）
  if (!(await installFonts(bridge, first))) return
  if (desc.cjkFontKeep) localStorage.setItem(mark, '1')
  // 其余字体后台补装，装完缩放往返强制重排（deckcraft 已布局文本不随新字体重排）
  void (async () => {
    if (!(await installFonts(bridge, rest))) return
    await bridge('execute', 'view.zoomOut', {}).catch(() => {})
    await bridge('execute', 'view.zoomIn', {}).catch(() => {})
    await bridge('execute', 'view.fit', {}).catch(() => {})
  })()
}

/** 并行拉取、逐个安装一批字体，全部成功返回 true（内核侧 font.add 串行处理） */
async function installFonts(bridge: KernelBridge, files: string[]): Promise<boolean> {
  let datas: string[]
  try {
    datas = await Promise.all(files.map(cjkFontBase64))
  } catch {
    return false /* 兜底失败仅表现为缺字方块 */
  }
  let ok = true
  for (let i = 0; i < files.length; i++) {
    try {
      await bridge('execute', 'font.add', { data: datas[i], name: files[i] })
    } catch { ok = false /* 单个失败不影响其余字体 */ }
  }
  return ok
}
