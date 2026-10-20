import { emit } from '@tauri-apps/api/event'
import { replaceEqualDeep } from '@tanstack/react-query'
import { desktopInvoke } from '../../src/lib/api/client'
import { listenStudioRun } from '../../src/lib/desktop-events'
import { StudioWebSocketAgent } from '../../src/lib/agui-websocket-agent'

void (async () => {
  try {
    const request = (method, path, body) => desktopInvoke('studio_request', { request: { method, path, body } })
    const check = (condition, message) => { if (!condition) throw new Error(message) }
    const account = window.__PIXOMA_ACCOUNT_CHECK__
    delete window.__PIXOMA_ACCOUNT_CHECK__
    await request('POST', '/api/v1/studio/auth/login', { ...account, remember_me: false })
    const primaryPassword = crypto.randomUUID()
    await request('POST', '/api/v1/studio/auth/password', { current_password: account.password, new_password: primaryPassword })
    await emit('studio:journey-progress', { stage: 'authenticated' })
    const model = await request('POST', '/api/v1/studio-operations/models', window.__PIXOMA_MODEL_CHECK__)
    delete window.__PIXOMA_MODEL_CHECK__
    check(!('api_key' in model), '模型接口暴露凭据')
    await emit('studio:journey-progress', { stage: 'model_saved' })
    const { uploadExpertAvatar, expertAvatarURL, listReferenceSkills, saveExpert, changeAssetPublication, listAssetPublications } = await import('/src/lib/api/agent-assets.ts')
    const image = await fetch('/images/pixoma-ghost.png')
    check(image.ok, '头像检查图片无法读取')
    const imageBytes = new Uint8Array(await image.arrayBuffer())
    const avatar = await uploadExpertAvatar('studio', new File([imageBytes], '头像.png', { type: 'image/png' }), crypto.randomUUID())
    const skillMarker = crypto.randomUUID()
    const skill = await request('POST', '/api/v1/studio/skills', {
      name: '原生文档技能', description: '读取文档编号', prompt: `本技能的识别编号为 ${skillMarker}。用户询问时回复完整编号。`, version: '1.0.0', enabled: true,
    })
    check((await listReferenceSkills('studio')).some((item) => item.id === skill.id), '专家编辑器没有包含个人技能')
    const expert = await saveExpert('studio', {
      schema_version: 2, name: '原生专家检查', description: '使用指定技能和本地模型', model_id: model.id,
      prompt_parts: [{ type: 'skill', skill_id: skill.id, version: '1.0.0' }],
      avatar_asset_id: avatar.id, avatar_version_id: avatar.versions[0].id, examples: [], skill_refs: [], workflow_refs: [], sop: [],
    }, '1.0.0', '')
    const avatarUrl = expertAvatarURL('studio', expert.id, expert.version)
    const avatarResponse = await fetch(avatarUrl)
    check(avatarResponse.ok && avatarResponse.headers.get('content-type') === 'image/png', '原生头像协议没有返回图片')
    const avatarBytes = new Uint8Array(await avatarResponse.arrayBuffer())
    check(avatarBytes.length === imageBytes.length && avatarBytes.every((value, index) => value === imageBytes[index]), '原生头像内容与上传图片不同')
    const publishRequest = crypto.randomUUID()
    const publication = await changeAssetPublication('studio', 'skills', skill.id, 'publish', '1.0.0', undefined, publishRequest)
    check(publication.id === skill.id && publication.published && publication.published_version === '1.0.0', '原生技能发布没有保留本地标识或所选版本')
    await changeAssetPublication('studio', 'skills', skill.id, 'publish', '1.0.0', undefined, publishRequest)
    check((await listAssetPublications('studio', 'skills', skill.id)).length === 1, '原生重复发布增加了发布记录')
    const publishedExpert = await changeAssetPublication('studio', 'experts', expert.id, 'publish', '1.0.0')
    check(publishedExpert.id === expert.id && publishedExpert.model_id === model.id && publishedExpert.published, '原生专家发布修改了本地模型或对象标识')
    check((await listAssetPublications('studio', 'experts', expert.id)).length === 1, '原生专家发布记录没有保存')
    await changeAssetPublication('studio', 'experts', expert.id, 'unpublish')
    await changeAssetPublication('studio', 'skills', skill.id, 'unpublish')
    check((await listAssetPublications('studio', 'experts', expert.id)).length === 2, '原生专家撤销发布没有保存记录')
    await emit('studio:journey-progress', { stage: 'personal_publications' })
    const expertTurn = await request('POST', '/api/v1/studio/messages', {
      request_id: crypto.randomUUID(), expert_id: expert.id, expert_version: '1.0.0', text: '请仅回复所引用技能的识别编号。', permission_mode: 'auto_approve',
    })
    const expertEvents = []
    await listenStudioRun(expertTurn.run.id, 0, (event) => expertEvents.push(event), new AbortController().signal)
    check(expertEvents.at(-1)?.outcome?.type === 'success', '原生专家运行没有完成')
    check(expertEvents.filter((event) => event.type === 'TEXT_MESSAGE_CONTENT').map((event) => event.delta).join('').includes(skillMarker), '原生专家没有读取绑定的技能')
    const expertDetail = await request('GET', `/api/v1/studio/sessions/${expertTurn.session.id}`, null)
    check(expertDetail.expert_capabilities.skills.length === 1 && expertDetail.expert_capabilities.skills[0].version === '1.0.0', '原生会话没有返回绑定的技能版本')
    await emit('studio:journey-progress', { stage: 'expert_avatar_and_skill' })
    const project = await request('POST', '/api/v1/studio/projects', { name: '原生项目检查' })
    const projects = await request('GET', '/api/v1/studio/projects', null)
    check(projects.some((item) => item.id === project.id), '项目未通过真实 IPC 保存')
    const turn = await request('POST', '/api/v1/studio/messages', {
      request_id: crypto.randomUUID(), project_id: project.id,
      text: '请记住对话编号 PX48253，仅回复编号。', locale: 'zh',
      model_config_id: model.id, permission_mode: 'auto_approve',
    })
    const events = []
    await listenStudioRun(turn.run.id, 0, (event) => events.push(event), new AbortController().signal)
    check(events[0]?.type === 'RUN_STARTED', 'Channel 缺少开始事件')
    check(events.at(-1)?.type === 'RUN_FINISHED', 'Channel 缺少结束事件')
    check(!events.some((event) => event.type === 'REASONING_MESSAGE_CONTENT'), '关闭思考后仍然返回思考内容')
    check(events.filter((event) => event.type === 'TEXT_MESSAGE_CONTENT').map((event) => event.delta).join('').includes('PX48253'), 'Channel 未收到真实模型回复')
    const replay = []
    await listenStudioRun(turn.run.id, events[0].sequence, (event) => replay.push(event), new AbortController().signal)
    check(replay.length === events.length - 1 && replay[0].sequence > events[0].sequence, 'Channel 续接序号无效')
    await emit('studio:journey-progress', { stage: 'streaming_replay' })
    const documentContent = 'native-asset-PX48253'
    const documentAsset = await request('POST', '/api/v1/studio/assets/text', {
      session_id: turn.session.id, request_id: crypto.randomUUID(), name: '原生文档', content: documentContent,
    })
    const documentUrl = documentAsset.versions[0].content_url
    const documentResponse = await fetch(documentUrl)
    check(documentResponse.ok && await documentResponse.text() === documentContent, '原生媒体协议未返回文档内容')
    const documentRange = await fetch(documentUrl, { headers: { Range: 'bytes=0-5' } })
    check(documentRange.status === 206 && await documentRange.text() === 'native', '原生媒体协议范围读取无效')
    await emit('studio:journey-progress', { stage: 'media' })
    const continued = []
    const agent = new StudioWebSocketAgent({
      url: '', threadId: turn.session.id,
      runConfig: { modelConfigId: model.id, locale: 'zh', permissionMode: 'auto_approve', selectedSkillIds: [], selectedAssets: [] },
      initialMessages: [{ id: crypto.randomUUID(), role: 'user', content: '刚刚的对话编号是什么？仅回复编号。' }],
      onEvent: (event) => continued.push(event),
    })
    await agent.runAgent({ runId: crypto.randomUUID(), tools: [], context: [], forwardedProps: {} })
    check(continued.at(-1)?.type === 'RUN_FINISHED', 'AG-UI 运行未结束')
    check(continued.filter((event) => event.type === 'TEXT_MESSAGE_CONTENT').map((event) => event.delta).join('').includes('PX48253'), 'AG-UI 未读取历史对话')
    await emit('studio:journey-progress', { stage: 'agui' })
    const contextSource = await request('POST', '/api/v1/studio/sessions', { request_id: crypto.randomUUID() })
    const contextTarget = await request('POST', '/api/v1/studio/sessions', { request_id: crypto.randomUUID() })
    const contextCode = crypto.randomUUID()
    const contextAsset = await request('POST', '/api/v1/studio/assets/text', {
      session_id: contextSource.id, request_id: crypto.randomUUID(), name: '上下文资料', content: `文档编号：${contextCode}`,
    })
    const contextEvents = []
    const contextAgent = new StudioWebSocketAgent({
      url: '', threadId: contextTarget.id,
      runConfig: { modelConfigId: model.id, locale: 'zh', permissionMode: 'auto_approve', selectedSkillIds: [], selectedAssets: [{ assetId: contextAsset.id, assetVersionId: contextAsset.versions[0].id }] },
      initialMessages: [{ id: crypto.randomUUID(), role: 'user', content: '请逐字复制所附文档正文中「文档编号：」之后的完整编号，包含全部字母、数字和连字符。不要更改或添加任何字符。' }],
      onEvent: (event) => contextEvents.push(event),
    })
    await contextAgent.runAgent({ runId: crypto.randomUUID(), tools: [], context: [], forwardedProps: {} })
    check(contextEvents.at(-1)?.outcome?.type === 'success', '所选资产上下文运行未结束')
    const contextOutput = contextEvents.filter((event) => event.type === 'TEXT_MESSAGE_CONTENT').map((event) => event.delta).join('')
    check(contextOutput.includes(contextCode), `AG-UI 未使用所选资产，实际回复：${contextOutput}，资料编号：${contextCode}`)
    const contextOverview = await request('GET', `/api/v1/studio/sessions/${contextTarget.id}/context`, null)
    check(contextOverview.injections === 1 && contextOverview.tokens.input > 0, '原生上下文未保存实际注入和用量')
    const contextRequest = await request('GET', `/api/v1/studio/sessions/${contextTarget.id}/context/current`, null)
    check(contextRequest.parts.some((part) => part.content.includes(contextCode)), '原生上下文未保存资料内容')
    const contextTrajectory = await request('GET', `/api/v1/studio/sessions/${contextTarget.id}/trajectory`, null)
    check(contextTrajectory.total_runs === 1 && contextTrajectory.runs[0].records.some((record) => record.kind === 'model'), '原生运行轨迹未保存模型请求')
    await emit('studio:journey-progress', { stage: 'asset_context' })
    const { id: contextModelId, api_key_masked: contextMasked, has_api_key: contextHasKey, ...originalConfiguration } = structuredClone(model)
    const compressionConfiguration = structuredClone(originalConfiguration)
    compressionConfiguration.limits = { context_window_tokens: 12000, max_input_tokens: 3000, max_output_tokens: 1200 }
    compressionConfiguration.capabilities.tools = false
    await request('PATCH', `/api/v1/studio-operations/models/${contextModelId}`, compressionConfiguration)
    const compressedSession = await request('POST', '/api/v1/studio/sessions', { request_id: crypto.randomUUID() })
    const compressionCode = crypto.randomUUID()
    const retainedCode = crypto.randomUUID()
    const material = `审核编号：${compressionCode}\n${'这是一份参考资料。湖边有一条道路，道路旁有树木，远处可以看见山峰。\n'.repeat(160)}\n备用编号：${retainedCode}`
    const materialAsset = await request('POST', '/api/v1/studio/assets/text', {
      session_id: contextSource.id, request_id: crypto.randomUUID(), name: '原生压缩资料', content: material,
    })
    const compressed = await request('POST', '/api/v1/studio/messages', {
      request_id: crypto.randomUUID(), session_id: compressedSession.id, text: '根据所附资料仅回复审核编号。',
      model_config_id: model.id, permission_mode: 'auto_approve',
      parts: [{ type: 'asset_ref', asset_id: materialAsset.id, asset_version_id: materialAsset.versions[0].id, name: materialAsset.name }],
    })
    const compressedEvents = []
    await listenStudioRun(compressed.run.id, 0, (event) => compressedEvents.push(event), new AbortController().signal)
    check(compressedEvents.at(-1)?.outcome?.type === 'success', '原生资料压缩运行失败')
    check(compressedEvents.filter((event) => event.type === 'TEXT_MESSAGE_CONTENT').map((event) => event.delta).join('').includes(compressionCode), '原生压缩资料没有保留审核编号')
    const compressionOverview = await request('GET', `/api/v1/studio/sessions/${compressedSession.id}/context`, null)
    check(compressionOverview.compactions > 0, '原生长资料没有生成摘要')
    const compressionFollowup = await request('POST', '/api/v1/studio/messages', {
      request_id: crypto.randomUUID(), session_id: compressedSession.id, text: '同一份资料里的备用编号是什么？仅回复备用编号。',
      model_config_id: model.id, permission_mode: 'auto_approve',
    })
    const compressionFollowupEvents = []
    await listenStudioRun(compressionFollowup.run.id, 0, (event) => compressionFollowupEvents.push(event), new AbortController().signal)
    check(compressionFollowupEvents.at(-1)?.outcome?.type === 'success', '原生摘要连续对话失败')
    check(compressionFollowupEvents.filter((event) => event.type === 'TEXT_MESSAGE_CONTENT').map((event) => event.delta).join('').includes(retainedCode), '原生摘要连续对话没有保留备用编号')
    const reusedOverview = await request('GET', `/api/v1/studio/sessions/${compressedSession.id}/context`, null)
    check(reusedOverview.compactions === compressionOverview.compactions, '原生连续对话重复生成已有资料摘要')
    const compressionRequests = await request('GET', `/api/v1/studio/sessions/${compressedSession.id}/context/requests`, null)
    check(compressionRequests.requests.some((item) => item.purpose === 'compaction'), '原生上下文没有保存真实摘要请求')
    check(compressionRequests.requests.every((item) => item.projected_tokens <= 3000 && item.input_tokens <= 3000), '原生压缩请求超过输入限制')
    check(await (await fetch(materialAsset.versions[0].content_url)).text() === material, '原生摘要处理修改了原始文档')
    await request('PATCH', `/api/v1/studio-operations/models/${contextModelId}`, originalConfiguration)
    await emit('studio:journey-progress', { stage: 'context_compaction' })
    const detail = await request('GET', `/api/v1/studio/sessions/${turn.session.id}`, null)
    check(detail.messages.filter((message) => message.role === 'assistant').length === 2, '真实回复未保存')
    const approvalEvents = []
    const approvalAgent = new StudioWebSocketAgent({
      url: '', threadId: turn.session.id,
      runConfig: { modelConfigId: model.id, locale: 'zh', permissionMode: 'request_approval', selectedSkillIds: [], selectedAssets: [] },
      initialMessages: [{ id: crypto.randomUUID(), role: 'user', content: '请调用 create_text_asset 工具创建一个名为 PX原生审批 的 Markdown 文档，正文严格为 PX73916。完成后仅回复完成。' }],
      onEvent: (event) => approvalEvents.push(event),
    })
    const runOptions = () => ({ runId: crypto.randomUUID(), tools: [], context: [], forwardedProps: {} })
    await approvalAgent.runAgent(runOptions())
    const interrupt = approvalEvents.at(-1)?.outcome?.interrupts?.[0]
    check(approvalEvents.at(-1)?.outcome?.type === 'interrupt' && interrupt?.id, 'AG-UI 未收到审批暂停事件')
    const pendingRunId = approvalAgent.activeStudioRunId()
    check((await request('GET', `/api/v1/studio/runs/${pendingRunId}`, null)).status === 'waiting_approval', '审批暂停未保存')
    const beforeApprovalAssets = await request('GET', `/api/v1/studio/sessions/${turn.session.id}/assets`, null)
    check(!beforeApprovalAssets.some((asset) => asset.name === 'PX原生审批.md'), '批准之前已创建文档')
    await approvalAgent.runAgent({ ...runOptions(), resume: [{ interruptId: interrupt.id, status: 'resolved', payload: true }] })
    check(approvalEvents.at(-1)?.outcome?.type === 'success', 'AG-UI 批准后未结束运行')
    check(approvalAgent.activeStudioRunId() === pendingRunId, '审批恢复创建了其他运行')
    const approvedAssets = await request('GET', `/api/v1/studio/sessions/${turn.session.id}/assets`, null)
    check(approvedAssets.filter((asset) => asset.name === 'PX原生审批.md').length === 1, '批准后未创建唯一文档')
    await request('POST', `/api/v1/studio/approvals/${interrupt.id}`, { approved: true })
    check((await request('GET', `/api/v1/studio/sessions/${turn.session.id}/assets`, null)).filter((asset) => asset.name === 'PX原生审批.md').length === 1, '重复答复重复执行了工具')
    await emit('studio:journey-progress', { stage: 'tool_approval' })
    const connector = await request('POST', '/api/v1/studio-operations/connectors', window.__PIXOMA_MCP_CHECK__)
    delete window.__PIXOMA_MCP_CHECK__
    const discovered = await request('POST', `/api/v1/studio-operations/connectors/${connector.id}/probe`, null)
    check(discovered.tools.some((tool) => tool.name === 'list_workflows'), '真实 MCP 未提供工作流列表工具')
    const mcpEvents = []
    const mcpAgent = new StudioWebSocketAgent({
      url: '', threadId: turn.session.id,
      runConfig: { modelConfigId: model.id, locale: 'zh', permissionMode: 'full_access', selectedSkillIds: [], selectedAssets: [] },
      initialMessages: [{ id: crypto.randomUUID(), role: 'user', content: '请调用真实 MCP 的 list_workflows 工具查询官方工作流列表，然后简要说明结果。仅调用一次 list_workflows，不要创建计算任务。' }],
      onEvent: (event) => mcpEvents.push(event),
    })
    await mcpAgent.runAgent(runOptions())
    const mcpInterrupt = mcpEvents.at(-1)?.outcome?.interrupts?.[0]
    const aliasBytes = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(`${connector.id}:list_workflows`))
    const expectedTool = `mcp_${Array.from(new Uint8Array(aliasBytes), (value) => value.toString(16).padStart(2, '0')).join('').slice(0, 32)}`
    check(mcpInterrupt?.metadata?.toolAlias === expectedTool && mcpInterrupt.metadata.toolName === 'list_workflows', 'MCP 调用未按连接器策略请求批准')
    await mcpAgent.runAgent({ ...runOptions(), resume: [{ interruptId: mcpInterrupt.id, status: 'resolved', payload: true }] })
    check(mcpEvents.at(-1)?.outcome?.type === 'success', '真实 MCP 调用未结束')
    check(mcpEvents.some((event) => event.type === 'TOOL_CALL_RESULT' && event.isError === false && JSON.parse(event.content).content.length > 0), '真实 MCP 调用未返回服务内容')
    await emit('studio:journey-progress', { stage: 'mcp_call' })
    const workflowCatalog = await request('GET', '/api/v1/studio/workflow-market', null)
    check(workflowCatalog.length > 0, '真实平台没有可用工作流')
    const workflowDetail = await request('GET', `/api/v1/studio/workflow-market/${workflowCatalog[0].id}`, null)
    const workflowSession = await request('POST', '/api/v1/studio/sessions', { request_id: crypto.randomUUID() })
    const workflowEvents = []
    const workflowAgent = new StudioWebSocketAgent({
      url: '', threadId: workflowSession.id,
      runConfig: {
        modelConfigId: model.id, locale: 'zh', permissionMode: 'full_access', selectedSkillIds: [], selectedAssets: [],
        messageParts: [{ type: 'workflow_ref', workflow_id: workflowDetail.id, version_id: workflowDetail.version_id, name: workflowDetail.name }],
      },
      initialMessages: [{ id: crypto.randomUUID(), role: 'user', content: `请调用真实平台工作流 ${workflowDetail.id} 的 run_workflow 工具获取输入表单，参数严格使用 case_id=${workflowDetail.id}、inputs={}。不要自行填写任何输入，等待我填写表单。` }],
      onEvent: (event) => workflowEvents.push(event),
    })
    await workflowAgent.runAgent(runOptions())
    const workflowApproval = workflowEvents.at(-1)?.outcome?.interrupts?.[0]
    check(workflowApproval?.metadata?.toolName === 'run_workflow', '工作流调用没有经过连接器审批')
    await workflowAgent.runAgent({ ...runOptions(), resume: [{ interruptId: workflowApproval.id, status: 'resolved', payload: true }] })
    const workflowInput = workflowEvents.at(-1)?.outcome?.interrupts?.[0]
    check(workflowInput?.reason === 'workflow_input' && workflowInput.metadata.workflow.id === workflowDetail.id, '工作流没有返回输入卡片')
    const waitingWorkflow = await request('GET', `/api/v1/studio/sessions/${workflowSession.id}`, null)
    check(waitingWorkflow.session.latest_run.status === 'waiting_clarification' && waitingWorkflow.pending_clarifications[0]?.id === workflowInput.id, '工作流输入等待状态没有保存')
    check(replaceEqualDeep(workflowDetail.input_fields, workflowInput.metadata.workflow.input_fields) === workflowDetail.input_fields, '工作流输入字段与真实目录不一致')
    await workflowAgent.runAgent({ ...runOptions(), resume: [{ interruptId: workflowInput.id, status: 'cancelled' }] })
    check(workflowEvents.at(-1)?.outcome?.type === 'cancelled', '跳过工作流输入没有结束本地运行')
    const skippedWorkflow = await request('GET', `/api/v1/studio/sessions/${workflowSession.id}`, null)
    check(skippedWorkflow.pending_clarifications.length === 0 && skippedWorkflow.workflow_executions.length === 0, '跳过输入后仍有等待事项或远程执行记录')
    check(skippedWorkflow.transcript.messages.some((message) => message.clarification?.id === workflowInput.id && message.clarification.status === 'skipped'), '跳过工作流输入没有保留会话记录')
    await emit('studio:journey-progress', { stage: 'workflow_input' })
    const cancellable = await request('POST', '/api/v1/studio/messages', {
      request_id: crypto.randomUUID(), session_id: turn.session.id,
      text: '请详细撰写一篇 3000 字的科幻故事。', locale: 'zh',
      model_config_id: model.id, permission_mode: 'auto_approve',
    })
    const detached = new AbortController()
    let resolveCancellableText
    const cancellableText = new Promise((resolve) => { resolveCancellableText = resolve })
    const detachedSubscription = listenStudioRun(cancellable.run.id, 0, (event) => {
      if (event.type === 'TEXT_MESSAGE_CONTENT' && event.delta) resolveCancellableText()
    }, detached.signal)
    await Promise.race([cancellableText, detachedSubscription.then(() => { throw new Error('取消检查的运行在收到正文之前结束') })])
    detached.abort()
    await detachedSubscription
    await request('POST', `/api/v1/studio/runs/${cancellable.run.id}/cancel`, null)
    const cancelled = []
    await listenStudioRun(cancellable.run.id, 0, (event) => cancelled.push(event), new AbortController().signal)
    check(cancelled.at(-1)?.outcome?.type === 'cancelled', '取消运行未保存结束事件')
    check((await request('GET', `/api/v1/studio/runs/${cancellable.run.id}`, null)).status === 'cancelled', '取消运行未保存结束状态')
    const afterCancellation = await request('POST', '/api/v1/studio/messages', {
      request_id: crypto.randomUUID(), session_id: turn.session.id, text: '最初记住的对话编号是什么？仅回复编号。',
      model_config_id: model.id, permission_mode: 'auto_approve',
    })
    const afterCancellationEvents = []
    await listenStudioRun(afterCancellation.run.id, 0, (event) => afterCancellationEvents.push(event), new AbortController().signal)
    check(afterCancellationEvents.at(-1)?.outcome?.type === 'success', '原生取消后的连续对话失败')
    check(afterCancellationEvents.filter((event) => event.type === 'TEXT_MESSAGE_CONTENT').map((event) => event.delta).join('').includes('PX48253'), '原生取消后丢失原有历史对话')
    await emit('studio:journey-progress', { stage: 'cancellation' })
    const { id, api_key_masked, has_api_key, ...nonStreamingConfiguration } = model
    nonStreamingConfiguration.capabilities.streaming = false
    nonStreamingConfiguration.limits.max_output_tokens = 12000
    nonStreamingConfiguration.thinking = { enabled: false }
    await request('PATCH', `/api/v1/studio-operations/models/${id}`, nonStreamingConfiguration)
    const largeContent = 'PX73916 原生文档完整内容\n'.repeat(1024)
    const largeAsset = await request('POST', '/api/v1/studio/assets/text', {
      session_id: turn.session.id, request_id: crypto.randomUUID(), name: '较大文档检查', content: largeContent,
    })
    const nonStreaming = await request('POST', '/api/v1/studio/messages', {
      request_id: crypto.randomUUID(), project_id: project.id,
      text: `请仅调用一次 read_asset 工具读取 asset_id=${largeAsset.id}、asset_version_id=${largeAsset.versions[0].id} 的文档。读取成功后仅回复收到，无需复制文档内容。`, locale: 'zh',
      model_config_id: model.id, permission_mode: 'auto_approve',
    })
    const nonStreamingEvents = []
    await listenStudioRun(nonStreaming.run.id, 0, (event) => nonStreamingEvents.push(event), new AbortController().signal)
    check(nonStreamingEvents.at(-1)?.type === 'RUN_FINISHED', '非流式运行缺少结束事件')
    const nonStreamingText = nonStreamingEvents.filter((event) => event.type === 'TEXT_MESSAGE_CONTENT').map((event) => event.delta).join('')
    check(nonStreamingText.length > 0, '非流式模型未返回正文')
    const largeToolResult = nonStreamingEvents.find((event) => event.type === 'TOOL_CALL_RESULT' && JSON.parse(event.content).asset_id === largeAsset.id)
    check(largeToolResult?.content.length > 8192 && JSON.parse(largeToolResult.content).content === largeContent, '较大 Channel 消息与文档内容不一致')
    const savedDetail = await request('GET', `/api/v1/studio/sessions/${nonStreaming.session.id}`, null)
    const savedResponse = savedDetail.messages.filter((message) => message.run_id === nonStreaming.run.id && message.role === 'assistant')
    check(savedResponse.flatMap((message) => message.content.filter((part) => part.type === 'text').map((part) => part.text)).join('') === nonStreamingText, '非流式回复与保存内容不一致')
    check(savedDetail.messages.some((message) => message.role === 'tool' && message.content.some((part) => part.text === largeToolResult.content)), '较大工具结果未完整保存')
    await emit('studio:journey-progress', { stage: 'non_streaming' })
    nonStreamingConfiguration.limits.max_output_tokens = 128
    nonStreamingConfiguration.capabilities.tools = false
    await request('PATCH', `/api/v1/studio-operations/models/${id}`, nonStreamingConfiguration)
    const limited = await request('POST', '/api/v1/studio/messages', {
      request_id: crypto.randomUUID(), session_id: turn.session.id,
      text: '请输出包含数字 1 到 500 的完整 JSON 数组，仅回复这个数组，不要解释。', locale: 'zh',
      model_config_id: model.id, permission_mode: 'auto_approve',
    })
    const limitedEvents = []
    await listenStudioRun(limited.run.id, 0, (event) => limitedEvents.push(event), new AbortController().signal)
    check(limitedEvents.at(-1)?.type === 'RUN_ERROR', '超过输出限制的回复未标记运行错误')
    const limitedRun = await request('GET', `/api/v1/studio/runs/${limited.run.id}`, null)
    check(limitedRun.status === 'failed', '超过输出限制的运行未保存错误状态')
    check(limitedRun.error_message.includes('输出限制'), '超过输出限制的运行缺少对应错误说明')
    await emit('studio:journey-progress', { stage: 'output_limit' })
    nonStreamingConfiguration.limits.max_output_tokens = 12000
    await request('PATCH', `/api/v1/studio-operations/models/${id}`, nonStreamingConfiguration)
    const afterLimit = await request('POST', '/api/v1/studio/messages', {
      request_id: crypto.randomUUID(), session_id: turn.session.id, text: '最初记住的对话编号是什么？仅回复编号。',
      model_config_id: model.id, permission_mode: 'auto_approve',
    })
    const afterLimitEvents = []
    await listenStudioRun(afterLimit.run.id, 0, (event) => afterLimitEvents.push(event), new AbortController().signal)
    check(afterLimitEvents.at(-1)?.outcome?.type === 'success', '原生输出限制后的连续对话失败')
    check(afterLimitEvents.filter((event) => event.type === 'TEXT_MESSAGE_CONTENT').map((event) => event.delta).join('').includes('PX48253'), '原生输出限制后丢失原有历史对话')
    await emit('studio:journey-progress', { stage: 'continue_after_output_limit' })
    const beforeSwitch = await request('POST', '/api/v1/studio/messages', {
      request_id: crypto.randomUUID(), session_id: turn.session.id,
      text: '请输出包含数字 1 到 2500 的完整 JSON 数组，仅回复这个数组，不要解释。', locale: 'zh',
      model_config_id: model.id, permission_mode: 'auto_approve',
    })
    const other = window.__PIXOMA_OTHER_CHECK__
    delete window.__PIXOMA_OTHER_CHECK__
    await request('POST', '/api/v1/studio/auth/login', { ...other, remember_me: false })
    await request('POST', '/api/v1/studio/auth/password', { current_password: other.password, new_password: crypto.randomUUID() })
    const otherProjects = await request('GET', '/api/v1/studio/projects', null)
    check(otherProjects.length === 0, '其他账号可以读取原账号项目')
    const otherMedia = await Promise.allSettled([fetch(documentUrl)])
    check(otherMedia[0].status === 'rejected' || otherMedia[0].value.status === 404, '其他账号可以读取原账号媒体内容')
    const otherAvatar = await Promise.allSettled([fetch(avatarUrl)])
    check(otherAvatar[0].status === 'rejected' || otherAvatar[0].value.status === 404, '其他账号可以读取原账号专家头像')
    const inaccessible = await Promise.allSettled([
      request('GET', `/api/v1/studio/runs/${beforeSwitch.run.id}`, null),
      request('GET', `/api/v1/studio/sessions/${turn.session.id}`, null),
      request('GET', `/api/v1/studio/assets/${documentAsset.id}`, null),
    ])
    check(inaccessible.every((result) => result.status === 'rejected' && result.reason.status === 404), '其他账号可以读取原账号运行或会话')
    await request('POST', '/api/v1/studio/auth/login', { username: account.username, password: primaryPassword, remember_me: false })
    const switchEvents = []
    await listenStudioRun(beforeSwitch.run.id, 0, (event) => switchEvents.push(event), new AbortController().signal)
    check(switchEvents.at(-1)?.outcome?.type === 'cancelled', '切换账号未保存取消事件')
    const switchedRun = await request('GET', `/api/v1/studio/runs/${beforeSwitch.run.id}`, null)
    check(switchedRun.status === 'cancelled', '切换账号未取消原账号运行')
    await emit('studio:journey-progress', { stage: 'account_switch' })
    const beforeClear = await request('POST', '/api/v1/studio/messages', {
      request_id: crypto.randomUUID(), session_id: turn.session.id,
      text: '请详细撰写一篇 3000 字的科幻故事。', locale: 'zh',
      model_config_id: model.id, permission_mode: 'auto_approve',
    })
    let resolveStarted
    const startedBeforeClear = new Promise((resolve) => { resolveStarted = resolve })
    const clearingSubscription = listenStudioRun(beforeClear.run.id, 0, (event) => {
      if (event.type === 'RUN_STARTED') resolveStarted()
    }, new AbortController().signal)
    await Promise.race([startedBeforeClear, clearingSubscription.then(() => { throw new Error('清空检查的运行提前结束') })])
    await request('DELETE', '/api/v1/studio/sessions', { confirmation: '确认清空' })
    await clearingSubscription
    check((await request('GET', '/api/v1/studio/sessions', null)).length === 0, '清空后仍有会话')
    check((await request('GET', '/api/v1/studio/projects', null)).some((item) => item.id === project.id), '清空会话删除了项目')
    const retainedAsset = await request('GET', `/api/v1/studio/assets/${documentAsset.id}`, null)
    check(retainedAsset.versions[0].id === documentAsset.versions[0].id, '清空会话删除了资产版本')
    check(retainedAsset.usages.length === 1 && retainedAsset.usages[0].session_available === false, '清空会话未保留资产使用记录')
    const retainedContent = await fetch(documentUrl)
    check(retainedContent.ok && await retainedContent.text() === documentContent, '清空会话删除了资产文件')
    const deletedRun = await Promise.allSettled([request('GET', `/api/v1/studio/runs/${beforeClear.run.id}`, null)])
    check(deletedRun[0].status === 'rejected' && deletedRun[0].reason.status === 404, '清空会话保留了运行记录')
    await emit('studio:journey-progress', { stage: 'clear_sessions' })
    await request('POST', '/api/v1/studio/auth/logout', null)
    await emit('studio:journey-result', { success: true, project: true, streaming: true, replay: true, media: true, range: true, agui: true, persisted_history: true, asset_context: true, context_compaction: true, personal_publications: true, expert_avatar: true, expert_skill_binding: true, tool_approval: true, mcp_call: true, workflow_input: true, detach: true, cancel: true, continue_after_cancellation: true, non_streaming: true, output_limit: true, continue_after_output_limit: true, account_switch: true, clear_sessions: true, logout: true })
  } catch (error) {
    await emit('studio:journey-result', { success: false, message: error instanceof Error ? error.message : String(error) })
  }
})().catch(async (error) => {
  await window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {
    event: 'studio:journey-result',
    payload: { success: false, location: location.href, message: `初始化联调失败：${error instanceof Error ? error.message : String(error)}` },
  })
})
