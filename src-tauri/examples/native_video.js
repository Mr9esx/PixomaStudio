void (async () => {
  const invoke = window.__TAURI_INTERNALS__.invoke
  const emit = payload => invoke('plugin:event|emit', { event: 'studio:video-progress', payload })
  const request = (method, path, body = null) => invoke('studio_request', { request: { method, path, body } })
  const assert = (condition, message) => { if (!condition) throw new Error(message) }
  const wait = async (condition, message, timeout = 90000) => {
    const deadline = Date.now() + timeout
    while (!await condition()) {
      if (Date.now() > deadline) throw new Error(message)
      await new Promise(resolve => setTimeout(resolve, 150))
    }
  }
  const button = label => [...document.querySelectorAll('button')].find(element => element.textContent.trim() === label)
  const projectPath = `/api/v1/studio/sessions/${window.__PIXOMA_VIDEO_SESSION__}/video-project`
  await wait(() => document.querySelector('[data-slot="studio-page"]'), 'Studio 启动超时')
  const path = `/studio/sessions/${window.__PIXOMA_VIDEO_SESSION__}?panel=video`
  history.pushState({}, '', path)
  dispatchEvent(new PopStateEvent('popstate'))
  await wait(() => document.querySelector('[aria-label="视频编辑器"]'), '视频编辑器没有打开')
  if (window.__PIXOMA_VIDEO_MODE__ === 'browse') {
    await emit({stage:'ready',session_id:window.__PIXOMA_VIDEO_SESSION__,path})
    return
  }
  await wait(() => document.querySelectorAll('[aria-label^="视频："]').length === 5, '时间轴没有显示五个分镜')
  const original = await request('GET', projectPath)
  assert(original.project.shots.every(shot => shot.media), '五个分镜尚未全部采用视频')
  assert(!document.querySelector('[aria-label="分镜属性"]'), '分镜属性没有按需展开')
  const first = original.project.shots[0]
  const workflow = await request('GET', `/api/v1/studio/workflows/${first.workflowId}/versions/${first.versionId}`)
  const nodeId = workflow.definition.execution.node_id
  await wait(async () => (await request('GET', `/api/v1/studio/compute-nodes/${nodeId}`)).healthy, '自建H3计算节点未就绪')
  document.querySelector(`[aria-label="视频：${first.name}"]`).click()
  await wait(() => button('重新生成') && !button('重新生成').disabled, '完成视频后没有显示可用的重新生成动作')
  await wait(() => document.querySelector('.sve-preview video')?.readyState >= 2, '真实分镜视频没有加载')
  assert(document.querySelector('[aria-label="时间轴播放游标"]'), '播放游标缺失')
  await emit({ stage: 'editor-open', session_id: window.__PIXOMA_VIDEO_SESSION__, path })
  document.querySelector(`[aria-label="字幕：${first.name}"]`).dispatchEvent(new MouseEvent('dblclick', { bubbles: true }))
  await wait(() => document.querySelector('[aria-label="编辑字幕"]'), '时间轴字幕没有进入编辑')
  const textarea = document.querySelector('[aria-label="编辑字幕"]')
  Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(textarea, '晨雾之间，茶香初醒')
  textarea.dispatchEvent(new Event('input', { bubbles: true }))
  textarea.dispatchEvent(new Event('change', { bubbles: true }))
  textarea.focus()
  textarea.blur()
  await wait(async () => (await request('GET', projectPath)).project.shots[0].subtitle === '晨雾之间，茶香初醒', '字幕编辑没有保存')
  document.querySelector('[aria-label="撤销视频编辑"]').click()
  await wait(async () => (await request('GET', projectPath)).project.shots[0].subtitle === first.subtitle, '撤销没有恢复原字幕')
  document.querySelector('[aria-label="重做视频编辑"]').click()
  await wait(async () => (await request('GET', projectPath)).project.shots[0].subtitle === '晨雾之间，茶香初醒', '重做没有恢复字幕')
  document.querySelector('[aria-label="撤销视频编辑"]').click()
  await wait(async () => (await request('GET', projectPath)).project.shots[0].subtitle === first.subtitle, '最终字幕恢复失败')
  await emit({ stage: 'timeline-edit-verified', shots: 5, duration: 15, undo: true, redo: true })
  const sessionPath = `/api/v1/studio/sessions/${window.__PIXOMA_VIDEO_SESSION__}`
  let detail = await request('GET', sessionPath)
  let runId = detail.pending_approvals?.length ? detail.session.latest_run.id : null
  if (!runId) {
  document.querySelector('[aria-label="引用分镜到对话"]').click()
  await wait(() => document.querySelector('[aria-label="输入消息"]')?.textContent.includes('创作上下文'), '分镜上下文没有进入Chat')
  const composer = document.querySelector('[aria-label="输入消息"]')
  composer.focus()
  const selection = window.getSelection()
  const range = document.createRange()
  range.selectNodeContents(composer)
  range.collapse(false)
  selection.removeAllRanges()
  selection.addRange(range)
  document.execCommand('insertText', false, '只修改引用的第一个分镜：将剧本和prompt改成清晨茶园中层叠山峦与轻柔晨雾，镜头缓慢推进，安静写实，无文字。使用已选H3工作流重新生成，完成后采用新视频并保留原视频历史。其他四个分镜完整保持，所有分镜剪辑时长保持3秒。直接完成。')
  const before = await request('GET', sessionPath)
  const previousRun = before.session.latest_run?.id
  await wait(() => !document.querySelector('[aria-label="发送消息"]').disabled, 'Chat发送动作不可用')
  document.querySelector('[aria-label="发送消息"]').click()
  await wait(async () => {
    detail = await request('GET', sessionPath)
    return detail.session.latest_run?.id && detail.session.latest_run.id !== previousRun
  }, 'Chat没有创建真实运行')
  runId = detail.session.latest_run.id
  }
  await emit({ stage: 'chat-generating', run_id: runId })
  await wait(async () => {
    detail = await request('GET', sessionPath)
    const run = detail.session.latest_run
    if (run?.status === 'failed' || run?.status === 'cancelled') throw new Error(`Chat运行失败：${JSON.stringify(run)}`)
    if (detail.pending_approvals?.length) {
      const approval = detail.pending_approvals[0]
      assert(approval.metadata.toolName === 'studio_run_workflow' && approval.metadata.arguments.workflow_id === first.workflowId, '审批涉及其他工作流')
      const approve = button('批准')
      if (approve && !approve.disabled) approve.click()
    }
    return run?.id === runId && run.status === 'succeeded'
  }, 'Chat生成没有完成', 600000)
  const changed = await request('GET', projectPath)
  assert(changed.project.shots[0].media.versionId !== first.media.versionId, 'Chat没有采用重新生成的视频')
  assert(changed.project.shots[0].versions.some(version => version.media.versionId === first.media.versionId), '原视频历史缺失')
  assert(changed.project.shots.slice(1).every((shot, index) => JSON.stringify(shot) === JSON.stringify(original.project.shots[index + 1])), '单分镜Chat修改了其他分镜')
  assert(changed.project.shots.reduce((sum, shot) => sum + shot.duration, 0) === 15, '总时长变化')
  await wait(() => !button('导出成片').disabled, '导出动作不可用')
  button('导出成片').click()
  await wait(async () => {
    const session = await request('GET', sessionPath)
    return session.assets.some(asset => asset.versions.some(version => version.content_origin === 'video_export'))
  }, '成片没有通过编辑器导出', 180000)
  await emit({ stage: 'complete', success: true, session_id: window.__PIXOMA_VIDEO_SESSION__, run_id: runId, asset_version: changed.asset_version, shots: 5, duration: 15, real_video_playback: true, timeline_edit: true, history_preserved: true, chat_generation: true, editor_export: true })
})().catch(error => window.__TAURI_INTERNALS__.invoke('plugin:event|emit', { event: 'studio:video-progress', payload: { stage: 'failed', message: String(error), page_text: document.body.innerText.slice(-2500) } }))
