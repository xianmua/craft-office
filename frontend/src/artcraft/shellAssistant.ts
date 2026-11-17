/**
 * 外壳插件运行时（阶段 6 gridcraft → 阶段 7 deckcraft/wordcraft）：
 * AI 助手从内核侧迁到 /office Vue 层——提示词/命令目录经 /api/plugins/file 拉取，技能指南经 /api/skills 拉取（顶层共享 skills/），
 * 文档上下文与命令执行走官方页面命令 API（iframe contentWindow.window.<app>，
 * 经 KernelFrame.call 薄代理）。协议与内核侧插件一致：<cmd>[…]</cmd> 按序执行；
 * wordcraft 另承接 <write> 写回（含 page 定位，原内核插件宿主职责）。
 */

import type { ChatMessage } from '@/api/ai'
import { chatStream, type AiEngine } from '@/api/ai'
import { apiUrl, kernelUrl } from '@/api/base'

/** 内核页面命令 API 桥：见 KernelFrame.call */
export type KernelBridge = (method: string, ...args: unknown[]) => Promise<unknown>

/** 外壳运行时支持的内核及各自插件文件（skill 为数组：本内核技能在前，全量注入见 skillGuide） */
export const SHELL_KERNELS = {
  wordcraft: { prompt: 'agent.md', skill: ['word/SKILL.md'], catalog: 'commands.json' },
  gridcraft: { prompt: 'agent-excel.md', skill: ['excel/SKILL.md'], catalog: 'commands-gridcraft.json' },
  deckcraft: { prompt: 'agent-pptx.md', skill: ['pptx/SKILL.md'], catalog: 'commands-deckcraft.json' },
} as const

export type ShellKernel = keyof typeof SHELL_KERNELS

interface CmdEntry {
  id: string
  label: string
  location: string
  params: string
  shortcut: string
}

/** 中文意图 → 命令 id 关键词（子串匹配），按内核分族。 */
const INTENTS: Record<ShellKernel, Array<[string, string]>> = {
  wordcraft: [
    ['表格|单元格|行高|列宽|合并|拆分', 'table'],
    ['图片|照片|图像|截图', 'picture'],
    ['页眉|页脚', 'hf header footer'],
    ['页码', 'pageNumber'],
    ['水印', 'watermark'],
    ['批注|评论|注释', 'comment'],
    ['修订|改痕|痕迹', 'trackChanges tracking'],
    ['脚注|尾注', 'footnote endnote citation bibliography'],
    ['目录', 'toc'],
    ['引用|参考文献|书目', 'citation bibliography references'],
    ['链接|超链接|网址', 'link'],
    ['书签|交叉引用', 'bookmark'],
    ['公式', 'equation'],
    ['图表', 'chart'],
    ['形状|图形', 'shape smartArt'],
    ['艺术字', 'wordArt'],
    ['文本框', 'textBox'],
    ['封面', 'coverPage'],
    ['字体|加粗|斜体|下划线|删除线|字号|颜色', 'font format.char format.text'],
    ['段落|缩进|对齐|行距|间距', 'para'],
    ['项目符号|编号|列表', 'bullets numbering list'],
    ['样式|标题级别', 'style para.style outline.level'],
    ['主题|设计|背景', 'design theme'],
    ['查找|替换', 'find replace'],
    ['撤销|重做', 'undo redo'],
    ['打印|导出|另存为|PDF', 'file.print file.export file.save'],
    ['页面|纸张|边距|横向|竖向|分栏|分页|分节', 'layout'],
    ['选中|选区|选择', 'select'],
    ['视图|缩放|窗口|阅读', 'view'],
    ['审阅|校对|拼写', 'review proof'],
    ['邮件|合并域', 'mailings merge'],
    ['域|日期|时间|自动更新', 'fields'],
    ['绘图|手绘|墨迹', 'draw'],
  ],
  gridcraft: [
    ['单元格|写入|填写|填入|赋值', 'cell.set cell.get range.setValues'],
    ['读取|读出|查看|看看|内容|数据|多少', 'sheet.read cell.get selection.stats docInspect'],
    ['选中|选择|选区|定位', 'selection.set selection.move selection.currentRegion selection.row selection.column edit.goTo'],
    ['公式|函数|求和|平均|统计|计算', 'formulas.'],
    ['字体|加粗|斜体|下划线|字号|颜色|删除线', 'home.font home.bold home.italic home.underline home.strikethrough home.fillColor home.borders'],
    ['对齐|换行|缩进|合并|拆分|边框|格式', 'home.align home.wrap home.indent home.merge home.unmerge home.borders home.formatCells home.numberFormat home.cellStyle home.conditionalFormat'],
    ['行|列|行高|列宽|隐藏|插入行|插入列|删除行|删除列', 'rows columns rowHeight columnWidth autofit'],
    ['工作表|sheet|标签|重命名|新建表|删除表', 'sheet.'],
    ['表格|套用格式|超级表', 'table. insert.table formatAsTable'],
    ['图表|图|可视化', 'chart insert.chart sparkline'],
    ['图片|形状|文本框|图标|艺术字|符号', 'insert.picture insert.shape insert.icons insert.textBox shape wordArt insert.symbol'],
    ['排序|筛选|去重|分列|验证|组合|分类汇总', 'data.sort data.filter data.removeDuplicates data.textToColumns data.validation data.group data.subtotal'],
    ['复制|粘贴|剪切|填充|清除|查找|替换', 'edit.copy edit.cut edit.paste edit.clear edit.fill edit.find edit.replace range.fill'],
    ['批注|注释|备注', 'review.newNote review.newComment review.deleteComment review.replyComment'],
    ['保护|锁定|加密', 'review.protect home.lockCell review.unprotect'],
    ['冻结|缩放|视图|网格线', 'view.freeze view.zoom view.gridlines view.show'],
    ['打印|导出|保存|PDF', 'file.save file.export file.print pageLayout.'],
    ['撤销|重做', 'edit.undo edit.redo'],
    ['透视|汇总分析', 'pivot insert.pivotTable'],
    ['链接|超链接', 'insert.link link.open'],
  ],
  deckcraft: [
    ['幻灯片|新增|新建|加一页|加页|插入页|删除页|复制页|移动页|重排', 'slide.new slide.delete slide.duplicate slide.move slide.go slide.hide slide.layout slide.reset slide.fromOutline'],
    ['读取|读出|查看|看看|内容|结构|大纲', 'slide.inspect document.inspect slide.fromOutline smartArt.toText shape.inspect'],
    ['标题|副标题|正文|占位符|文字|文本', 'text.set text.get text.insert text.edit shape.rename insert.textBox placeholder'],
    ['查找|替换', 'text.find text.replace'],
    ['形状|图形|插入|图标|图片|线条|箭头|表格|smartart', 'shape.insert shape.presets insert. smartArt shape.delete'],
    ['字体|加粗|斜体|下划线|字号|颜色|删除线', 'format.font format.bold format.italic format.underline format.color'],
    ['对齐|行距|缩进|项目符号|编号|列表|段落', 'format.align format.para format.list'],
    ['填充|边框|线条|样式|背景|阴影|圆角', 'shape.fill shape.line shape.quickStyle shape.effects format.shadow shape.adjust slide.background'],
    ['移动|缩放|大小|旋转|翻转|层级|对齐分布|组合', 'shape.move shape.resize shape.setBounds shape.rotate shape.flip arrange.'],
    ['动画|切换|过渡', 'animation. transition'],
    ['备注|演讲者', 'slide.notes'],
    ['选择|选中|选区', 'shape.select slide.selectSlides slide.selectRange text.select'],
    ['撤销|重做', 'edit.undo edit.redo'],
    ['放映|演示|播放', 'view.slideshow slideshow'],
    ['主题|母版|版式|模板', 'theme. master slide.layout file.templates'],
    ['批注|评论', 'comment.'],
    ['导出|保存|打印|PDF|图片', 'file.save file.export file.print'],
    ['图表', 'insert.chart chart.'],
  ],
}

function pluginFile(file: string): Promise<string> {
  return fetch(`${apiUrl(`plugins/file`)}?id=ai-assistant&file=${encodeURIComponent(file)}`).then((r) => {
    if (!r.ok) throw new Error(`插件文件 ${file} 加载失败 HTTP ${r.status}`)
    return r.text()
  })
}

/** 共享技能库：顶层 skills/ 目录，与插件解耦，任意 AI 消费方经 GET /api/skills 引用 */
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
    p = pluginFile(SHELL_KERNELS[kernel].prompt)
    promptCache.set(kernel, p)
  }
  return p
}

const skillCache = new Map<ShellKernel, Promise<string>>()
/** 技能指南：word/excel/pptx 三个 skill 全量注入，当前内核的排最前 */
function skillGuide(kernel: ShellKernel): Promise<string> {
  let p = skillCache.get(kernel)
  if (!p) {
    const ordered = [...SHELL_KERNELS[kernel].skill, ...Object.values(SHELL_KERNELS).flatMap((k) => k.skill)]
    p = Promise.all(
      ordered.map((f) => skillFile(f).then((t) => `<!-- skill: ${f} -->\n${t}`).catch(() => '')),
    ).then((parts) => parts.filter(Boolean).join('\n\n'))
    skillCache.set(kernel, p)
  }
  return p
}

const catalogCache = new Map<ShellKernel, CmdEntry[] | null>()
async function catalog(kernel: ShellKernel): Promise<CmdEntry[]> {
  if (!catalogCache.has(kernel)) {
    try {
      const j = JSON.parse(await pluginFile(SHELL_KERNELS[kernel].catalog))
      catalogCache.set(kernel, Array.isArray(j) ? j : [])
    } catch {
      catalogCache.set(kernel, [])
    }
  }
  return catalogCache.get(kernel)!
}

/** 按意图从目录挑相关命令拼成清单注入 system prompt；上限 140 条防膨胀。 */
export async function commandRef(kernel: ShellKernel, text: string): Promise<string> {
  const entries = await catalog(kernel)
  if (!entries.length) return ''
  const keys: string[] = []
  for (const [re, ids] of INTENTS[kernel]) {
    if (new RegExp(re).test(text)) keys.push(...ids.split(' '))
  }
  if (!keys.length) return ''
  const lines: string[] = []
  for (const c of entries) {
    if (lines.length >= 140) break
    const id = c.id.toLowerCase()
    if (keys.some((k) => id.includes(k.toLowerCase()))) {
      lines.push(`- ${c.id} ${c.params.replace(/\s+/g, ' ').slice(0, 110)} // ${c.label}`)
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

function colName(n: number): string {
  let s = ''
  while (n > 0) {
    const r = (n - 1) % 26
    s = String.fromCharCode(65 + r) + s
    n = Math.floor((n - 1) / 26)
  }
  return s
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

export async function docContext(kernel: ShellKernel, bridge: KernelBridge): Promise<string> {
  if (kernel === 'deckcraft') return deckContext(bridge)
  if (kernel === 'wordcraft') return wordContext(bridge)
  return gridContext(bridge)
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

export function parseCmds(reply: string): CmdOp[] {
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

/** 剥掉 <cmd> 块后的展示文本 */
export function displayText(reply: string): string {
  return reply.replace(/<cmd>[\s\S]*?<\/cmd>/g, '').trim()
}

export async function runCmds(
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
  )

  const cmds = parseCmds(reply)
  opts.history.push({ role: 'user', content: user }, { role: 'assistant', content: reply })
  // wordcraft 先写回文本再执行命令（对齐旧内核侧顺序，定位由命令自行完成）
  if (opts.kernel === 'wordcraft') {
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
export function liveDisplay(reply: string): string {
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

let cjkFontB64: string | null = null

/** CJK 兜底字体 base64（wasm 进程无系统中文字体，缺字会显示方块；来源见 docs/BUILD.md） */
async function cjkFontBase64(): Promise<string> {
  if (cjkFontB64) return cjkFontB64
  const res = await fetch(kernelUrl('fonts/SourceHanSansCN-Regular.otf'))
  if (!res.ok) throw new Error(`font fetch ${res.status}`)
  const buf = new Uint8Array(await res.arrayBuffer())
  let s = ''
  const CHUNK = 0x8000
  for (let i = 0; i < buf.length; i += CHUNK) {
    s += String.fromCharCode(...buf.subarray(i, i + CHUNK))
  }
  cjkFontB64 = btoa(s)
  return cjkFontB64
}

/** 内核就绪后安装中文字体（每次刷新 wasm 进程重建，需重装；失败静默不影响主流程） */
export async function ensureCjkFont(kernel: ShellKernel, bridge: KernelBridge | null): Promise<void> {
  if (kernel !== 'wordcraft' || !bridge) return
  try {
    const data = await cjkFontBase64()
    await bridge('execute', 'font.add', { data, name: 'SourceHanSansCN-Regular.otf' })
  } catch { /* 兜底失败仅表现为缺字方块 */ }
}
