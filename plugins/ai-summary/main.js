// AI 文档总结：把文档文本交给 DeepSeek 生成摘要，结果弹窗展示
ctx.registerCommand({
  id: 'ai-summary.run',
  title: '总结当前文档',
  run: async function () {
    var text = await ctx.doc.getText()
    if (!text) {
      ctx.ui.toast('当前没有可总结的文档内容')
      return
    }
    ctx.ui.toast('正在生成摘要…')
    var reply = await ctx.ai.chat(
      '请用简洁的中文列出以下文档的要点摘要，不超过 200 字。\n\n' + text
    )
    ctx.ui.dialog('AI 文档总结', reply)
  }
})
