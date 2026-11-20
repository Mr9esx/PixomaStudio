void (async () => {
  const invoke = window.__TAURI_INTERNALS__.invoke
  const emit = (event, payload) => invoke('plugin:event|emit', { event, payload })
  const request = (method, path, body = null) => invoke('studio_request', { request: { method, path, body } })
  const assert = (condition, message) => { if (!condition) throw new Error(message) }
  const visible = element => {
    const bounds = element.getBoundingClientRect()
    let left = Math.max(0, bounds.left), right = Math.min(innerWidth, bounds.right)
    for (let parent = element.parentElement; parent; parent = parent.parentElement) {
      const style = getComputedStyle(parent)
      if (style.display === 'none' || style.visibility === 'hidden') return false
      if (style.overflowX !== 'visible') {
        const clip = parent.getBoundingClientRect()
        left = Math.max(left, clip.left); right = Math.min(right, clip.right)
      }
    }
    return right > left && bounds.height > 0
  }
  const editor = () => [...document.querySelectorAll('[aria-label="视频编辑器"]')].find(visible)
  const aria = label => editor()?.querySelector(`[aria-label="${label}"]`)
  const button = label => [...document.querySelectorAll('button')].find(item => visible(item) && item.textContent.trim() === label)
  const wait = async (condition, message) => {
    const deadline = Date.now() + 20000
    while (!await condition()) {
      if (Date.now() > deadline) throw new Error(message)
      await new Promise(resolve => setTimeout(resolve, 80))
    }
  }
  try {
    await wait(() => button('资产库'), '应用没有就绪')
    const session = await request('POST', '/api/v1/studio/sessions', { request_id: crypto.randomUUID() })
    let image
    const imageURL = window.__PIXOMA_EDITOR_IMAGE_URL__ ?? '/.local-cache/editor-real-frame.png'
    if (imageURL) {
      const response = await fetch(imageURL)
      assert(response.ok, '读取实际首帧图片失败')
      image = await invoke('studio_upload', await response.arrayBuffer(), { headers: { 'x-pixoma-upload': encodeURIComponent(JSON.stringify({ file_name: '茶园首帧.png', session_id: session.id, request_id: crypto.randomUUID() })) } })
    }
    history.pushState({}, '', `/studio/sessions/${session.id}?panel=video`)
    dispatchEvent(new PopStateEvent('popstate'))
    await wait(() => button('创建十五秒茶品牌短片 · 五个分镜')?.disabled === false, '视频编辑器没有就绪')
    button('创建十五秒茶品牌短片 · 五个分镜').click()
    await wait(() => editor()?.querySelectorAll('[data-track="video"]').length >= 2, '分镜没有显示')
    const measurements = []
    for (const width of [1800, 1440, 1024, 760]) {
      await emit('studio:layout-size', [width, 900])
      await wait(() => innerWidth === width, '窗口宽度没有更新')
      await new Promise(resolve => setTimeout(resolve, 200))
      if (width < 1280 && !editor()) {
        document.querySelector('[aria-label="打开创作工作台"]').click()
        await wait(() => editor(), '窄窗口工作台没有打开')
      }
      await new Promise(resolve => setTimeout(resolve, 200))
      const root = editor()
      const preview = root.querySelector('.sve-preview-pane')
      const assets = root.querySelector('.sve-assets-pane')
      const timeline = root.querySelector('.sve-timeline-body')
      const before = preview.getBoundingClientRect()
      const assetBounds = assets.getBoundingClientRect()
      assert(assetBounds.left >= before.right - 1 && Math.abs(assetBounds.top - before.top) < 1, '视频预览与项目资产没有左右排列')
      assert(timeline.getBoundingClientRect().top >= before.bottom - 1, '时间轴没有显示在预览与资产下方')
      assert(timeline.clientWidth >= root.clientWidth - 2, '时间轴没有使用完整宽度')
      aria('收起项目资产').click()
      await wait(() => assets.dataset.collapsed === 'true', '项目资产没有收起')
      assert(preview.clientWidth > before.width + 80, '视频预览没有使用资产腾出的空间')
      aria('展开项目资产').click()
      await wait(() => assets.dataset.collapsed === 'false', '项目资产没有展开')
      aria('收起视频预览').click()
      await wait(() => preview.dataset.collapsed === 'true', '视频预览没有收起')
      assert(assets.clientWidth > assetBounds.width + 80, '项目资产没有使用预览腾出的空间')
      if (image) {
        await wait(() => root.querySelector('.sve-asset-card img')?.naturalWidth > 0, '资产小卡片没有显示实际图片')
        aria(`预览资产：${image.name}`).click()
        await wait(() => preview.dataset.collapsed === 'false' && root.querySelector('.sve-preview img')?.naturalWidth > 0, '资产卡片没有展开图片预览')
        button('返回视频预览').click()
      } else {
        aria('展开视频预览').click()
        await wait(() => preview.dataset.collapsed === 'false', '视频预览没有展开')
      }
      assert(root.scrollWidth <= root.clientWidth, '视频编辑器出现横向溢出')
      measurements.push({ width, previewWidth: preview.clientWidth, assetsWidth: assets.clientWidth, timelineWidth: timeline.clientWidth })
    }
    await emit('studio:layout-result', { success: true, measurements, actualImage: Boolean(image), assetCards: true, independentCollapse: true })
  } catch (error) {
    await emit('studio:layout-result', { success: false, message: String(error), width: innerWidth, editors: [...document.querySelectorAll('[aria-label="视频编辑器"]')].map(root => ({ width: root.clientWidth, scroll: root.scrollWidth, bounds: root.getBoundingClientRect().width, visibility: getComputedStyle(root).visibility, children: [...root.children].map(child => ({ name: child.className, width: child.clientWidth, scroll: child.scrollWidth })) })), location: location.href })
  }
})()
