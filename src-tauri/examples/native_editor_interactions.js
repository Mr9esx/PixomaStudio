void (async () => {
  const invoke = window.__TAURI_INTERNALS__.invoke
  const emit = (event, payload) => invoke('plugin:event|emit', { event, payload })
  const request = (method, path, body = null) => invoke('studio_request', { request: { method, path, body } })
  const assert = (condition, message) => { if (!condition) throw new Error(message) }
  const visible = element => {
    if (!element) return false
    if (element.closest('[inert],[aria-hidden="true"]')) return false
    const bounds = element.getBoundingClientRect()
    return bounds.width > 0 && bounds.height > 0 && bounds.right > 0 && bounds.left < innerWidth && getComputedStyle(element).visibility !== 'hidden'
  }
  const editor = () => [...document.querySelectorAll('[aria-label="视频编辑器"]')].find(visible)
  const aria = label => [...document.querySelectorAll('[aria-label]')].find(item => item.getAttribute('aria-label') === label && visible(item))
  const button = label => [...document.querySelectorAll('button')].find(item => visible(item) && item.textContent.trim() === label)
  const wait = async (condition, message) => {
    const deadline = Date.now() + 20000
    while (!await condition()) {
      if (Date.now() > deadline) throw new Error(message)
      await new Promise(resolve => setTimeout(resolve, 80))
    }
  }
  const clickTrack = async track => {
    const block = editor().querySelector(`[data-track="${track}"] button`)
    block.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0 }))
    block.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, button: 0 }))
    block.click()
    await wait(() => aria('分镜属性'), '属性面板未打开')
  }
  try {
    await wait(() => button('资产库'), '应用没有就绪')
    await emit('studio:layout-size', [1800, 1000])
    const session = await request('POST', '/api/v1/studio/sessions', { request_id: crypto.randomUUID() })
    const base = `/api/v1/studio/sessions/${session.id}/video-project`
    const response = await fetch('/.local-cache/editor-real-frame.png')
    assert(response.ok, '读取实际视频首帧失败')
    const image = await invoke('studio_upload', await response.arrayBuffer(), { headers: { 'x-pixoma-upload': encodeURIComponent(JSON.stringify({ file_name: '茶园首帧.png', session_id: session.id, request_id: crypto.randomUUID() })) } })
    const shots = [
      { id: crypto.randomUUID(), name: '开场', script: '', storyboard: '茶园远景', references: [], subtitle: '清晨', duration: 3, sourceIn: 0, inputs: {} },
      { id: crypto.randomUUID(), name: '结尾', script: '镜头移向茶杯', storyboard: '', references: [], subtitle: '', duration: 3, sourceIn: 0, inputs: {} },
    ]
    await request('PUT', base, { request_id: crypto.randomUUID(), expected_version: 0, project: { version: 2, name: '交互验收', width: 1280, height: 720, fps: 30, shots, audio: [] } })
    history.pushState({}, '', `/studio/sessions/${session.id}?panel=video`)
    dispatchEvent(new PopStateEvent('popstate'))
    await wait(() => editor()?.querySelectorAll('[data-track="video"]').length === 2, '分镜没有显示')
    const measurements = []
    await clickTrack('script')
    assert(aria('剧本内容') && !aria('字幕内容'), '剧本属性包含错误字段')
    const root = editor(), preview = root.querySelector('.sve-preview-pane'), assets = root.querySelector('.sve-assets-pane'), inspector = aria('分镜属性')
    assert(assets.getBoundingClientRect().right <= preview.getBoundingClientRect().left + 2, '资产区域没有位于预览左侧')
    assert(inspector.getBoundingClientRect().left >= preview.getBoundingClientRect().right - 2, '属性没有位于预览右侧')
    assert(root.querySelector('.sve-timeline-body').clientWidth >= root.clientWidth - 2, '时间轴未使用完整宽度')
    const separator = aria('调整预览与时间轴高度')
    const beforeHeight = root.querySelector('.sve-timeline-body').clientHeight
    separator.focus(); separator.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowUp', bubbles: true }))
    await wait(() => root.querySelector('.sve-timeline-body').clientHeight !== beforeHeight, '键盘不能调整时间轴高度')
    aria('关闭分镜属性').click()
    aria('下一帧').click()
    await wait(() => root.querySelector('[data-playhead]').dataset.playhead !== '0', '逐帧播放未更新')
    const beforeSelection = root.querySelector('[data-playhead]').dataset.playhead
    await clickTrack('subtitle')
    assert(aria('字幕内容') && !aria('剧本内容'), '字幕属性包含错误字段')
    assert(root.querySelector('[data-playhead]').dataset.playhead === beforeSelection, '选择对象改变了播放位置')
    aria('关闭分镜属性').click()
    root.querySelector('[data-track="script"] button').dispatchEvent(new MouseEvent('dblclick', { bubbles: true, button: 0 }))
    await wait(() => document.querySelector('#video-script-ideas'), '双击空剧本未打开想法输入')
    assert(button('发送到 Chat 生成').disabled, '空想法可以发送')
    button('取消').click()
    const target = root.querySelector('[data-track="reference"]')
    const bounds = target.getBoundingClientRect(), dataTransfer = new DataTransfer()
    const assetCard = aria(`预览资产：${image.name}`)?.closest('article')
    assert(assetCard, '项目资产中缺少参考图片')
    assetCard.dispatchEvent(new DragEvent('dragstart', { bubbles: true, cancelable: true, dataTransfer }))
    target.dispatchEvent(new DragEvent('drop', { bubbles: true, cancelable: true, dataTransfer, clientX: bounds.left + 20, clientY: bounds.top + 20 }))
    await wait(() => button('确认添加'), '拖放参考未打开用途选择')
    button('确认添加').click()
    await wait(async () => (await request('GET', base)).project.shots[0].references.length === 1, '拖放参考没有保存')
    aria('关闭分镜属性').click()
    const row = root.querySelector('.timeline-editor-edit-row'), block = root.querySelector('.sve-block')
    assert(getComputedStyle(row).borderBottomWidth === '1px', '轨道没有分隔线')
    assert(getComputedStyle(block).borderRadius === '0px' && getComputedStyle(block).borderRightWidth === '1px', '时间轴块样式无效')
    await emit('studio:layout-progress', { stage: '交互检查通过', referenceDrop: true, frameStep: true })
    for (const width of [1800, 1440, 1024, 760]) {
      await emit('studio:layout-size', [width, 1000])
      await wait(() => innerWidth === width, '窗口宽度没有更新')
      await new Promise(resolve => setTimeout(resolve, 250))
      if (!editor()) { aria('打开创作工作台').click(); await wait(editor, '窄窗口工作台没有打开') }
      const current = editor()
      assert(current.scrollWidth <= current.clientWidth + 1, '编辑器横向溢出')
      assert(current.querySelector('.sve-timeline-body').clientWidth >= current.clientWidth - 2, '窄窗口时间轴宽度不足')
      measurements.push({ window: width, editor: current.clientWidth, compact: current.querySelector('.sve-layout').dataset.compact })
      if (current.querySelector('.sve-layout').dataset.compact === 'true') {
        aria('打开项目资产').click()
        await wait(() => aria('项目资产库'), '资产侧边面板未打开')
        assert(aria(`资产操作：${image.name}`), '资产更多菜单缺失')
        document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
        await wait(() => !aria('项目资产库'), '资产面板没有关闭')
      }
    }
    await emit('studio:layout-result', { success: true, measurements, scopedInspector: true, scriptIdeas: true, referenceDrop: true, frameStep: true, separators: true })
  } catch (error) {
    await emit('studio:layout-result', { success: false, message: String(error), width: innerWidth, location: location.href, editorWidth: editor()?.clientWidth, dialogs: [...document.querySelectorAll('[role="dialog"]')].map(item => item.textContent.slice(0, 200)) })
  }
})()
