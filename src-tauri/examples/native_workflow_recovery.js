void (async () => {
  const invoke = window.__TAURI_INTERNALS__.invoke
  const emit = (event, payload) => invoke('plugin:event|emit', { event, payload })
  const request = path => invoke('studio_request', { request: { method: 'GET', path, body: null } })
  try {
    await emit('studio:layout-show')
    const workflows = await request('/api/v1/studio/workflows')
    const workflow = workflows.find(item => item.type === 'comfyui')
    const records = await request(`/api/v1/studio/workflow-executions?workflow_id=${workflow.id}`)
    const pending = records.find(item => item.status === 'running')
    if (!pending?.prompt_id) throw new Error('没有等待恢复的已提交执行')
    await emit('studio:layout-progress', { stage: '恢复执行', execution: pending.id, prompt: pending.prompt_id })
    const deadline = Date.now() + 90000
    let execution
    do {
      execution = await request(`/api/v1/studio/workflow-executions/${pending.id}`)
      if (execution.status === 'succeeded') break
      if (['failed', 'cancelled', 'submission_unknown'].includes(execution.status)) throw new Error(execution.error || execution.status)
      if (Date.now() > deadline) throw new Error('重新启动后执行没有完成')
      await new Promise(resolve => setTimeout(resolve, 200))
    } while (true)
    if (execution.prompt_id !== pending.prompt_id || execution.outputs.length !== 1) throw new Error('恢复执行的提交编号或结果数量不一致')
    await emit('studio:layout-result', { success: true, recoveredExecution: execution.id, prompt: execution.prompt_id, output: execution.outputs[0].asset_id, events: execution.events.map(event => event.kind) })
  } catch (error) { await emit('studio:layout-result', { success: false, message: error.message }) }
})()
