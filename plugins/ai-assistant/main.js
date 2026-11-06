// AI 助手插件：对话面板。用户用自然语言下指令（选中文字时带选区上下文）；
// 模型把要写回文档的结果包在 <write></write> 标签里，回答完成后自动
// 替换选区（无选区则插入光标处），结论直接展示在消息流里。
var SYSTEM = '你是 Word 文档编辑器内置的 AI 助手。用简体中文回答，直接给出结果本身，不要寒暄或重复问题。若用户要求改写、续写、翻译、润色或生成要放进文档的内容，把结果放在 <write></write> 标签中（编辑器会自动用它替换选区，或插入光标处），标签外最多留一句简短说明；纯问答、总结类回答不要使用标签。'

var history = [] // OpenAI 兼容历史（不含 system），一轮两条
var busy = false
var state = { messages: [], busy: false, placeholder: '输入指令，如：润色这段话' }

function render() {
  ctx.panel.setState('assistant', state)
}

// 剥掉 <write> 标记后的展示文本。
function displayText(reply) {
  return reply.replace(/<write>[\s\S]*?<\/write>/g, function (m) {
    return m.replace(/<\/?write>/g, '').trim()
  }).trim()
}

function writeContent(reply) {
  var m = reply.match(/<write>([\s\S]*?)<\/write>/)
  return m ? m[1] : null
}

var hadSelection = false // 本次回答是否有选区上下文：决定写回是替换还是插入

function handle(text) {
  if (busy) return
  busy = true
  state.busy = true
  state.error = ''
  state.messages.push({ role: 'user', text: text })
  render()

  Promise.all([ctx.doc.getSelectedText(), ctx.doc.getText()]).then(function (parts) {
    var selection = parts[0].trim()
    hadSelection = !!selection
    var user = text
    if (selection) {
      user += '\n\n【参考内容（选区）】\n' + selection.slice(0, 6000)
    } else if (parts[1].trim()) {
      user += '\n\n【参考内容（全文）】\n' + parts[1].slice(0, 6000)
    }
    history.push({ role: 'user', content: user })
    var messages = [{ role: 'system', content: SYSTEM }].concat(history.slice(-8))
    return ctx.ai.chat(messages)
  }).then(function (reply) {
    history.push({ role: 'assistant', content: reply })
    var content = writeContent(reply)
    var write = content === null
      ? Promise.resolve()
      : (hadSelection ? ctx.doc.replaceSelected(content) : ctx.doc.insert(content))
    // 写回目标看上一条用户消息是否带选区上下文。
    function finish() {
      state.messages.push({ role: 'assistant', text: displayText(reply) })
      busy = false
      state.busy = false
      render()
    }
    return write.then(finish, finish)
  }).catch(function (e) {
    state.messages.push({ role: 'assistant', text: '' })
    busy = false
    state.busy = false
    state.error = String((e && e.message) || e)
    render()
  })
}

ctx.registerPanel({
  id: 'assistant',
  title: 'AI 助手',
  defaultWidth: 340,
  onSend: function (text) { handle(text) }
})
ctx.registerCommand({
  id: 'ai-assistant.open',
  title: 'AI 助手',
  run: function () { ctx.panel.open('assistant') }
})
