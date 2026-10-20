import { emit } from '@tauri-apps/api/event'
import { desktopInvoke } from '../../src/lib/api/client'
import { listenStudioRun } from '../../src/lib/desktop-events'
import { StudioWebSocketAgent } from '../../src/lib/agui-websocket-agent'

const request = (method, path, body = null) => desktopInvoke('studio_request', { request: { method, path, body } })
const check = (condition, message) => { if (!condition) throw new Error(message) }
const output = (events) => events.filter((event) => event.type === 'TEXT_MESSAGE_CONTENT').map((event) => event.delta).join('')
const runOptions = () => ({ runId: crypto.randomUUID(), tools: [], context: [], forwardedProps: {} })

async function complete(input) {
  const turn = await request('POST', '/api/v1/studio/messages', { request_id: crypto.randomUUID(), permission_mode: 'auto_approve', ...input })
  const events = []
  await listenStudioRun(turn.run.id, 0, (event) => events.push(event), new AbortController().signal)
  check(events.at(-1)?.outcome?.type === 'success', `原生恢复对话失败：${events.at(-1)?.message ?? events.at(-1)?.type}`)
  return { turn, events }
}

async function create(input) {
  const identity = await request('POST', '/api/v1/studio/auth/login', { ...input.account, remember_me: true })
  await request('POST', '/api/v1/studio/auth/password', { current_password: input.account.password, new_password: crypto.randomUUID() })
  delete input.account
  const configuration = input.model
  delete input.model
  const compressedConfiguration = structuredClone(configuration)
  compressedConfiguration.limits = { context_window_tokens: 12000, max_input_tokens: 3000, max_output_tokens: 1200 }
  compressedConfiguration.capabilities.tools = false
  const compressedModel = await request('POST', '/api/v1/studio-operations/models', compressedConfiguration)
  const project = await request('POST', '/api/v1/studio/projects', { name: '原生重新启动检查' })
  const session = await request('POST', '/api/v1/studio/sessions', { request_id: crypto.randomUUID() })
  await request('PATCH', `/api/v1/studio/sessions/${session.id}/project`, { project_id: project.id })
  const marker = crypto.randomUUID()
  const backup = crypto.randomUUID()
  const material = `审核编号：${marker}\n${'这是一份参考资料。湖边有一条道路，道路旁有树木，远处可以看见山峰。\n'.repeat(160)}\n备用编号：${backup}`
  const asset = await request('POST', '/api/v1/studio/assets/text', {
    session_id: session.id, request_id: crypto.randomUUID(), name: '重新启动资料', content: material,
  })
  const first = await complete({
    session_id: session.id, model_config_id: compressedModel.id, text: '根据所附资料仅回复审核编号。',
    parts: [{ type: 'asset_ref', asset_id: asset.id, asset_version_id: asset.versions[0].id, name: asset.name }],
  })
  check(output(first.events).includes(marker), '重新启动前的摘要没有保留审核编号')
  const overview = await request('GET', `/api/v1/studio/sessions/${session.id}/context`)
  check(overview.compactions > 0, '重新启动前没有保存资料摘要')
  await emit('studio:recovery-progress', { stage: 'summary_saved' })
  const toolModel = await request('POST', '/api/v1/studio-operations/models', configuration)
  const approvalSession = await request('POST', '/api/v1/studio/sessions', { request_id: crypto.randomUUID() })
  const content = crypto.randomUUID()
  const name = `恢复检查-${content}.md`
  const events = []
  const agent = new StudioWebSocketAgent({
    url: '', threadId: approvalSession.id,
    runConfig: { modelConfigId: toolModel.id, locale: 'zh', permissionMode: 'request_approval', selectedSkillIds: [], selectedAssets: [] },
    initialMessages: [{ id: crypto.randomUUID(), role: 'user', content: `调用一次 create_text_asset 创建 Markdown 文档，name 严格为「${name}」，content 严格为「${content}」，不添加其他内容。` }],
    onEvent: (event) => events.push(event),
  })
  await agent.runAgent(runOptions())
  const interrupt = events.at(-1)?.outcome?.interrupts?.[0]
  const run = agent.activeStudioRunId()
  check(events.at(-1)?.outcome?.type === 'interrupt' && interrupt?.id && run, '重新启动前没有进入审批等待')
  check((await request('GET', `/api/v1/studio/runs/${run}`)).status === 'waiting_approval', '重新启动前的审批状态没有保存')
  check((await request('GET', `/api/v1/studio/sessions/${approvalSession.id}/assets`)).length === 0, '批准之前已经创建文档')
  await emit('studio:recovery-result', {
    success: true, phase: 'create', account: identity.id, project: project.id, session: session.id, model: compressedModel.id,
    marker, backup, material, asset: asset.id, assetVersion: asset.versions[0].id, compactions: overview.compactions,
    toolModel: toolModel.id, approvalSession: approvalSession.id, run, interrupt, content, name,
  })
}

async function resume(state) {
  const identity = await request('GET', '/api/v1/studio/auth/me')
  check(identity.id === state.account, '重新启动后没有恢复原账号')
  check((await request('GET', '/api/v1/studio/projects')).some((project) => project.id === state.project), '重新启动后项目没有保留')
  const asset = await request('GET', `/api/v1/studio/assets/${state.asset}`)
  const version = asset.versions.find((version) => version.id === state.assetVersion)
  check(version && await (await fetch(version.content_url)).text() === state.material, '重新启动后原始资料内容不同')
  const overview = await request('GET', `/api/v1/studio/sessions/${state.session}/context`)
  check(overview.compactions === state.compactions, '重新启动后摘要记录没有保留')
  const followup = await complete({ session_id: state.session, model_config_id: state.model, text: '同一份资料里的备用编号是什么？仅回复备用编号。' })
  check(output(followup.events).includes(state.backup), '重新启动后的连续对话没有读取保存的摘要')
  const reused = await request('GET', `/api/v1/studio/sessions/${state.session}/context`)
  check(reused.compactions === state.compactions, '重新启动后重复生成了已有资料摘要')
  await emit('studio:recovery-progress', { stage: 'summary_reused_after_restart' })
  const pending = await request('GET', `/api/v1/studio/sessions/${state.approvalSession}`)
  check(pending.session.latest_run.id === state.run && pending.session.latest_run.status === 'waiting_approval', '重新启动后原审批运行没有保留')
  check(pending.pending_approvals.length === 1 && pending.pending_approvals[0].id === state.interrupt.id, '重新启动后待批准事项不同')
  check(pending.assets.length === 0, '重新启动自动执行了未批准工具')
  const events = []
  const agent = new StudioWebSocketAgent({
    url: '', threadId: state.approvalSession,
    runConfig: { modelConfigId: state.toolModel, locale: 'zh', permissionMode: 'request_approval', selectedSkillIds: [], selectedAssets: [] },
    initialMessages: [], onEvent: (event) => events.push(event),
  })
  await agent.runAgent({ ...runOptions(), resume: [{ interruptId: state.interrupt.id, status: 'resolved', payload: true }] })
  check(events.at(-1)?.outcome?.type === 'success' && agent.activeStudioRunId() === state.run, '重新启动后没有继续原审批运行')
  const assets = await request('GET', `/api/v1/studio/sessions/${state.approvalSession}/assets`)
  check(assets.length === 1 && assets[0].name === state.name, '恢复审批没有创建唯一文档')
  check(await (await fetch(assets[0].versions[0].content_url)).text() === state.content, '恢复审批创建的文档内容不同')
  await request('POST', `/api/v1/studio/approvals/${state.interrupt.id}`, { approved: true })
  check((await request('GET', `/api/v1/studio/sessions/${state.approvalSession}/assets`)).length === 1, '重新启动后的重复批准再次执行了工具')
  await request('POST', '/api/v1/studio/auth/logout')
  await emit('studio:recovery-result', { success: true, phase: 'resume', authentication: true, project: true, original_asset: true, summary_reused: true, original_run: true, approval_once: true })
}

void (async () => {
  const input = window.__PIXOMA_RECOVERY_CHECK__
  delete window.__PIXOMA_RECOVERY_CHECK__
  try {
    if (input.phase === 'create') await create(input)
    else if (input.phase === 'resume') await resume(input.state)
    else throw new Error('原生恢复检查阶段无效')
  } catch (error) {
    await emit('studio:recovery-result', { success: false, phase: input.phase, message: error instanceof Error ? error.message : String(error) })
  }
})()
