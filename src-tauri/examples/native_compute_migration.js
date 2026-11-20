void (async () => {
  const invoke = window.__TAURI_INTERNALS__.invoke
  const emit = (event, payload) => invoke('plugin:event|emit', { event, payload })
  const request = (method, path, body = null) => invoke('studio_request', { request: { method, path, body } })
  const assert = (condition, message) => { if (!condition) throw new Error(message) }
  const pause = () => new Promise(resolve => setTimeout(resolve, 80))
  const wait = async (condition, message) => {
    const deadline = Date.now() + 30000
    while (!await condition()) {
      if (document.hidden) await emit('studio:layout-focus')
      if (Date.now() > deadline) throw new Error(message)
      await pause()
    }
    await pause()
  }
  const button = (label, parent = document) => [...parent.querySelectorAll('button')].find(item => item.getClientRects().length && item.textContent.trim() === label)
  const click = label => { const target = button(label); assert(target, `按钮缺失：${label}`); target.click() }
  const fill = (selector, value) => {
    const input = document.querySelector(selector)
    assert(input, `输入框缺失：${selector}`)
    const prototype = input instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype
    Object.getOwnPropertyDescriptor(prototype, 'value').set.call(input, value)
    input.dispatchEvent(new Event('input', { bubbles: true }))
  }
  const tab = async label => {
    const target = [...document.querySelectorAll('[role="tab"]')].find(item => item.textContent.trim() === label)
    assert(target, `标签页缺失：${label}`)
    target.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0, ctrlKey: false }))
    await pause()
  }
  const menu = async (selector, label) => {
    const trigger = document.querySelector(selector)
    assert(trigger, `菜单缺失：${selector}`)
    trigger.focus()
    trigger.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, key: 'Enter', code: 'Enter' }))
    await wait(() => [...document.querySelectorAll('[role="menuitem"]')].some(item => item.textContent.trim() === label), `菜单操作缺失：${label}`)
    ;[...document.querySelectorAll('[role="menuitem"]')].find(item => item.textContent.trim() === label).click()
    await pause()
  }
  let stage = '打开设置'
  const started = Date.now()
  try {
    await emit('studio:layout-focus')
    await wait(() => button('设置'), '设置入口缺失')
    click('设置')
    await wait(() => button('计算节点'), '计算节点菜单缺失')
    click('计算节点')
    await wait(() => button('新建节点'), '节点列表没有显示')
    stage = '创建节点'
    await emit('studio:layout-progress', { stage, elapsed: Date.now() - started })
    click('新建节点')
    await wait(() => document.querySelector('#edge-name'), '创建表单没有显示')
    assert(document.querySelector('[data-testid="create-edge-steps"]').textContent.includes('填写节点信息'), '创建步骤缺失')
    fill('#edge-name', '迁移检查工作站')
    fill('#edge-comfy-url', 'http://192.168.31.131:8188')
    fill('#edge-capabilities', '图片, 视频')
    document.querySelector('#edge-location').click()
    await wait(() => [...document.querySelectorAll('[role="option"]')].some(item => item.textContent === '本机'), '节点位置选项缺失')
    ;[...document.querySelectorAll('[role="option"]')].find(item => item.textContent === '本机').click()
    await pause()
    click('创建并继续')
    await wait(() => document.querySelector('[data-testid="edge-deploy-step"] #edge-deploy'), '部署步骤没有显示')
    assert(!document.querySelector('[role="dialog"]'), '创建节点出现模态窗口')
    const token = document.querySelector('#edge-agent-token').value
    const url = document.querySelector('#edge-control-plane').value
    assert(token.startsWith('pxnode_'), '创建节点未生成凭据')
    const command = document.querySelector('#edge-deploy').textContent
    assert(command.includes('CONTROL_PLANE_URL') && command.includes('EDGE_DATA_DIR') && !command.includes(token), '部署命令不完整或显示明文凭据')
    await tab('Windows PowerShell')
    assert(document.querySelector('#edge-deploy').textContent.includes('.\\edge-agent.exe'), 'Windows 命令缺失')
    await tab('macOS / Linux')
    const nodes = await request('GET', '/api/v1/studio/compute-nodes')
    const node = nodes.find(item => item.name === '迁移检查工作站')
    assert(node.capabilities.join(',') === '图片,视频', '能力标签没有保存')
    assert(url.startsWith('http://127.0.0.1:'), '本机部署地址没有使用实际节点服务')
    stage = '自动连接本机 Agent'
    await emit('studio:layout-progress', { stage, elapsed: Date.now() - started })
    await wait(async () => (await request('GET', `/api/v1/studio/compute-nodes/${node.id}`)).healthy, '真实 Agent 未连接或 ComfyUI 未就绪')
    await wait(() => button('完成') && !button('完成').disabled, '部署步骤没有更新连接状态')
    click('完成')
    await wait(() => document.querySelector('[data-testid="edge-observation"]'), '节点详情没有显示')
    await wait(() => document.querySelector('[data-testid="edge-metrics-charts"]'), '真实监控图表没有显示')
    assert(document.body.innerText.includes('0.43.0'), 'ComfyUI 版本没有展示')
    click('部署命令')
    await wait(() => button('停止 Agent'), '部署页没有显示 Agent 控制')
    click('停止 Agent')
    await wait(async () => !(await request('GET', `/api/v1/studio/compute-nodes/${node.id}`)).connected, '停止 Agent 后仍显示连接中')
    await wait(() => button('启动 Agent'), '启动 Agent 操作没有显示')
    click('启动 Agent')
    await wait(async () => (await request('GET', `/api/v1/studio/compute-nodes/${node.id}`)).healthy, '重新启动 Agent 后没有就绪')
    await wait(() => document.querySelector('#edge-agent-token')?.value === token, '重新打开部署页没有读取保存的凭据')
    document.querySelector('[aria-label="返回计算节点"]').click()
    await wait(() => document.querySelector('[data-testid="edge-observation"]'), '没有返回节点详情')
    stage = '硬件编辑与采集'
    await emit('studio:layout-progress', { stage, elapsed: Date.now() - started })
    const originalHardware = (await request('GET', `/api/v1/studio/compute-nodes/${node.id}`)).hardware
    click('编辑')
    await wait(() => document.querySelector('#edge-cpu'), '硬件编辑没有显示')
    fill('#edge-cpu', '工作站 CPU 标注')
    click('保存')
    await wait(() => document.querySelector('[data-testid="edge-observation"]'), '硬件编辑没有保存')
    const afterEdit = await request('GET', `/api/v1/studio/compute-nodes/${node.id}`)
    await wait(async () => (await request('GET', `/api/v1/studio/compute-nodes/${node.id}`)).hardware.collected_at !== afterEdit.hardware.collected_at, 'Agent 没有更新采集时间')
    assert((await request('GET', `/api/v1/studio/compute-nodes/${node.id}`)).hardware.cpu_model === '工作站 CPU 标注', '心跳覆盖了手动编辑的硬件信息')
    click('编辑')
    await wait(() => document.querySelector('#edge-cpu'), '硬件表单没有显示')
    click('从机器更新')
    await wait(() => document.querySelector('#edge-cpu')?.value === originalHardware.cpu_model && !button('保存').disabled, '没有恢复实际硬件采集信息')
    click('取消')
    await wait(() => document.querySelector('[data-testid="edge-observation"]'), '没有返回节点详情')
    const metrics = await request('GET', `/api/v1/studio/compute-nodes/${node.id}/metrics?window=1h`)
    assert(metrics.series.length > 0 && metrics.latest.mem_total_bytes > 0, '监控历史没有保存真实数据')
    stage = '检查窗口布局'
    await emit('studio:layout-progress', { stage, elapsed: Date.now() - started })
    const widths = []
    for (const width of [1440, 1024, 760]) {
      await emit('studio:layout-size', [width, 1000])
      await wait(() => innerWidth === width, '窗口尺寸没有更新')
      const page = document.querySelector('[data-slot="studio-page"]')
      assert(page.scrollWidth <= page.clientWidth, `节点页面超出窗口：${width}`)
      widths.push(width)
    }
    await emit('studio:layout-size', [1440, 1000])
    await wait(() => innerWidth === 1440, '窗口尺寸没有恢复')
    await tab('6h')
    await wait(() => document.querySelector('[data-testid="edge-metrics-charts"]'), '6h 监控没有显示')
    stage = '工作流配置与版本'
    await emit('studio:layout-progress', { stage, elapsed: Date.now() - started })
    const definition = { type: 'comfyui', description: '迁移检查', usage: '输入画面描述', inputs: [{ key: 'prompt', type: 'string', required: true, description: '画面描述' }], input_schema: { type: 'object', properties: { prompt: { type: 'string' } }, required: ['prompt'], additionalProperties: false }, outputs: [{ key: 'image', type: 'image', media_type: 'image/png' }], execution: { node_id: node.id, workflow: window.__PIXOMA_LAYOUT_WORKFLOW__, input_bindings: [{ key: 'prompt', node_id: '40', field_path: 'text' }], output_bindings: [{ key: 'image', node_id: '80', index: 0 }] } }
    const workflow = await request('POST', '/api/v1/studio/workflows', { name: '迁移检查工作流', category: 'image', enabled: true, draft: definition })
    click('工作流')
    await wait(() => document.querySelector(`a[href="/studio/workflows/${workflow.id}"]`), '工作流卡片没有显示')
    document.querySelector(`a[href="/studio/workflows/${workflow.id}"]`).click()
    await wait(() => button('运行草稿'), '工作流详情没有显示')
    await tab('工作流配置')
    await wait(() => document.querySelector('[data-testid="case-workflow-config-view"]'), '工作流配置没有显示')
    assert(document.querySelector('#case-routing-section').textContent.includes('迁移检查工作站'), '节点关联没有显示')
    assert(document.querySelector('[data-testid="workflow-import-diagram"]'), '配置没有显示节点图')
    await menu('[aria-label="工作流操作"]', '停用')
    await wait(() => button('运行草稿').disabled, '停用后仍允许执行')
    await menu('[aria-label="工作流操作"]', '启用')
    await wait(() => !button('运行草稿').disabled, '启用后未恢复执行')
    await tab('版本管理')
    await wait(() => button('创建版本'), '版本列表没有显示')
    click('创建版本')
    await wait(() => document.querySelector('#workflow-version-number'), '版本创建表单没有显示')
    fill('#workflow-version-number', '1.0.0')
    document.querySelector('#workflow-version-number').closest('form').requestSubmit()
    await wait(() => button('1.0.0') && !button('1.0.0').disabled, '版本没有保存')
    click('1.0.0')
    await wait(() => document.querySelector('section[aria-label="版本内容"] [data-testid="case-workflow-config-view"]'), '版本配置没有复用查看器')
    assert(!document.querySelector('[role="dialog"]'), '版本详情出现模态窗口')
    await emit('studio:layout-result', { success: true, widths, nodeId: node.id, workflowId: workflow.id, metricSamples: metrics.series.length, comfyVersion: '0.43.0' })
  } catch (error) {
    await emit('studio:layout-result', { success: false, stage, message: error.message, pageText: document.body.innerText.slice(-2400) })
  }
})()
