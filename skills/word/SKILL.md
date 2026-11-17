## Word 技能（WordCraft）

### 写回与定位提醒

格式化/删除前必须先选中目标；用户指明行号、段落、位置时必须用按行/段落/范围定位，不要依赖光标；没指明位置时优先 `select.text` 按内容找。

### 字符格式（作用于当前选区）

- `format.bold {}`、`format.italic {}`、`format.underline {}`、`format.strikethrough {}`
- `format.color {"color":"RRGGBB"|"auto"}`、`format.highlight {"color":"RRGGBB"|"none"}`
- `format.size {"size":数字}`、`format.font {"name":"字体"}`、`format.clear {}`

示例——「帮我在第 2 行把『速度』加粗」：
`<cmd>[{"command":"select.range","params":{"anchor":{"block":2,"off":3},"focus":{"block":2,"off":5}}},{"command":"format.bold","params":{}}]</cmd>`
（off 按参考内容 [2] 行文本里『速度』的起止字符位置数）

### 插入对象（直接执行命令，不要用 <write> 写文字冒充；执行后不要重复插入）

- 表格：`insert.table {"rows":行,"cols":列}`
  - 用户指明位置时先定位（如文档末尾：`caret.docEnd {}`）；没指明时插到当前光标处
  - 填内容：参考内容里单元格文本也带 [行号]，`caret.set {"pos":{"block":单元格块号,"off":0}}` 后再 `<write>`
- 图形：`insert.shape {"kind":"rectangle|roundedRectangle|ellipse|triangle|diamond|line|arrow|star|heart","width":宽,"height":高,"fill":"RRGGBB"}`（宽高单位 pt，可省略）
- 文本框：`insert.textBox {"text":"文字"}`
- 图片仅当用户消息里带了图片数据（base64）时用 `insert.picture {"data":"base64","width":宽}`；你无法凭空生成图片，用户要插图但没给数据时如实说明需要提供

示例——「画一个橙色的五角星」：
`<cmd>[{"command":"insert.shape","params":{"kind":"star","fill":"FFA500"}}]</cmd>`

### 表格操作

行列增删、删除表格、合并等都作用于光标所在的表格——先 `caret.set {"pos":{"block":表格内任一单元格块号,"off":0}}` 把光标放进目标表格：
- 删除整个表格：`table.deleteTable {}`（删除表格唯一正确方式，不要用选区+<write> 冒充）
- 删行/删列：`table.deleteRow {}`、`table.deleteColumn {}`；加行/加列：`table.insertRowAbove {}`、`table.insertRowBelow {}`、`table.insertColumnLeft {}`、`table.insertColumnRight {}`
- 合并单元格：先 `select.range` 圈住多格再 `table.merge {}`；拆分 `table.split {}`
- 空表格在参考内容里没有 [行号] 无法定位光标时：如果表格是你刚刚插入的（光标大概率还在其中），可直接执行 `table.deleteTable {}`；否则如实告知需要用户先在表格中点一下

示例——「删除刚才那个表格」（其某单元格在参考内容里是 [12]）：
`<cmd>[{"command":"caret.set","params":{"pos":{"block":12,"off":0}}},{"command":"table.deleteTable","params":{}}]</cmd>`

### 编辑操作

- 撤销：`edit.undo {}`（可连续多次）；重做：`edit.redo {}`
- 删除一段文本：select 定位后 `edit.cut {}`
- 全文替换：`edit.replaceAll {"text":"旧词","with":"新词"}`

### 页面操作

页定位（用户指明页码时用页定位写回标签，宿主负责精确定位，禁止自己用 caret.set 按页计算）：
- `<write page="2" pos="end">文本</write>`：插入到第 2 页末尾；pos 取 start|end（缺省 start）
- 该标签只管页首/页末；页中间的位置仍按 [行号] 用 select/caret.set 定位
- 参考内容没有【分页】映射时无法按页操作，如实告知，不要猜

页面元素：
- 分页：`insert.pageBreak {}`；页码：`insert.pageNumber {"position":"bottom","align":"center"}`
- 页眉/页脚：`insert.header {"text":"文字"}`、`insert.footer {"text":"文字"}`
