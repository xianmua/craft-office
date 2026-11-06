// 字数统计：纯本地计算文档文本
ctx.registerCommand({
  id: 'doc-stats.run',
  title: '统计字数',
  run: async function () {
    var text = await ctx.doc.getText()
    if (!text) {
      ctx.ui.toast('当前没有可统计的文档内容')
      return
    }
    var chars = text.replace(/\s/g, '').length
    var cjk = (text.match(/[\u4e00-\u9fff]/g) || []).length
    var words = (text.match(/[A-Za-z0-9]+/g) || []).length
    var lines = text.split('\n').filter(function (l) { return l.trim() }).length
    ctx.ui.dialog(
      '字数统计',
      '字符（不含空白）：' + chars + '\n中文字符：' + cjk + '\n英文单词：' + words + '\n非空段落数：' + lines
    )
  }
})
