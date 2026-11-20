void (async () => {
  const invoke = window.__TAURI_INTERNALS__.invoke
  const emit = (event, payload) => invoke('plugin:event|emit', { event, payload })
  const request = (method, path, body = null) => invoke('studio_request', { request: { method, path, body } })
  const assert = (condition, message) => { if (!condition) throw new Error(message) }
  const wait = async (condition, message) => {
    const deadline = Date.now() + 90000
    let reportAt = Date.now() + 5000
    while (!await condition()) {
      if (Date.now() > deadline) throw new Error(message)
      if (Date.now() > reportAt) { await emit('studio:layout-progress', { stage, remainingSeconds: Math.round((deadline - Date.now()) / 1000) }); reportAt = Date.now() + 5000 }
      await new Promise(resolve => setTimeout(resolve, 80))
    }
  }
  const button = (label, parent = document) => [...parent.querySelectorAll('button')].find(item => item.getClientRects().length && item.textContent.trim() === label)
  let stage = '读取本机工作流'
  try {
    await emit('studio:layout-show')
    const workflow = (await request('GET', '/api/v1/studio/workflows')).find(item => item.type === 'comfyui')
    assert(workflow, '本机工作流不存在')
    const nodeId = workflow.draft.execution.node_id
    await wait(async () => (await request('GET', `/api/v1/studio/compute-nodes/${nodeId}`)).healthy, '本机 Agent 没有就绪')
    const path = `/api/v1/studio/workflow-executions?workflow_id=${workflow.id}`
    const previous = (await request('GET', path)).length
    await wait(() => button('工作流'), '工作流入口没有显示')
    button('工作流').click()
    await wait(() => document.querySelector(`a[href="/studio/workflows/${workflow.id}"]`), '工作流卡片没有显示')
    document.querySelector(`a[href="/studio/workflows/${workflow.id}"]`).click()
    await wait(() => button('运行草稿'), '工作流详情没有显示')
    button('运行草稿').click()
    await wait(() => document.querySelector('#workflow-test-prompt'), '输入表单没有显示')
    const prompt = document.querySelector('#workflow-test-prompt')
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(prompt, workflow.draft.execution.workflow['40'].inputs.text)
    prompt.dispatchEvent(new Event('input', { bubbles: true }))
    await new Promise(resolve => setTimeout(resolve, 100))
    stage = '本机 Agent 执行'
    await emit('studio:layout-progress', { stage })
    button('运行工作流').click()
    let first
    await wait(async () => {
      const records = await request('GET', path)
      if (records.length <= previous) return false
      first = records[0]
      return Boolean(first.attempt_id)
    }, '本机 Agent 没有领取执行')
    let blocked = false
    try { await request('POST', `/api/v1/studio/compute-nodes/${nodeId}/agent`, { enabled: false }) }
    catch (error) { blocked = error.status === 409 && error.message.includes('执行中') }
    assert(blocked, '运行中允许停止本机 Agent')
    await emit('studio:layout-progress', { stage: '等待执行结果', execution: first.id, blocked })
    await wait(async () => {
      first = await request('GET', `/api/v1/studio/workflow-executions/${first.id}`)
      if (['failed', 'cancelled', 'submission_unknown'].includes(first.status)) throw new Error(first.error || first.status)
      return first.status === 'succeeded'
    }, '本机执行没有返回结果')
    await emit('studio:layout-focus')
    await wait(() => document.querySelector('[aria-label="执行结果"] img')?.naturalWidth === 1024 && button('再次运行') && !button('再次运行').disabled, '执行结果或再次运行操作没有显示')
    stage = '再次运行'
    await emit('studio:layout-progress', { stage, firstExecution: first.id })
    button('再次运行').click()
    let second
    await wait(async () => {
      const records = await request('GET', path)
      if (records.length <= previous + 1) return false
      second = records[0]
      if (['failed', 'cancelled', 'submission_unknown'].includes(second.status)) throw new Error(second.error || second.status)
      return second.status === 'succeeded'
    }, '再次运行没有生成新的执行记录')
    assert(second.id !== first.id && second.prompt_id !== first.prompt_id, '再次运行重复使用原执行')
    assert(first.outputs.length === 1 && second.outputs.length === 1, '执行结果数量不一致')
    assert((await request('GET', `/api/v1/studio/workflow-executions/${first.id}`)).outputs[0].asset_id === first.outputs[0].asset_id, '再次运行改变了原有结果')
    await emit('studio:layout-result', { success: true, firstExecution: first.id, secondExecution: second.id, preventedStopDuringExecution: blocked, width: 1024, events: second.events.map(event => event.kind) })
  } catch (error) { await emit('studio:layout-result', { success: false, stage, message: error.message, pageText: document.body.innerText.slice(-1700) }) }
})()
