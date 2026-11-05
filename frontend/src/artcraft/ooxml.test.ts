// @vitest-environment jsdom
import { describe, expect, it } from 'vitest'
import { zipSync } from 'fflate'
import {
  colIndex,
  inspectOoxml,
  rowIndex,
} from './ooxml'
import type { PreviewModel } from './ooxml'

const enc = (s: string) => new TextEncoder().encode(s)

// previewXlsx / previewPptx / previewDocx 未导出，通过 inspectOoxml 覆盖；
// 这里用 entryCount 间接验证包结构。
// zip 兼容字符串与已编码 Uint8Array 两种入参
function zip(entries: Record<string, string | Uint8Array>): Uint8Array {
  return zipSync(
    Object.fromEntries(
      Object.entries(entries).map(([k, v]) => [k, typeof v === 'string' ? enc(v) : v]),
    ),
  )
}

describe('colIndex / rowIndex：A1 引用换算', () => {
  it.each([
    ['A1', 0, 0],
    ['B2', 1, 1],
    ['C10', 2, 9],
    ['Z9', 25, 8],
    ['AA1', 26, 0],
    ['AB12', 27, 11],
    ['AZ1', 51, 0],
    ['BA1', 52, 0],
    ['ZZ1', 701, 0],
    ['AAA1', 702, 0],
    ['XFD1', 16383, 0],
    ['A100', 0, 99],
    ['C20', 2, 19],
    ['', 0, 0],
  ])('ref=%s -> col=%d row=%d', (ref, col, row) => {
    expect(colIndex(ref)).toBe(col)
    expect(rowIndex(ref)).toBe(row)
  })
})

describe('docx 解析', () => {
  it('空包返回空模型并记录 note', () => {
    const m = inspectOoxml('docx', zip({}))
    expect(m.paragraphs).toEqual([])
    expect(m.notes.join()).toContain('word/document.xml')
  })

  it('缺 word/document.xml 时返回空段落并记录 note', () => {
    const m = inspectOoxml('docx', zip({ 'word/other.xml': enc('<x/>') }))
    expect(m.paragraphs).toEqual([])
    expect(m.notes.join()).toContain('word/document.xml')
  })

  it('抽取段落并合并多 run 文本', () => {
    const xml = `<?xml version="1.0"?><w:document xmlns:w="x"><w:body>
      <w:p><w:r><w:t>Hello </w:t></w:r><w:r><w:t>World</w:t></w:r></w:p>
      <w:p><w:r><w:t>Second</w:t></w:r></w:p>
    </w:body></w:document>`
    const m = inspectOoxml('docx', zip({ 'word/document.xml': enc(xml) }))
    expect(m.paragraphs).toEqual(['Hello World', 'Second'])
  })

  it('空白段落过滤、空白折叠', () => {
    const xml = `<?xml version="1.0"?><w:document xmlns:w="x"><w:body>
      <w:p><w:r><w:t>   </w:t></w:r></w:p>
      <w:p><w:r><w:t>line1&#10;tab\tend</w:t></w:r></w:p>
    </w:body></w:document>`
    const m = inspectOoxml('docx', zip({ 'word/document.xml': enc(xml) }))
    expect(m.paragraphs).toEqual(['line1 tab end'])
  })

  it('超过 500 段时裁剪并记录 note', () => {
    const ps = Array.from({ length: 505 }, (_, i) => `<w:p><w:r><w:t>p${i}</w:t></w:r></w:p>`).join('')
    const xml = `<?xml version="1.0"?><w:document xmlns:w="x"><w:body>${ps}</w:body></w:document>`
    const m = inspectOoxml('docx', zip({ 'word/document.xml': enc(xml) }))
    expect(m.paragraphs!.length).toBe(500)
    expect(m.notes.join()).toContain('505')
  })
})

describe('xlsx 解析', () => {
  const wb = `<?xml version="1.0"?><workbook xmlns="x" xmlns:r="y"><sheets>
    <sheet name="销量" sheetId="1" r:id="rId1"/><sheet name="汇总" sheetId="2" r:id="rId2"/>
  </sheets></workbook>`
  const ss = `<?xml version="1.0"?><sst xmlns="x">
    <si><t>产品</t></si><si><t>数量</t></si><si><t>单价</t></si>
  </sst>`

  it('共享字符串与数字单元格', () => {
    const s1 = `<?xml version="1.0"?><worksheet xmlns="x"><sheetData>
      <row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>1</v></c></row>
      <row r="2"><c r="A2" t="inlineStr"><is><t>苹果</t></is></c><c r="B2"><v>10</v></c></row>
    </sheetData></worksheet>`
    const m = inspectOoxml('xlsx', zip({ 'xl/workbook.xml': enc(wb), 'xl/sharedStrings.xml': enc(ss), 'xl/worksheets/sheet1.xml': enc(s1) }))
    expect(m.sheets!.length).toBe(1)
    const sh = m.sheets![0]!
    expect(sh.name).toBe('销量')
    expect(sh.rows[0]).toEqual(['产品', '数量', '', '', '', '', '', '', '', '', '', '', '', '', '', '', '', '', '', ''])
    expect(sh.rows[1]).toEqual(['苹果', '10', '', '', '', '', '', '', '', '', '', '', '', '', '', '', '', '', '', ''])
  })

  it('按 sheetN 数字排序而非 zip 顺序', () => {
    const s1 = `<worksheet xmlns="x"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>S1</t></is></c></row></sheetData></worksheet>`
    const s2 = `<worksheet xmlns="x"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>S2</t></is></c></row></sheetData></worksheet>`
    const m = inspectOoxml('xlsx', zip({ 'xl/workbook.xml': enc(wb), 'xl/sharedStrings.xml': enc(ss), 'xl/worksheets/sheet2.xml': enc(s2), 'xl/worksheets/sheet1.xml': enc(s1) }))
    expect(m.sheets!.map((s) => s.rows[0]![0])).toEqual(['S1', 'S2'])
    expect(m.sheets!.map((s) => s.name)).toEqual(['销量', '汇总'])
  })

  it('越界行列触发 truncated', () => {
    const s1 = `<worksheet xmlns="x"><sheetData>
      <c r="A85" t="inlineStr"><is><t>x</t></is></c>
      <c r="Z1" t="inlineStr"><is><t>y</t></is></c>
    </sheetData></worksheet>`
    const m = inspectOoxml('xlsx', zip({ 'xl/workbook.xml': enc(wb), 'xl/sharedStrings.xml': enc(ss), 'xl/worksheets/sheet1.xml': enc(s1) }))
    expect(m.sheets![0]!.truncated).toBe(true)
  })

  it('缺 sharedStrings 时 s 类型单元格跳过而不是 panic', () => {
    const s1 = `<worksheet xmlns="x"><sheetData><c r="A1" t="s"><v>0</v></c></sheetData></worksheet>`
    const m = inspectOoxml('xlsx', zip({ 'xl/workbook.xml': enc(wb), 'xl/worksheets/sheet1.xml': enc(s1) }))
    // 共享字符串缺失 → 解析为空串 → 空单元格被跳过，行不产生
    expect(m.sheets![0]!.rows).toEqual([])
  })

  it('无 worksheet 时记录 note', () => {
    const m = inspectOoxml('xlsx', zip({ 'xl/workbook.xml': enc(wb) }))
    expect(m.notes.join()).toContain('工作表')
    expect(m.sheets).toEqual([])
  })

  it('最多预览 5 个 sheet', () => {
    const entries: Record<string, string> = {}
    for (let i = 1; i <= 7; i++) entries[`xl/worksheets/sheet${i}.xml`] = `<worksheet xmlns="x"><sheetData><c r="A1" t="inlineStr"><is><t>S${i}</t></is></c></sheetData></worksheet>`
    const m = inspectOoxml('xlsx', zip(entries))
    expect(m.sheets!.length).toBe(5)
  })
})

describe('pptx 解析', () => {
  it('按页码排序并抽取 a:t', () => {
    const s2 = `<p:sld xmlns:a="x" xmlns:p="y"><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>第二页</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>`
    const s1 = `<p:sld xmlns:a="x" xmlns:p="y"><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>  第一页  </a:t></a:r></a:p><a:p><a:r><a:t>要点一</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>`
    const m = inspectOoxml('pptx', zip({ 'ppt/slides/slide2.xml': enc(s2), 'ppt/slides/slide1.xml': enc(s1) }))
    expect(m.slides![0]).toEqual({ index: 1, texts: ['第一页', '要点一'] })
    expect(m.slides![1]).toEqual({ index: 2, texts: ['第二页'] })
  })

  it('无幻灯片记录 note', () => {
    const m: PreviewModel = inspectOoxml('pptx', zip({ 'ppt/other.xml': enc('<x/>') }))
    expect(m.slides).toEqual([])
    expect(m.notes.join()).toContain('幻灯片')
  })

  it('每页最多 40 条文本', () => {
    const ts = Array.from({ length: 45 }, (_, i) => `<a:p><a:r><a:t>t${i}</a:t></a:r></a:p>`).join('')
    const s1 = `<p:sld xmlns:a="x" xmlns:p="y"><p:cSld><p:spTree><p:sp><p:txBody>${ts}</p:txBody></p:sp></p:spTree></p:cSld></p:sld>`
    const m = inspectOoxml('pptx', zip({ 'ppt/slides/slide1.xml': enc(s1) }))
    expect(m.slides![0]!.texts.length).toBe(40)
  })
})

describe('入口', () => {
  it.each([['docx'], ['xlsx'], ['pptx']] as const)('kind=%s 记录 entryCount', (kind) => {
    const extra: Record<string, string> =
      kind === 'docx'
        ? { 'word/document.xml': '<w:document xmlns:w="x"/>' }
        : kind === 'xlsx'
          ? { 'xl/worksheets/sheet1.xml': '<worksheet/>' }
          : { 'ppt/slides/slide1.xml': '<p:sld xmlns:a="x" xmlns:p="y"/>' }
    const m = inspectOoxml(kind, zip(extra))
    expect(m.kind).toBe(kind)
    expect(m.entryCount).toBeGreaterThan(0)
  })

  it('损坏的 zip 抛错', () => {
    expect(() => inspectOoxml('docx', new Uint8Array([1, 2, 3]))).toThrow()
  })
})
