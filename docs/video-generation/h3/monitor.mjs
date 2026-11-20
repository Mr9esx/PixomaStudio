const base = process.env.COMFYUI_URL ?? 'http://192.168.31.131:8188'
const id = process.argv[2]
if (!id) throw new Error('填写任务标识')
const socket = new WebSocket(`${base.replace(/^http/, 'ws')}/ws?clientId=pixoma-tea-h3-real`)
socket.addEventListener('message', event => {
  if (typeof event.data !== 'string') return
  const { type, data } = JSON.parse(event.data)
  if (data.prompt_id === id && ['progress', 'executing', 'execution_error', 'execution_success'].includes(type)) console.log(JSON.stringify({ type, data }))
})
const timer = setInterval(async () => {
  const response = await fetch(`${base}/history/${id}`)
  if (!response.ok) throw new Error(`查询失败：${response.status}`)
  const record = (await response.json())[id]
  if (record) {
    console.log(JSON.stringify({ status: record.status, outputs: record.outputs }))
    clearInterval(timer)
    socket.close()
  }
}, 15000)
