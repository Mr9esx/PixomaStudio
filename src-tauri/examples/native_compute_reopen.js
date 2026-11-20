void (async () => {
  const invoke = window.__TAURI_INTERNALS__.invoke
  const emit = (event, payload) => invoke('plugin:event|emit', { event, payload })
  const request = (method, path, body = null) => invoke('studio_request', { request: { method, path, body } })
  try {
    const nodes = await request('GET', '/api/v1/studio/compute-nodes')
    if (nodes.length !== 1) throw new Error('持久化节点数量不一致')
    const id = nodes[0].id
    const deadline = Date.now() + 30000
    let node
    do {
      node = await request('GET', `/api/v1/studio/compute-nodes/${id}`)
      if (node.healthy) break
      if (Date.now() > deadline) throw new Error(`重新启动 App 后节点没有就绪：${node.issues.join('；')}`)
      await new Promise(resolve => setTimeout(resolve, 200))
    } while (true)
    const credential = await request('GET', `/api/v1/studio/compute-nodes/${id}/credential`)
    if (!credential.agent_token.startsWith('pxnode_')) throw new Error('节点凭据未持久化')
    const metrics = await request('GET', `/api/v1/studio/compute-nodes/${id}/metrics?window=24h`)
    if (metrics.series.length < 1) throw new Error('监控历史没有保留')
    await emit('studio:layout-result', { success: true, restoredNode: id, agentVersion: node.agent_version, metricSamples: metrics.series.length })
  } catch (error) {
    await emit('studio:layout-result', { success: false, message: error.message })
  }
})()
