void (async () => {
  const invoke = window.__TAURI_INTERNALS__.invoke
  const emit = (event, payload) => invoke('plugin:event|emit', { event, payload })
  const request = (method, path, body = null) => invoke('studio_request', { request: { method, path, body } })
  const assert = (condition, message) => { if (!condition) throw new Error(message) }
  const canonical = value => JSON.stringify(value, (_key, entry) => entry && typeof entry === 'object' && !Array.isArray(entry) ? Object.fromEntries(Object.entries(entry).sort(([left], [right]) => left.localeCompare(right))) : entry)
  const wait = async (condition, message) => {
    const end = Date.now() + 20000
    while (!await condition()) {
      if (Date.now() > end) throw new Error(message)
      await new Promise(resolve => setTimeout(resolve, 50))
    }
  }
  const button = (label, parent = document) => [...parent.querySelectorAll('button')].find(item => item.getClientRects().length && item.textContent.trim() === label)
  const click = label => { const target = button(label); assert(target, `操作不存在：${label}`); target.click() }
  const tab = label => [...document.querySelectorAll('[role="tab"]')].find(item => item.textContent.trim() === label)?.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0, ctrlKey: false }))
  const fill = (selector, value) => {
    const input = document.querySelector(selector)
    assert(input, `输入框不存在：${selector}`)
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(input, value)
    input.dispatchEvent(new Event('input', { bubbles: true }))
  }
  const noDialog = () => assert(!document.querySelector('[role="dialog"]'), '页面操作显示模态窗口')
  try {
    await emit('studio:layout-show')
    await wait(() => button('工作流'), '主导航未显示')
    click('工作流')
    await wait(() => document.querySelector('a[href*="/studio/workflows/"]:not([href*="new"])'), '真实工作流不存在')
    document.querySelector('a[href*="/studio/workflows/"]:not([href*="new"])').click()
    await wait(() => button('运行草稿'), '工作流详情没有显示')
    const workflowId = location.pathname.split('/').at(-1)
    tab('版本管理')
    await wait(() => button('创建版本'), '版本列表没有显示')
    click('创建版本')
    await wait(() => document.querySelector('#workflow-version-number'), '版本表单没有展开')
    noDialog()
    const existingVersions = await request('GET', `/api/v1/studio/workflows/${workflowId}/versions`)
    const versionNumber = `1.0.${existingVersions.length}`
    fill('#workflow-version-number', versionNumber)
    document.querySelector('#workflow-version-number').closest('form').requestSubmit()
    await wait(() => !document.querySelector('#workflow-version-number'), '工作流版本没有保存')
    const versions = await request('GET', `/api/v1/studio/workflows/${workflowId}/versions`)
    assert(versions.some(item => item.version === versionNumber), '保存的版本不存在')
    await wait(() => button(versionNumber) && !button(versionNumber).disabled, '版本内容操作仍不可用')
    click(versionNumber)
    await wait(() => document.querySelector('section[aria-label="版本内容"]'), '版本内容没有展开')
    noDialog()
    click('收起')
    await wait(() => !document.querySelector('section[aria-label="版本内容"]'), '版本内容没有收起')
    const openAction = async label => {
      const action = () => [...document.querySelectorAll('tr')].find(row => row.textContent.includes(versionNumber))?.querySelector('button[aria-label="更多操作"]')
      await wait(() => action() && !action().disabled, '版本操作仍不可用')
      action().focus()
      action().dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, key: 'Enter', code: 'Enter' }))
      await wait(() => [...document.querySelectorAll('[role="menuitem"]')].some(item => item.textContent.trim() === label), '版本菜单操作没有显示')
      ;[...document.querySelectorAll('[role="menuitem"]')].find(item => item.textContent.trim() === label).click()
    }
    const version = versions.find(item => item.version === versionNumber)
    await openAction('设为默认版本')
    await wait(async () => (await request('GET', `/api/v1/studio/workflows/${workflowId}`)).default_version_id === version.id, '默认版本没有保存')
    const beforeCopy = await request('GET', `/api/v1/studio/workflows/${workflowId}`)
    await openAction('复制到草稿')
    await wait(() => document.querySelector('[role="dialog"]'), '复制到草稿没有确认')
    button('复制到草稿', document.querySelector('[role="dialog"]')).click()
    await wait(() => !document.querySelector('[role="dialog"]'), '版本没有复制到草稿')
    const afterCopy = await request('GET', `/api/v1/studio/workflows/${workflowId}`)
    assert(afterCopy.revision > beforeCopy.revision && canonical(afterCopy.draft) === canonical(version.definition), '草稿内容与版本不一致')
    await wait(() => [...document.querySelectorAll('tr')].find(row => row.textContent.includes(versionNumber))?.querySelector('button[aria-label="更多操作"]:not([disabled])'), '版本操作仍不可用')
    const actions = [...document.querySelectorAll('tr')].find(row => row.textContent.includes(versionNumber))?.querySelector('button[aria-label="更多操作"]')
    assert(actions, '版本操作按钮不存在')
    await wait(() => !actions.disabled, '版本操作仍不可用')
    actions.focus()
    actions.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, key: 'Enter', code: 'Enter' }))
    await wait(() => document.querySelector('[role="menuitem"]'), '版本操作没有显示')
    const testVersion = [...document.querySelectorAll('[role="menuitem"]')].find(item => item.textContent.trim() === '测试版本')
    assert(testVersion, '测试版本操作不存在')
    testVersion.click()
    await wait(() => document.querySelector('#workflow-test-prompt'), '版本运行表单没有显示')
    noDialog()
    const widths = []
    for (const width of [1440, 1024, 760]) {
      await emit('studio:layout-size', [width, 900])
      await wait(() => innerWidth === width, '窗口尺寸未更新')
      const main = document.querySelector('[data-slot="studio-page"]')
      assert(main.scrollWidth <= main.clientWidth, '工作流运行区域超出窗口')
      widths.push({ width, scroll: main.scrollWidth, client: main.clientWidth })
    }
    await emit('studio:layout-result', { success: true, workflowId, version: versionNumber, defaultVersion: afterCopy.default_version_id, copiedDraft: afterCopy.revision, widths })
  } catch (error) {
    await emit('studio:layout-result', { success: false, message: error.message, location: location.href, actions: [...document.querySelectorAll('button[aria-label="更多操作"]')].map(item => ({ expanded: item.getAttribute('aria-expanded'), disabled: item.disabled, state: item.dataset.state, context: item.parentElement.parentElement.textContent.slice(0, 160) })), pageText: document.body.innerText.slice(-1800) })
  }
})()
