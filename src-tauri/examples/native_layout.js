void (async () => {
  if (window.__PIXOMA_LAYOUT_RUNNING__) return
  window.__PIXOMA_LAYOUT_RUNNING__ = true
  const invoke = window.__TAURI_INTERNALS__.invoke
  const emit = (event, payload) => invoke('plugin:event|emit', { event, payload })
  const request = (method, path, body = null) => invoke('studio_request', { request: { method, path, body } })
  const assert = (condition, message) => { if (!condition) throw new Error(message) }
  const wait = async (condition, message) => {
    const deadline = Date.now() + 20000
    while (!condition()) {
      const error = document.querySelector('[role="alert"] p')
      if (error) throw new Error(error.textContent.trim())
      if (Date.now() >= deadline) throw new Error(message)
      await new Promise(resolve => setTimeout(resolve, 50))
    }
    await new Promise(resolve => setTimeout(resolve, 50))
  }
  const navigate = async (path, ready) => {
    history.pushState({}, '', path)
    dispatchEvent(new PopStateEvent('popstate'))
    await wait(() => location.pathname === path && ready(), `页面没有就绪：${path}`)
  }
  const main = () => document.querySelector('[data-slot="studio-page"]')
  const header = () => main()?.querySelector('.studio-page-header')
  const heading = () => main()?.querySelector('h1')?.textContent.trim()
  const measurements = []
  let surfaceStyle
  try {
    await emit('studio:layout-progress', { stage: 'load' })
    await wait(() => document.getElementById('root')?.childElementCount > 0, '应用没有启动')
    await emit('studio:layout-progress', { stage: 'ready', visibility: document.visibilityState, pageText: document.body.innerText.slice(0, 800) })
    await wait(() => main() && heading() === '新对话', '新对话页面没有就绪')
    const sidebar = document.querySelector('[data-slot="studio-desktop-sidebar"]')
    const capabilityEntries = [...sidebar.querySelectorAll('[data-slot="sidebar-menu-button"]')].filter(button => button.textContent.trim() === '技能与专家')
    assert(capabilityEntries.length === 1, '侧栏未合并技能与专家入口')
    assert(!sidebar.textContent.includes('管理'), '侧栏菜单仍包含管理文案')
    capabilityEntries[0].click()
    await wait(() => document.querySelector('[role="tablist"][aria-label="技能与专家"]'), '技能与专家标签没有显示')
    const expertTab = [...document.querySelectorAll('[role="tab"]')].find(tab => tab.textContent.trim() === '专家')
    expertTab.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0, ctrlKey: false }))
    await wait(() => location.pathname === '/studio/experts' && [...document.querySelectorAll('[role="tab"]')].some(tab => tab.textContent.trim() === '专家' && tab.getAttribute('aria-selected') === 'true'), '专家标签没有切换')
    assert(capabilityEntries[0].getAttribute('data-active') === 'true', '专家页面未选中合并菜单')
    const skillTab = [...document.querySelectorAll('[role="tab"]')].find(tab => tab.textContent.trim() === '技能')
    skillTab.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0, ctrlKey: false }))
    await wait(() => location.pathname === '/studio/skills' && [...document.querySelectorAll('[role="tab"]')].some(tab => tab.textContent.trim() === '技能' && tab.getAttribute('aria-selected') === 'true'), '技能标签没有切换')
    const footer = sidebar.querySelector('[data-slot="sidebar-footer"]')
    assert(footer, '侧栏底部控件缺失')
    const themeSwitch = footer.querySelector('[role="switch"]')
    assert(themeSwitch, '侧栏主题切换入口缺失')
    const initialDark = document.documentElement.classList.contains('dark')
    themeSwitch.click()
    await wait(() => document.documentElement.classList.contains('dark') !== initialDark, '主题没有切换')
    assert(document.cookie.includes(`vite-ui-theme=${initialDark ? 'light' : 'dark'}`), '主题没有保存')
    themeSwitch.click()
    await wait(() => document.documentElement.classList.contains('dark') === initialDark, '主题没有恢复')
    const settings = [...footer.querySelectorAll('button')].find(button => button.textContent.trim() === '设置')
    assert(settings, '侧栏底部设置入口缺失')
    settings.click()
    await wait(() => location.pathname === '/studio/settings/general', '底部设置入口没有打开设置页面')
    for (const label of sidebar.querySelectorAll('[data-slot="sidebar-group-label"]')) {
      const control = label.querySelector('button[aria-expanded]')
      const hoverRules = [...document.styleSheets].flatMap(sheet => [...sheet.cssRules]).filter(rule => rule instanceof CSSStyleRule && rule.selectorText.endsWith(':hover') && control.matches(rule.selectorText.slice(0, -6)))
      assert(hoverRules.some(rule => rule.style.backgroundColor === 'transparent'), '分组标题缺少透明悬停背景样式')
    }
    const session = await request('POST', '/api/v1/studio/sessions', { request_id: crypto.randomUUID() })
    const pages = [
      ['/studio', () => heading() === '新对话'],
      [`/studio/sessions/${session.id}`, () => main()?.querySelector('[data-slot="studio-workbench"]')],
      ['/studio/library', () => main()?.querySelector('[data-slot="studio-library-toolbar"]')],
      ['/studio/workflows', () => heading()?.includes('工作流')],
      ['/studio/skills', () => heading()?.includes('技能')],
      ['/studio/experts', () => heading()?.includes('专家')],
      ['/studio/settings/models', () => main()?.querySelector('h2')?.textContent.trim() === '模型'],
      ['/studio/settings/skills', () => main()?.querySelector('h2')?.textContent.trim() === '技能'],
      ['/studio/settings/mcp', () => main()?.querySelector('h2')?.textContent.trim() === '外部 MCP 服务'],
      ['/studio/settings/compute-nodes', () => main()?.querySelector('h2')?.textContent.trim() === '计算节点'],
      ['/studio/settings/general', () => main()?.querySelector('[aria-current="page"]')?.textContent.trim() === '通用'],
      ['/studio/expert-editor/new', () => heading() === '创建专家'],
    ]
    const headerHeight = matchMedia('(pointer: coarse)').matches ? 76 : 64
    for (const width of [1440, 760]) {
      await emit('studio:layout-size', [width, 800])
      await wait(() => innerWidth === width, `窗口宽度没有变为 ${width}`)
      for (const [path, ready] of pages) {
        await navigate(path, ready)
        const surface = main()
        if (path.startsWith('/studio/settings/')) {
          const navigation = [...surface.querySelectorAll('[aria-label="设置"]')].find(item => item.getClientRects().length)
          assert(navigation, `设置菜单没有显示：${path}`)
          const labels = [...navigation.querySelectorAll('button')].map(item => item.textContent.trim())
          assert(labels.join(',') === '通用,模型,技能,外部 MCP 服务,计算节点', `设置菜单不一致：${path}`)
          assert(labels.filter(label => label === '计算节点').length === 1, `计算节点入口数量错误：${path}`)
          assert(new Set(labels).size === labels.length, `设置菜单存在重复项：${path}`)
        }
        const bounds = surface.getBoundingClientRect()
        const style = getComputedStyle(surface)
        assert(document.querySelectorAll('main#main-content').length === 1, `页面主体数量错误：${path}`)
        assert(Math.abs(bounds.top - 16) < 1 && Math.abs(innerHeight - bounds.bottom - 16) < 1, `页面纵向间距错误：${path}`)
        assert(Math.abs(innerWidth - bounds.right - 16) < 1, `页面右侧间距错误：${path}`)
        assert(style.borderTopWidth === '1px' && parseFloat(style.borderRadius) > 0, `页面边框或圆角错误：${path}`)
        const appearance = `${style.borderRadius};${style.backgroundColor};${style.borderColor}`
        surfaceStyle ??= appearance
        assert(appearance === surfaceStyle, `页面表面样式不一致：${path}`)
        assert(surface.scrollWidth <= surface.clientWidth, `页面出现横向溢出：${path}`)
        const bars = [...surface.querySelectorAll('.studio-page-header')]
        for (const bar of bars) {
          const area = bar.getBoundingClientRect()
          assert(Math.abs(area.height - headerHeight) < 1, `页面顶部高度错误：${path}，${area.height}`)
          assert(bar.dataset.tauriDragRegion === 'deep', `页面缺少窗口拖动区域：${path}`)
          for (const button of bar.querySelectorAll('button')) {
            if (!button.getClientRects().length) continue
            const buttonBounds = button.getBoundingClientRect()
            assert(buttonBounds.left >= area.left && buttonBounds.right <= area.right + 1, `顶部按钮超出页面：${path}`)
          }
        }
        measurements.push({ path, width, top: bounds.top, right: innerWidth - bounds.right, height: bounds.height, headerHeights: bars.map(bar => bar.getBoundingClientRect().height), background: style.backgroundColor })
      }
      if (width === 760) {
        await navigate('/studio/library', () => main()?.querySelector('[data-slot="studio-library-toolbar"]'))
        document.querySelector('[aria-label="打开 Studio 菜单"]').click()
        await wait(() => document.querySelector('[role="dialog"] .studio-sidebar-header'), '左侧菜单没有打开')
        const brand = document.querySelector('[role="dialog"] .studio-sidebar-header img')
        assert(brand.getBoundingClientRect().top >= 44, '左侧菜单品牌进入窗口按钮区域')
        const close = [...document.querySelectorAll('[role="dialog"] button')].find(button => button.textContent.trim() === '关闭')
        assert(close, '左侧菜单缺少关闭按钮')
        close.click()
        await wait(() => !document.querySelector('[role="dialog"]'), '左侧菜单没有关闭')
      }
    }
    await navigate('/studio/settings/models', () => main()?.querySelector('h2')?.textContent.trim() === '模型')
    const initialWidth = innerWidth
    const initialHeight = innerHeight
    const isMaximized = () => invoke('plugin:window|is_maximized', { label: 'main' })
    assert(!await isMaximized(), '窗口在双击检查前已经缩放')
    const doubleClick = () => {
      const bar = header()
      const bounds = bar.getBoundingClientRect()
      const options = { bubbles: true, button: 0, detail: 2, clientX: Math.round(bounds.right - 32), clientY: Math.round(bounds.top + 32) }
      bar.dispatchEvent(new MouseEvent('mousedown', options))
      bar.dispatchEvent(new MouseEvent('mouseup', options))
    }
    doubleClick()
    await wait(() => innerWidth !== initialWidth || innerHeight !== initialHeight, '双击顶部没有改变窗口尺寸')
    const maximized = [innerWidth, innerHeight]
    const zoomed = await isMaximized()
    assert(zoomed, '双击顶部没有进入原生缩放状态')
    doubleClick()
    await wait(() => innerWidth === initialWidth && innerHeight === initialHeight, '双击顶部没有恢复窗口尺寸')
    const restoredMaximized = await isMaximized()
    assert(!restoredMaximized, '双击顶部没有退出原生缩放状态')
    await emit('studio:layout-result', { success: true, measurements, maximized, zoomed, restoredMaximized, restored: [innerWidth, innerHeight], screen: [screen.availWidth, screen.availHeight] })
  } catch (error) {
    const dialogs = [...document.querySelectorAll('[role="dialog"]')].map(dialog => {
      const style = getComputedStyle(dialog)
      return { state: dialog.dataset.state, opacity: style.opacity, display: style.display, pointerEvents: style.pointerEvents, animations: dialog.getAnimations().map(animation => ({ playState: animation.playState, currentTime: animation.currentTime, name: animation.animationName })) }
    })
    await emit('studio:layout-result', { success: false, message: error instanceof Error ? error.message : JSON.stringify(error), location: location.href, visibility: document.visibilityState, menuExpanded: document.querySelector('[aria-label="打开 Studio 菜单"]')?.getAttribute('aria-expanded'), dialogs, pageText: document.body.innerText.slice(0, 800), measurements })
  }
})()
