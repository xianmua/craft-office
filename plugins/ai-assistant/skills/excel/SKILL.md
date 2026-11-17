## Excel 技能（GridCraft）

命令目录随插件分发（commands-gridcraft.json，409 条），运行时按意图注入【本条消息可用命令】。以下是常用套路速查。

### 定位与选择原则

- 一切修改前先选中目标：`selection.set {"range":"A1:B3"}`（区域引用形如 A1:B3，多区域用逗号，如 "A1:B2,D4"）；改格式时可给 home.* 命令直接传 range 参数
- 用户指明工作表名时先 `sheet.activate {"sheet":"名称"}` 再操作；没指明时作用于当前表
- 撤销兜底：误操作可 `edit.undo {}`

### 读写数据

- 读区域：`sheet.read {"range":"A1:D10"}`（缺省读已用区域；`formulas:true` 读公式原文）；读单格详情：`cell.get {"cell":"A1"}`（返回值/公式/样式/批注）；选区统计：`selection.stats {}`（count/sum/average/min/max）
- 写单格：`cell.set {"cell":"B2","input":"…"}`，input 以 = 开头即公式；批量写：`range.setValues {"range":"A1","values":[[...]]}`（parse:false 保留原样文本）
- 填充：`edit.fillDown/fillRight/fillUp/fillLeft {}`、`edit.autoFill`、`edit.fillSeries`、`edit.flashFill`

### 格式

- 字体：`home.bold/italic/underline/strikethrough`、`home.fontName {"fontName":"…"}`、`home.fontSize {"size":14}`、`home.fontColor {"color":"#C00000"}`
- 对齐与边框：`home.alignCenter {}`、`home.wrapText {}`、`home.borders`、`home.mergeCells/mergeCenter`、`home.unmergeCells`
- 数字格式：`home.numberFormat`、`home.percent`、`home.comma`、`home.accounting`
- 行列：`home.rowHeight/columnWidth`、`home.autofitRowHeight/autofitColumnWidth`、`home.insertRows/insertColumns/deleteRows/deleteColumns`
- 条件格式：`home.conditionalFormat`、`home.clearRules`

### 工作表与数据

- 工作表：`home.insertSheet {"name":"…"}`、`sheet.rename {"name":"…"}`、`sheet.activate`、`sheet.tabColor`、`home.deleteSheet {"sheet":…}`（破坏性，先说明）
- 排序筛选：`data.sort {"keys":[{"column":"B","order":"desc"}]}`、`data.filterBy`、`data.clearFilter`、`data.removeDuplicates`
- 查找替换：`edit.find`、`edit.replace`

### 图表与对象

- 图表：`insert.chart {"range":"A1:D8","type":"column","title":"…"}`（类型 column/bar/line/pie/doughnut/area/scatter 等）、`chart.changeType`、`chart.changeColors`、`object.delete`
- 其他插入：`insert.table`、`insert.picture`、`insert.shape`、`insert.textBox`、`insert.link`、`insert.sparkline`
- 批注：`review.newNote {"cell":"B2","text":"…"}`（笔记）、`review.newComment`（批注）

### 汇报要求

- 完成后简述改了哪些单元格/工作表；失败时如实报错，不要口头声称已完成
