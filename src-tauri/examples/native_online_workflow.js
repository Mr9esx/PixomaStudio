void (async () => {
  const invoke = window.__TAURI_INTERNALS__.invoke
  const emit = (event, payload) => invoke('plugin:event|emit', { event, payload })
  const request = (method, path, body = null) => invoke('studio_request', { request: { method, path, body } })
  const assert = (condition, message) => { if (!condition) throw new Error(message) }
  const wait = async (condition, message) => {
    const deadline = Date.now() + 20000
    while (!await condition()) {
      if (document.hidden) await emit('studio:layout-focus')
      if (Date.now() > deadline) throw new Error(message)
      await new Promise(resolve => setTimeout(resolve, 80))
    }
    await new Promise(resolve => setTimeout(resolve, 100))
  }
  const button = label => [...document.querySelectorAll('button')].find(item => item.getClientRects().length && item.textContent.trim() === label)
  const fill = (selector, value) => {
    const input = document.querySelector(selector)
    assert(input, `字段缺失：${selector}`)
    const prototype = input instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype
    Object.getOwnPropertyDescriptor(prototype, 'value').set.call(input, value)
    input.dispatchEvent(new Event('input', { bubbles: true }))
  }
  try {
    await emit('studio:layout-focus')
    const connection = await request('POST', '/api/v1/studio/generation-connections', { name: '配置保存检查', provider: 'seedance', api_key: crypto.randomUUID() })
    await wait(() => button('工作流'), '工作流入口缺失')
    button('工作流').click()
    await wait(() => document.querySelector('a[href="/studio/workflows/new"]'), '创建工作流入口缺失')
    document.querySelector('a[href="/studio/workflows/new"]').click()
    await wait(() => [...document.querySelectorAll('[role="tab"]')].some(item => item.textContent === '在线模型'), '在线模型入口缺失')
    ;[...document.querySelectorAll('[role="tab"]')].find(item => item.textContent === '在线模型').dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0, ctrlKey: false }))
    await wait(() => document.querySelector('#online-workflow-template'), '在线模型表单没有显示')
    document.querySelector('#online-workflow-template').click()
    await wait(() => [...document.querySelectorAll('[role="option"]')].some(item => item.textContent.includes('文生视频')), '模型模板缺失')
    ;[...document.querySelectorAll('[role="option"]')].find(item => item.textContent.includes('文生视频')).click()
    await wait(() => document.querySelector('#case-name') && document.querySelector('#online-workflow-connection'), '基础信息组件没有显示')
    fill('#case-name', '在线工作流基础信息检查')
    fill('#case-description', '工作流说明')
    fill('#case-usage', '工作流使用方法')
    button('添加示例').click()
    await wait(() => document.querySelector('#case-example-title-0'), '示例编辑没有显示')
    fill('#case-example-title-0', '示例标题')
    fill('#case-example-prompt-0', '示例生成要求')
    document.querySelector('#online-workflow-connection').click()
    await wait(() => [...document.querySelectorAll('[role="option"]')].some(item => item.textContent.includes(connection.name)), '保存的服务连接没有显示')
    ;[...document.querySelectorAll('[role="option"]')].find(item => item.textContent.includes(connection.name)).click()
    button('保存工作流').click()
    await wait(() => location.pathname !== '/studio/workflows/new', '保存后没有返回工作流详情')
    assert(!document.querySelector('[role="alertdialog"]'), '保存成功后出现未保存确认')
    const workflow = await request('GET', `/api/v1/studio/workflows/${location.pathname.split('/').at(-1)}`)
    assert(workflow.name === '在线工作流基础信息检查', '名称没有保存')
    assert(workflow.draft.description === '工作流说明' && workflow.draft.usage === '工作流使用方法', '说明或使用方法没有保存')
    assert(workflow.draft.examples[0].title === '示例标题' && workflow.draft.examples[0].prompt === '示例生成要求', '示例没有保存')
    assert((await request('GET', `/api/v1/studio/workflow-executions?workflow_id=${workflow.id}`)).length === 0, '配置保存检查提交了外部执行')
    await emit('studio:layout-result', { success: true, workflowId: workflow.id, savedMetadata: ['name', 'description', 'usage', 'examples'], externalExecutions: 0 })
  } catch (error) { await emit('studio:layout-result', { success: false, message: error.message, path: location.pathname, pageText: document.body.innerText.slice(-1800) }) }
})()
