// AI 文档总结：用 hostApi.aiChat 把文档文本交给 DeepSeek 生成摘要
hostApi.registerCommand({
  id: 'ai-summary.run',
  title: '总结当前文档',
  run: async function () {
    var text = hostApi.docText()
    if (!text) {
      hostApi.toast('当前没有可总结的文档内容')
      return
    }
    hostApi.toast('正在生成摘要…')
    var reply = await hostApi.aiChat([
      { role: 'system', content: '你是文档助手，请用简洁的中文列出文档要点摘要，不超过 200 字。' },
      { role: 'user', content: text }
    ])
    hostApi.dialog('AI 文档总结', reply)
  }
})
