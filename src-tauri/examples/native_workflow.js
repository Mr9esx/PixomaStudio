void (async () => {
  const invoke = window.__TAURI_INTERNALS__.invoke
  const emit = (event, payload) => invoke('plugin:event|emit', { event, payload })
  const request = (method, path, body = null) => invoke('studio_request', { request: { method, path, body } })
  const progress = stage => emit('studio:workflow-progress', { stage })
  const assert = (condition, message) => { if (!condition) throw new Error(message) }
  const pause = () => new Promise(resolve => setTimeout(resolve, 100))
  const wait = async (condition, message) => {
    const deadline = Date.now() + 90000
    while (!await condition()) {
      const error = document.querySelector('[role="alert"][data-variant="destructive"]')
      if (error) throw new Error(error.textContent.trim())
      if (Date.now() >= deadline) throw new Error(message)
      await pause()
    }
    await pause()
  }
  const button = (label, parent = document) => [...parent.querySelectorAll('button')].find(item => item.textContent.trim() === label)
  const input = (element, value) => {
    assert(element, `输入控件缺失：${value}`)
    const prototype = element instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype
    Object.getOwnPropertyDescriptor(prototype, 'value').set.call(element, value)
    element.dispatchEvent(new Event('input', { bubbles: true }))
    element.dispatchEvent(new Event('change', { bubbles: true }))
  }
  const navigate = async path => {
    history.pushState({}, '', path)
    dispatchEvent(new PopStateEvent('popstate'))
    await wait(() => location.pathname === path, '工作流页面没有打开')
  }
  let stage = 'startup'
  try {
    await wait(() => document.querySelector('[data-slot="studio-page"]'), 'Studio 没有启动')
    stage = 'connect-agent'; await progress(stage)
    const connection = await request('POST', '/api/v1/studio/compute-nodes', {
      name: 'Linux-ComfyUI 页面验收', location: 'remote', enabled: true, comfyui_base_url: 'http://192.168.31.131:8188',
    })
    await emit('studio:workflow-agent', connection)
    await wait(async () => (await request('GET', `/api/v1/studio/compute-nodes/${connection.node.id}`)).healthy, '计算节点没有就绪')
    stage = 'import-file'; await progress(stage)
    await navigate('/studio/workflows/new')
    await wait(() => document.querySelector('#workflow-editor'), '工作流编辑器没有打开')
    const source = window.__PIXOMA_WORKFLOW_INPUT__
    const fileInput = document.querySelector('[data-testid="case-section-workflow-import"] input[type="file"]')
    const transfer = new DataTransfer()
    transfer.items.add(new File([source.content], source.filename, { type: 'application/json' }))
    fileInput.files = transfer.files
    fileInput.dispatchEvent(new Event('change', { bubbles: true }))
    await wait(() => document.querySelectorAll('[data-testid="workflow-import-diagram"] > ol > li').length === 9, '原附件的 9 个节点没有显示')
    input(document.querySelector('#case-name'), 'MiniMax Klein9B 页面验收')
    await pause()
    stage = 'bind-input'; await progress(stage)
    const inputs = document.querySelector('[data-testid="case-section-inputs"]')
    const addInput = [...inputs.querySelectorAll('button')].find(item => item.textContent.includes('添加') || item.textContent.includes('新增'))
    assert(addInput, `输入字段操作缺失：${inputs.innerText}`)
    addInput.click()
    await wait(() => inputs.querySelector('input'), '输入字段没有添加')
    const inputRow = inputs.querySelector('[data-testid="input-table-row"], [data-testid="input-field-card"]')
    input(inputRow.querySelector('input'), 'prompt')
    await pause()
    const bindInput = inputRow.querySelector('button[aria-label] svg.lucide-chevrons-up-down')?.closest('button')
    assert(bindInput, '输入绑定入口缺失')
    bindInput.click()
    await wait(() => [...document.querySelectorAll('[role="option"]')].some(item => item.dataset.value?.includes(' 40 text')), '文本节点没有显示')
    ;[...document.querySelectorAll('[role="option"]')].find(item => item.dataset.value?.includes(' 40 text')).click()
    await wait(() => bindInput.textContent.includes('text'), '文本输入绑定没有保存')
    stage = 'bind-output'; await progress(stage)
    const outputs = document.querySelector('[data-testid="case-section-outputs"]')
    const addOutput = [...outputs.querySelectorAll('button')].find(item => item.textContent.includes('添加') || item.textContent.includes('新增'))
    assert(addOutput, `输出字段操作缺失：${outputs.innerText}`)
    addOutput.click()
    await wait(() => outputs.querySelector('input'), '输出字段没有添加')
    const outputRow = outputs.querySelector('[data-testid="output-table-row"], [data-testid="output-field-card"]')
    input(outputRow.querySelector('input'), 'image')
    await pause()
    const bindOutput = outputRow.querySelector('button[aria-label] svg.lucide-chevrons-up-down')?.closest('button')
    assert(bindOutput, '输出绑定入口缺失')
    bindOutput.click()
    await wait(() => [...document.querySelectorAll('[role="option"]')].some(item => item.dataset.value?.includes(' 80 0')), '图片输出节点没有显示')
    ;[...document.querySelectorAll('[role="option"]')].find(item => item.dataset.value?.includes(' 80 0')).click()
    await pause()
    stage = 'select-node'; await progress(stage)
    document.querySelector('#workflow-node').click()
    await wait(() => [...document.querySelectorAll('[role="option"]')].some(item => item.textContent.includes(connection.node.name)), '执行节点选项没有显示')
    ;[...document.querySelectorAll('[role="option"]')].find(item => item.textContent.includes(connection.node.name)).click()
    await pause()
    stage = 'save-workflow'; await progress(stage)
    document.querySelector('#workflow-editor button[type="submit"]').click()
    await wait(() => !document.querySelector('#workflow-editor') && location.pathname !== '/studio/workflows/new', '工作流没有保存')
    const workflows = await request('GET', '/api/v1/studio/workflows')
    assert(workflows.length === 1, '工作流保存数量错误')
    const workflow = workflows[0]
    const original = JSON.parse(source.content)
    const properties = new Set()
    JSON.stringify(original, (key, value) => { properties.add(key); return value })
    const propertyOrder = [...properties].sort()
    assert(JSON.stringify(workflow.draft.execution.workflow, propertyOrder) === JSON.stringify(original, propertyOrder), '导入后的工作流内容变化')
    assert(workflow.draft.execution.node_id === connection.node.id, '所选计算节点没有保存')
    assert(workflow.draft.execution.input_bindings[0].node_id === '40', '文本节点绑定没有保存')
    assert(workflow.draft.execution.output_bindings[0].node_id === '80', '输出节点绑定没有保存')
    stage = 'run-workflow'; await progress(stage)
    button('运行草稿').click()
    await wait(() => document.querySelector('#workflow-test-prompt'), '运行输入表单没有打开')
    input(document.querySelector('#workflow-test-prompt'), JSON.parse(source.content)['40'].inputs.text)
    await pause()
    assert(!document.querySelector('[role="dialog"]'), '运行工作流仍显示模态窗口')
    button('运行工作流', document.querySelector('section[aria-label="运行工作流"]')).click()
    let execution
    await wait(async () => {
      const records = await request('GET', `/api/v1/studio/workflow-executions?workflow_id=${workflow.id}`)
      execution = records[0]
      if (['failed', 'cancelled', 'submission_unknown'].includes(execution?.status)) throw new Error(JSON.stringify(execution))
      return execution?.status === 'succeeded'
    }, '计算节点没有返回图片')
    stage = 'read-result'; await progress(stage)
    await wait(() => document.querySelector('[aria-label="执行结果"] img')?.naturalWidth === 1024, '结果图片没有显示')
    const image = document.querySelector('[aria-label="执行结果"] img')
    assert(image.naturalHeight === 1024, '结果图片高度不一致')
    assert(execution.outputs.length === 1, '输出数量错误')
    await emit('studio:workflow-result', { success: true, workflow_id: workflow.id, execution, width: image.naturalWidth, height: image.naturalHeight })
  } catch (error) {
    await emit('studio:workflow-result', { success: false, stage, message: error instanceof Error ? error.message : String(error), path: location.pathname, page_text: document.body.innerText.slice(-2000) })
  }
})()
