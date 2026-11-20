void (async () => {
  const invoke = window.__TAURI_INTERNALS__.invoke
  const emit = (event, payload) => invoke('plugin:event|emit', { event, payload })
  const request = (method, path, body = null) => invoke('studio_request', { request: { method, path, body } })
  const assert = (condition, message) => { if (!condition) throw new Error(message) }
  const wait = async (condition, message) => {
    const end = Date.now() + 20000
    while (!condition()) {
      if (Date.now() > end) throw new Error(message)
      await new Promise(resolve => setTimeout(resolve, 50))
    }
  }
  const button = text => [...document.querySelectorAll('button')].find(item => item.getClientRects().length && item.textContent.trim() === text)
  const menu = () => [...document.querySelectorAll('[data-slot="studio-page"] [aria-label="设置"]')].find(item => item.getClientRects().length)
  const expected = ['通用', '模型', '技能', '外部 MCP 服务', '计算节点']
  try {
    await wait(() => button('设置'), '设置入口没有显示')
    button('设置').click()
    await wait(() => location.pathname === '/studio/settings/general', '设置没有默认打开通用页')
    for (const width of [1440, 760]) {
      await emit('studio:layout-size', [width, 900])
      await wait(() => innerWidth === width, '窗口尺寸没有更新')
      for (const label of expected) {
        const target = [...menu().querySelectorAll('button')].find(item => item.textContent.trim() === label)
        assert(target, `设置菜单缺少 ${label}`)
        target.click()
        await wait(() => menu()?.querySelector('[aria-current="page"]')?.textContent.trim() === label, `设置页面没有切换到 ${label}`)
        const labels = [...menu().querySelectorAll('button')].map(item => item.textContent.trim())
        assert(JSON.stringify(labels) === JSON.stringify(expected), `设置菜单不一致：${labels.join('、')}`)
      }
    }
    history.pushState({ ...history.state }, '', '/studio/settings/sessions')
    dispatchEvent(new PopStateEvent('popstate', { state: history.state }))
    await wait(() => location.pathname === '/studio/settings/general', '旧会话地址没有转到通用页')
    const session = await request('POST', '/api/v1/studio/sessions', { request_id: crypto.randomUUID() })
    await request('POST', '/api/v1/studio/assets/text', { session_id: session.id, name: '保留的项目资料', content: '清空对话后保留资产。', request_id: crypto.randomUUID() })
    button('模型').click()
    await wait(() => location.pathname.endsWith('/models'), '模型页面没有显示')
    button('通用').click()
    await wait(() => button('清空全部对话') && !button('清空全部对话').disabled, '清空对话操作不可用')
    button('清空全部对话').click()
    await wait(() => document.querySelector('[role="alertdialog"]'), '清空对话没有二次确认')
    const dialog = document.querySelector('[role="alertdialog"]')
    const confirm = [...dialog.querySelectorAll('button')].find(item => item.textContent.trim() === '清空全部对话')
    assert(confirm.disabled, '未输入确认内容即可清空对话')
    const input = dialog.querySelector('input')
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(input, '确认清空')
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await wait(() => !confirm.disabled, '确认内容没有生效')
    confirm.click()
    await wait(() => !document.querySelector('[role="alertdialog"]'), '清空对话确认没有关闭')
    assert((await request('GET', '/api/v1/studio/sessions')).length === 0, '对话没有清空')
    const projects = await request('GET', '/api/v1/studio/library/projects')
    assert(projects.find(project => project.id === '')?.asset_count === 1, '清空对话后资产没有保留')
    await wait(() => button('清空全部对话').disabled, '空数据仍可清空对话')
    await emit('studio:layout-result', { success: true, widths: [1440, 760], sections: expected, cleared: true, retainedAssets: 1 })
  } catch (error) {
    await emit('studio:layout-result', { success: false, message: error.message, location: location.href, pageText: document.body.innerText.slice(-1500) })
  }
})()
