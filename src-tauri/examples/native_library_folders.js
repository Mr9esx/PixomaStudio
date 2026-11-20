void (async () => {
  const invoke = window.__TAURI_INTERNALS__.invoke
  const emit = (event, payload) => invoke('plugin:event|emit', { event, payload })
  const request = (method, path, body = null) => invoke('studio_request', { request: { method, path, body } })
  const assert = (condition, message) => { if (!condition) throw new Error(message) }
  const wait = async (condition, message) => {
    const deadline = Date.now() + 40000
    while (!condition()) { if (Date.now() > deadline) throw new Error(message); await new Promise(resolve => setTimeout(resolve, 50)) }
  }
  const button = label => [...document.querySelectorAll('button')].find(item => item.getClientRects().length && item.textContent.trim() === label)
  const closeMenu = async () => {
    await wait(() => document.querySelector('[role="menu"]')?.contains(document.activeElement), '操作菜单没有取得焦点')
    const escape = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })
    document.activeElement.dispatchEvent(escape)
    await emit('studio:layout-progress', '已发送菜单关闭按键')
    await wait(() => !document.querySelector('[role="menu"]'), '操作菜单没有关闭')
  }
  try {
    await wait(() => button('资产库'), '应用未就绪')
    const project = await request('POST', '/api/v1/studio/projects', { name: '文件夹交互检查' })
    const session = await request('POST', '/api/v1/studio/sessions', { request_id: crypto.randomUUID() })
    await request('PATCH', `/api/v1/studio/sessions/${session.id}/project`, { project_id: project.id })
    await request('POST', '/api/v1/studio/assets/text', { session_id: session.id, name: '项目文档', content: '# 项目文档\n\n文件夹中的完整内容。', request_id: crypto.randomUUID() })
    await emit('studio:layout-progress', { stage: '项目和文档已创建', projects: await request('GET', '/api/v1/studio/library/projects') })
    button('资产库').click()
    await wait(() => document.querySelector('[data-slot="library-project-card"]'), '资产库没有展示项目文件夹')
    const projectToolbar = document.querySelector('[data-slot="studio-library-toolbar"]')
    assert(projectToolbar.querySelector('[aria-label="搜索项目文件夹"]') && [...projectToolbar.querySelectorAll('button')].some(item => item.textContent.includes('新建项目')), '项目搜索与新建项目没有位于同一操作栏')
    assert(!projectToolbar.querySelector('h1, h2'), '项目操作栏仍有独立标题')
    assert(!document.querySelector('[data-slot="studio-asset-card"]'), '文件夹首页直接展示资产')
    const folder = document.querySelector(`[data-slot="library-project-card"][data-project-id="${project.id}"]`)
    assert(folder, '项目文件夹缺失')
    folder.click()
    await wait(() => document.querySelector('[data-slot="studio-asset-card"]'), '项目资产没有显示')
    await emit('studio:layout-progress', '文件夹导航通过')
    let card = document.querySelector('[data-slot="studio-asset-card"]')
    assert(!card.querySelector('[role="checkbox"]'), '资产卡片默认显示勾选控件')
    button('选择').click()
    await wait(() => document.querySelector('[data-slot="studio-asset-card"] [role="checkbox"]'), '选择模式没有显示勾选控件')
    card = document.querySelector('[data-slot="studio-asset-card"]')
    card.querySelector('[data-slot="asset-open"]').click()
    await wait(() => card.querySelector('[role="checkbox"]').getAttribute('aria-checked') === 'true', '选择模式点击卡片没有选中资产')
    assert(!document.querySelector('[data-slot="asset-inspector"]'), '选择模式打开了详情')
    button('退出选择').click()
    await wait(() => !document.querySelector('[data-slot="studio-asset-card"] [role="checkbox"]'), '退出选择后仍显示勾选控件')
    card = document.querySelector('[data-slot="studio-asset-card"]')
    card.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, button: 2, clientX: 700, clientY: 250 }))
    await wait(() => document.querySelector('[role="menu"]'), '右键菜单没有打开')
    assert([...document.querySelectorAll('[role="menuitem"]')].some(item => item.textContent.includes('下载资产')), '右键菜单缺少下载入口')
    assert(!document.querySelector('[data-slot="asset-inspector"]'), '右键打开了详情')
    await closeMenu()
    await emit('studio:layout-progress', '选择模式和右键菜单通过')
    const measurements = []
    for (const width of [1440, 1024, 760]) {
      await emit('studio:layout-size', [width, 900])
      await wait(() => innerWidth === width, '窗口尺寸没有更新')
      const breadcrumb = document.querySelector('[data-slot="library-breadcrumb"]').getBoundingClientRect()
      const toolbar = document.querySelector('[data-slot="studio-library-toolbar"]').getBoundingClientRect()
      assert(breadcrumb.bottom <= toolbar.top, '项目面包屑没有位于操作栏上方')
      assert(toolbar.right <= innerWidth && toolbar.left >= 0, '项目操作栏超出窗口')
      card = document.querySelector('[data-slot="studio-asset-card"]')
      const open = card.querySelector('[data-slot="asset-open"]')
      open.focus()
      open.click()
      await wait(() => document.querySelector('[data-slot="asset-preview"]'), '大预览没有显示')
      const inspector = document.querySelector('[data-slot="asset-inspector"]')
      const preview = inspector.querySelector('[data-slot="asset-preview"]').getBoundingClientRect()
      assert(preview.width >= 450 && preview.height >= 400, '资产预览区域过小')
      assert(inspector.scrollWidth <= inspector.clientWidth, '资产详情出现横向溢出')
      if (width === 1440) await wait(() => document.querySelector('[role="treeitem"][data-asset-id][aria-selected="true"]'), '文件树没有选中当前资产')
      if (width === 1440) {
        for (const mode of ['format', 'category']) {
          document.querySelector('[aria-label="设置文件树"]').click()
          await wait(() => document.querySelector(`[role="radio"][value="${mode}"]`), '文件树分组设置没有打开')
          document.querySelector(`[role="radio"][value="${mode}"]`).click()
          await wait(() => document.querySelector(`[data-group-id^="${mode}:"][aria-expanded="true"] [data-asset-id][aria-selected="true"]`), '分组文件树没有展开当前资产路径')
        }
        inspector.querySelector('[aria-label="放大资产"]').click()
        await wait(() => inspector.querySelector('[aria-label="返回资产详情"]'), '放大资产没有生效')
        inspector.querySelector('[aria-label="重命名资产"]').click()
        await wait(() => inspector.querySelector('[aria-label="资产名称"]'), '放大状态下重命名没有显示输入框')
        inspector.querySelector('[aria-label="资产名称"]').closest('form').requestSubmit()
        await wait(() => !inspector.querySelector('[aria-label="资产名称"]'), '资产名称没有保存')
      }
      inspector.focus()
      inspector.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }))
      await wait(() => !document.querySelector('[data-slot="asset-inspector"]'), 'Escape 没有关闭详情')
      await wait(() => document.activeElement === open, '关闭详情没有恢复焦点')
      measurements.push({ width, previewWidth: preview.width, previewHeight: preview.height })
    }
    document.querySelector('[data-slot="library-breadcrumb"] button').click()
    await wait(() => document.querySelector('[data-slot="library-project-card"]'), '面包屑没有返回项目文件夹')
    await emit('studio:layout-result', { success: true, measurements })
  } catch (error) {
    await emit('studio:layout-result', { success: false, message: error.message, location: location.href, pageText: document.body.innerText.slice(-1200) })
  }
})()
