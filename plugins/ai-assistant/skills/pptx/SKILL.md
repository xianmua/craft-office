## PPT 技能（DeckCraft）

命令目录随插件分发（commands-deckcraft.json，383 条），运行时按意图注入【本条消息可用命令】。以下是常用套路速查。

### 幻灯片定位原则

- 定位目标页：`slide.go {"index":2}`（0 起）、`slide.next/previous/first/last`
- 新建页：`slide.new {"layout":"titleAndContent","title":"…","body":"…"}`；版式名以概览里 layouts 为准（title/titleAndContent/sectionHeader/twoContent/comparison/titleOnly/blank 等）
- 批量建页：`slide.fromOutline {"text":"markdown 大纲","markdown":true}`
- 结构调整：`slide.duplicate`、`slide.move {"from":1,"to":3}`、`slide.delete`（破坏性，先说明）、`slide.hide`
- 当前页详情：`slide.inspect {}` 返回逐形状（id/name/text/box），是定位形状的主要依据

### 内容与文字

- 改形状文字：`text.set {"id":2,"text":"…"}`（整段替换保留格式）；追加：`text.insert {"text":"…"}`（`\n` 换段）
- 读形状文字核对：`text.get {"id":2}`
- 查找替换：`text.find`、`text.replace`
- 多条要点可一次写入（`\n` 分段），不必逐条；要分开的占位框不要硬塞

### 页面元素与样式

- 插入：`shape.insert`/`shape.presets`（形状）、`insert.textBox`、`insert.chart {"type":"column","categories":[…],"series":[…]}`、`insert.table`、`insert.slideNumber`、`smartArt.fromText`
- 图片仅当用户消息带图片数据（base64）时才可插入，无法凭空生成
- 样式：`shape.fill {"color":"#…"}`、`shape.line`、`format.bold/italic/underline {"on":true}`、`format.font {"family":"…"}`、`shape.quickStyle`、`shape.effects`
- 几何：`shape.move`（x/y 绝对、dx/dy 相对，单位 pt）、`shape.resize`、`shape.rotate`、`shape.flip`、`arrange.align {"edge":"center"}`、`arrange.distribute`、`arrange.group/ungroup`
- 选中：`selection.select {"ids":[…]}`；层级：`arrange.bringToFront/sendToBack` 等
- 动画与切换：`animation.add`/`animation.list`（先查可用效果）、`transition`；演讲者备注：`slide.notes {"text":"…"}`

### 汇报要求

- 完成后简述改了哪几页的什么内容；失败时如实报错，不要口头声称已完成
