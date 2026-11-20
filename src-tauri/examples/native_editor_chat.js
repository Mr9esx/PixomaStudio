void (async () => {
  const invoke = window.__TAURI_INTERNALS__.invoke
  const emit = (event, payload) => invoke('plugin:event|emit', { event, payload })
  const request = (method, path, body = null) => invoke('studio_request', { request: { method, path, body } })
  const assert = (condition, message) => { if (!condition) throw new Error(message) }
  const visible = element => Boolean(element && !element.closest('[inert],[aria-hidden="true"]') && element.getBoundingClientRect().width && element.getBoundingClientRect().height)
  const button = label => [...document.querySelectorAll('button')].find(element => visible(element) && element.textContent.trim() === label)
  const wait = async (condition, message, timeout = 30000) => {
    const end = Date.now() + timeout
    while (!await condition()) {
      if (Date.now() > end) throw new Error(message)
      await new Promise(resolve => setTimeout(resolve, 150))
    }
  }
  const typeIdeas = value => {
    const input = document.querySelector('#video-script-ideas')
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(input, value)
    input.dispatchEvent(new Event('input', { bubbles: true }))
    input.dispatchEvent(new Event('change', { bubbles: true }))
  }
  try {
    await wait(() => button('资产库'), '应用未就绪')
    await emit('studio:layout-progress', { stage: '应用已经就绪' })
    await emit('studio:layout-size', [1800, 1000])
    const session = await request('POST', '/api/v1/studio/sessions', { request_id: crypto.randomUUID() })
    await emit('studio:layout-progress', { stage: '会话已创建', session: session.id })
    const base = `/api/v1/studio/sessions/${session.id}`
    const shots = [
      { id: crypto.randomUUID(), name: '山间开场', script: '', storyboard: '清晨远景', references: [], subtitle: '山间晨光', duration: 3, sourceIn: 0, inputs: {} },
      { id: crypto.randomUUID(), name: '人物进场', script: '', storyboard: '人物近景', references: [], subtitle: '新的一天', duration: 4, sourceIn: 0, inputs: {} },
    ]
    await request('PUT', `${base}/video-project`, { request_id: crypto.randomUUID(), expected_version: 0, project: { version: 2, name: '剧本交互验收', width: 1280, height: 720, fps: 30, shots, audio: [] } })
    history.pushState({}, '', `/studio/sessions/${session.id}?panel=video`); dispatchEvent(new PopStateEvent('popstate'))
    await wait(() => document.querySelector(`[data-shot-id="${shots[0].id}"][data-track="script"] button`), '分镜未显示')
    const composer = [...document.querySelectorAll('[contenteditable="true"]')].find(visible)
    assert(composer, 'Chat 输入框缺失')
    composer.focus(); document.execCommand('insertText', false, '稍后继续编辑的 Chat 草稿')
    await wait(() => composer.textContent.includes('稍后继续编辑的 Chat 草稿'), 'Chat 草稿未写入')
    const ideas = ['三秒开场，清晨茶园薄雾散开，安静自然，无对白。', '四秒钟，一位年轻人端起热茶，望向窗外，表现期待，无对白。']
    for (const [index, shot] of shots.entries()) {
      document.querySelector(`[data-shot-id="${shot.id}"][data-track="script"] button`).dispatchEvent(new MouseEvent('dblclick', { bubbles: true, button: 0 }))
      await wait(() => document.querySelector('#video-script-ideas'), '想法窗口未打开')
      if (button('安装视频剧本 Skill')) {
        button('安装视频剧本 Skill').click()
        await wait(() => document.querySelector('[aria-label="剧本 Skill"]')?.textContent.includes('video-script'), '剧本 Skill 安装未完成')
      }
      typeIdeas(ideas[index])
      await wait(() => button('发送到 Chat 生成')?.disabled === false, '发送按钮未启用')
      button('发送到 Chat 生成').click()
      await wait(() => !document.querySelector('#video-script-ideas'), '剧本请求未进入 Chat')
      assert(composer.textContent.includes('稍后继续编辑的 Chat 草稿'), '编辑器请求清除了 Chat 草稿')
      await emit('studio:layout-progress', { stage: '已发送剧本请求', shot: shot.id, session: session.id })
    }
    let saved, detail
    await wait(async () => {
      saved = await request('GET', `${base}/video-project`)
      detail = await request('GET', base)
      if (detail.session.latest_run?.status === 'failed') throw new Error(`模型请求失败：${detail.session.latest_run.error_message ?? '读取运行详情'}`)
      if (detail.pending_approvals?.length && button('批准') && !button('批准').disabled) button('批准').click()
      return saved.project.shots.every(shot => shot.script.length > 10)
    }, '真实模型没有完成两个分镜的剧本写入', 120000)
    for (const [index, original] of shots.entries()) {
      const result = saved.project.shots[index]
      assert(result.id === original.id && result.subtitle === original.subtitle && result.storyboard === original.storyboard && result.duration === original.duration, '生成改写了请求范围外的内容')
    }
    const users = detail.transcript.messages.filter(message => message.role === 'user')
    assert(users.length === 2, '请求出现重复发送或未发送')
    assert(users.every(message => message.parts?.some(part => part.type === 'skill_ref')), 'Chat 请求没有引用剧本 Skill')
    assert(composer.textContent.includes('稍后继续编辑的 Chat 草稿'), '生成完成后 Chat 草稿被清除')
    await emit('studio:layout-result', { success: true, session: session.id, actualModel: detail.session.model_config_id, actualSkill: true, queuedRequests: 2, draftRetained: true, scopeRetained: true, scripts: saved.project.shots.map(shot => ({ id: shot.id, script: shot.script })) })
  } catch (error) {
    await emit('studio:layout-result', { success: false, message: String(error), location: location.href, alerts: [...document.querySelectorAll('[role="alert"]')].map(item => item.textContent) })
  }
})()
