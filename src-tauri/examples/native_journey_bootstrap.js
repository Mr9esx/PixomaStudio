void (async () => {
  const root = document.getElementById('root')
  if (!root) throw new Error('应用挂载位置不存在')
  if (root.childElementCount === 0) {
    await new Promise((resolve) => {
      const observer = new MutationObserver(() => {
        if (root.childElementCount > 0) {
          observer.disconnect()
          resolve()
        }
      })
      observer.observe(root, { childList: true })
    })
  }
  await import('/src-tauri/examples/native_journey.js')
})().catch((error) => {
  void window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {
    event: 'studio:journey-result',
    payload: { success: false, location: location.href, message: `加载联调模块失败：${String(error)}` },
  })
})
