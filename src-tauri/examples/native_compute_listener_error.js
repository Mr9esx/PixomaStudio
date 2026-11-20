void (async () => {
  const invoke = window.__TAURI_INTERNALS__.invoke
  const emit = (event, payload) => invoke('plugin:event|emit', { event, payload })
  const request = (method, path, body = null) => invoke('studio_request', { request: { method, path, body } })
  const wait = async condition => {
    const deadline = Date.now() + 10000
    while (!condition()) {
      if (Date.now() > deadline) throw new Error('设置页面没有显示')
      await new Promise(resolve => setTimeout(resolve, 80))
    }
  }
  const button = label => [...document.querySelectorAll('button')].find(item => item.getClientRects().length && item.textContent.trim() === label)
  try {
    await emit('studio:layout-focus')
    const connection = await request('GET', '/api/v1/studio/compute-connection')
    if (!connection.listen_error) throw new Error('监听失败没有返回具体错误')
    const session = await request('POST', '/api/v1/studio/sessions', { request_id: crypto.randomUUID() })
    if (!session.id) throw new Error('监听失败导致本地功能不可用')
    await wait(() => button('设置'))
    button('设置').click()
    await wait(() => button('计算节点'))
    button('计算节点').click()
    await wait(() => button('连接设置'))
    button('连接设置').click()
    await wait(() => document.querySelector('#compute-listen-address'))
    if (!document.querySelector('#compute-listen-address').disabled) throw new Error('启动配置覆盖状态没有显示')
    await emit('studio:layout-result', { success: true, listenerError: connection.listen_error, retainedLocalSession: session.id })
  } catch (error) { await emit('studio:layout-result', { success: false, message: error.message, pageText: document.body.innerText.slice(-1200) }) }
})()
