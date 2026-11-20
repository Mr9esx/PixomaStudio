void (async () => {
  const invoke = window.__TAURI_INTERNALS__.invoke
  const emit = (event, payload) => invoke('plugin:event|emit', { event, payload })
  const request = (method, path, body = null) => invoke('studio_request', { request: { method, path, body } })
  const assert = (condition, message) => { if (!condition) throw new Error(message) }
  const wait = async (condition, message) => {
    const deadline = Date.now() + 20000
    while (!condition()) { if (Date.now() > deadline) throw new Error(message); await new Promise(resolve => setTimeout(resolve, 50)) }
  }
  const button = label => [...document.querySelectorAll('button')].find(item => item.getClientRects().length && item.textContent.trim() === label)
  try {
    await wait(() => button('资产库'), '应用未就绪')
    const session = await request('POST', '/api/v1/studio/sessions', { request_id: crypto.randomUUID() })
    await request('POST', '/api/v1/studio/assets/text', { session_id: session.id, name: '资产预览检查', content: '# 资产预览检查\n\n通过卡片打开完整内容。', request_id: crypto.randomUUID() })
    button('资产库').click()
    await wait(() => document.querySelector('[data-slot="library-project-card"][data-project-id=""]'), '未归属文件夹没有显示')
    document.querySelector('[data-slot="library-project-card"][data-project-id=""]').click()
    await wait(() => document.querySelector('[data-slot="studio-asset-card"]'), '资产卡片未显示')
    const card = document.querySelector('[data-slot="studio-asset-card"]')
    assert(![...card.querySelectorAll('button')].some(item => item.textContent.trim() === '查看'), '资产卡片仍包含独立查看按钮')
    const open = card.querySelector('[data-slot="asset-open"]')
    assert(open, '卡片缺少直接预览入口')
    const measurements = []
    for (const width of [1440, 1024, 760]) {
      await emit('studio:layout-size', [width, 900])
      await wait(() => innerWidth === width, '窗口尺寸没有更新')
      const menuTrigger = card.querySelector('button[aria-label="更多操作"]')
      assert(menuTrigger?.getClientRects().length, '文件操作菜单没有显示')
      menuTrigger.focus()
      menuTrigger.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }))
      await wait(() => document.querySelector('[role="menu"]'), '文件操作菜单没有打开')
      assert([...document.querySelectorAll('[role="menuitem"]')].some(item => item.textContent.includes('下载资产')), '文件操作缺少下载入口')
      assert(!document.querySelector('[data-slot="asset-inspector"]'), '文件操作触发了资产预览')
      document.querySelector('[role="menu"]').dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }))
      await wait(() => !document.querySelector('[role="menu"]'), '文件操作菜单没有关闭')
      assert(!card.querySelector('[role="checkbox"]'), '默认显示资产选择控件')
      button('选择').click()
      await wait(() => card.querySelector('[role="checkbox"]'), '选择模式没有开启')
      const selection = card.querySelector('[role="checkbox"]')
      selection.click()
      await wait(() => selection.getAttribute('aria-checked') === 'true', '多选没有生效')
      assert(!document.querySelector('[data-slot="asset-inspector"]'), '多选触发了资产预览')
      button('退出选择').click()
      await wait(() => !card.querySelector('[role="checkbox"]'), '选择模式没有关闭')
      open.focus()
      open.click()
      await wait(() => document.querySelector('[data-slot="asset-preview"]'), '大预览没有显示')
      assert(!document.querySelector('[role="dialog"]'), '资产预览显示模态窗口')
      const preview = document.querySelector('[data-slot="asset-preview"]').getBoundingClientRect()
      const inspector = document.querySelector('[data-slot="asset-inspector"]')
      const main = document.querySelector('[data-slot="studio-page"]')
      const behind = document.querySelector('[data-slot="studio-library-toolbar"] button')
      assert(behind, '资产操作栏缺失')
      behind.focus()
      assert(document.activeElement !== behind, '预览后仍可聚焦被覆盖的操作')
      assert(preview.width >= 450 && preview.height >= 400, `预览区域过小：${preview.width} × ${preview.height}`)
      assert(inspector.scrollWidth <= inspector.clientWidth && main.scrollWidth <= main.clientWidth, '资产预览出现横向溢出')
      assert(inspector.querySelector('[aria-label="重命名资产"]'), '资产属性操作缺失')
      document.querySelector('[aria-label="放大资产"]').click()
      await wait(() => document.querySelector('[aria-label="返回资产详情"]'), '放大操作没有生效')
      assert(document.querySelector('[data-slot="asset-preview"]').getBoundingClientRect().width >= preview.width, '放大后预览缩小')
      document.querySelector('[aria-label="返回资产详情"]').click()
      await wait(() => document.querySelector('[aria-label="重命名资产"]'), '属性区域没有恢复')
      inspector.focus()
      const escape = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })
      inspector.dispatchEvent(escape)
      await wait(() => !document.querySelector('[data-slot="asset-inspector"]'), 'Escape 没有返回资产列表')
      await wait(() => document.activeElement === open, '返回后焦点没有恢复')
      assert(!card.querySelector('[role="checkbox"]'), '返回后重新开启选择模式')
      measurements.push({ width, previewWidth: preview.width, previewHeight: preview.height })
    }
    const imageCard = [...document.querySelectorAll('[data-slot="studio-asset-card"]')].find(item => item.querySelector('img'))
    let image = null
    if (imageCard) {
      imageCard.querySelector('[data-slot="asset-open"]').click()
      await wait(() => document.querySelector('[data-slot="asset-preview"] img')?.naturalWidth > 0, '图片预览没有读取真实内容')
      const previewImage = document.querySelector('[data-slot="asset-preview"] img')
      assert(getComputedStyle(previewImage).objectFit === 'contain', '图片预览没有保留完整画面')
      image = { width: previewImage.naturalWidth, height: previewImage.naturalHeight }
    }
    await emit('studio:layout-result', { success: true, measurements, image })
  } catch (error) {
    await emit('studio:layout-result', { success: false, message: error.message, location: location.href, pageText: document.body.innerText.slice(-1200) })
  }
})()
