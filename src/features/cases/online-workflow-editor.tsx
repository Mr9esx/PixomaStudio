import { useEffect, useRef, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useBlocker } from '@tanstack/react-router'
import { createWorkflow, updateWorkflow, type OnlineWorkflowDefinition } from '@/lib/api/workflows'
import { listGenerationConnections, listGenerationTemplates, saveGenerationConnection, type GenerationProvider } from '@/lib/api/generation'
import { Alert, AlertTitle } from '@/components/ui/alert'
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from '@/components/ui/alert-dialog'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Field, FieldGroup, FieldLabel } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { CodeEditor } from '@/components/code-editor'
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Spinner } from '@/components/ui/spinner'
import type { WorkflowEditorProps } from './workflow-editor'
import { workflowDefaultValues, WorkflowParameterFields } from './workflow-parameter-fields'
import { parseOnlineDefinition, prepareOnlineDefinition } from './lib/online-definition'
import { BasicsSection, type BasicsSlice } from './sections/basics'
import { uploadStudioAsset, type StudioAsset } from '@/lib/api/studio'

export type OnlineWorkflowEditorState = {
  kind: 'online'
  workflowId?: string
  revision?: number
  name: string
  enabled: boolean
  definition?: OnlineWorkflowDefinition
  templateId: string
  exposed: string[]
  baseline: string
  rawDefinition?: string
  metadata?: Omit<BasicsSlice, 'name' | 'enabled'>
}

function signature(state: OnlineWorkflowEditorState) {
  return JSON.stringify({ ...state, baseline: undefined })
}

export function OnlineWorkflowEditor(props: WorkflowEditorProps) {
  const client = useQueryClient()
  const [state, setState] = useState<OnlineWorkflowEditorState>(() => {
    if (props.state?.kind === 'online' && props.state.workflowId === props.initial?.id) return props.state
    const definition = props.initial?.draft.type === 'online' ? structuredClone(props.initial.draft) : undefined
    const initial: OnlineWorkflowEditorState = { kind: 'online', workflowId: props.initial?.id, revision: props.initial?.revision,
      name: props.initial?.name ?? '', enabled: props.initial?.enabled ?? true, definition, templateId: '', exposed: definition?.inputs.map(field => field.key) ?? [], baseline: '', metadata: props.initial ? { category: props.initial.category, tags: props.initial.tags, description: definition?.description, usage: definition?.usage, preview: definition?.preview, examples: definition?.examples } : undefined }
    initial.baseline = signature(initial)
    return initial
  })
  const [apiKey, setApiKey] = useState('')
  const [connectionName, setConnectionName] = useState('火山方舟')
  const [connectionProvider, setConnectionProvider] = useState<GenerationProvider>(state.definition?.execution.provider ?? 'seedance')
  const [baseUrl, setBaseUrl] = useState('')
  const leaving = useRef(false)
  const [validation, setValidation] = useState('')
  const [uploadedAssets, setUploadedAssets] = useState<StudioAsset[]>([])
  const upload = useMutation({ mutationFn: async ({ key, file }: { key: string; file: File }) => ({ key, asset: await uploadStudioAsset(file) }), onSuccess: ({ key, asset }) => {
    const version = asset.versions.find(item => item.version === asset.current_version)
    if (!version) throw new Error('上传文件的资产版本缺失')
    setUploadedAssets(current => [...current, asset])
    setState(current => {
      if (!current.definition) throw new Error('工作流定义缺失')
      const multiple = (current.definition.input_schema.properties as Record<string, { type?: string }>)[key]?.type === 'array'
      const previous = current.definition.execution.defaults[key]
      const reference = { asset_id: asset.id, asset_version_id: version.id }
      return { ...current, definition: { ...current.definition, execution: { ...current.definition.execution, defaults: { ...current.definition.execution.defaults, [key]: multiple ? [...(Array.isArray(previous) ? previous : []), reference] : reference } } } }
    })
  } })
  const templates = useQuery({ queryKey: ['studio', 'generation-templates'], queryFn: listGenerationTemplates })
  const connections = useQuery({ queryKey: ['studio', 'generation-connections'], queryFn: listGenerationConnections })
  const storedDefinition = state.definition
  const matchingTemplate = templates.data?.find(template => template.definition.execution.provider === storedDefinition?.execution.provider && template.definition.execution.capability === storedDefinition.execution.capability)?.definition
  const templateDefinition = storedDefinition && matchingTemplate ? { ...storedDefinition, inputs: matchingTemplate.inputs,
    input_schema: { ...matchingTemplate.input_schema, properties: { ...(matchingTemplate.input_schema.properties as object), ...(storedDefinition.input_schema.properties as object) } } } : storedDefinition
  const metadata = state.metadata ?? { category: templateDefinition?.execution.operation === 'image' ? 'image' : 'video', tags: [], description: templateDefinition?.description, usage: templateDefinition?.usage, preview: templateDefinition?.preview, examples: templateDefinition?.examples }
  const { category, tags, ...information } = metadata
  const definition = templateDefinition ? { ...templateDefinition, ...information } : undefined
  const dirty = signature(state) !== state.baseline || Boolean(apiKey)
  const blocker = useBlocker({ shouldBlockFn: () => !leaving.current && dirty, enableBeforeUnload: () => !leaving.current && dirty, withResolver: true })
  useEffect(() => { props.onStateChange?.(state) }, [state, props.onStateChange])
  const saveConnection = useMutation({
    mutationFn: () => saveGenerationConnection({ name: connectionName.trim(), provider: connectionProvider, ...(connectionProvider === 'http_json' ? { base_url: baseUrl.trim() } : {}), api_key: apiKey }),
    onSuccess: connection => {
      setApiKey('')
      setState(current => current.definition ? { ...current, definition: { ...current.definition, execution: { ...current.definition.execution, connection_id: connection.id } } } : current)
      void client.invalidateQueries({ queryKey: ['studio', 'generation-connections'] })
    },
  })
  const save = useMutation({ mutationFn: async () => {
    if (!definition) throw new Error('选择模型模板')
    if (state.rawDefinition !== undefined) throw new Error('应用 JSON 定义后保存工作流')
    if (!state.name.trim()) throw new Error('填写工作流名称')
    if (!definition.execution.connection_id) throw new Error('选择服务连接')
    const draft = prepareOnlineDefinition({ ...definition, execution: { ...definition.execution, input_mapping: { ...matchingTemplate?.execution.input_mapping, ...definition.execution.input_mapping } } }, state.exposed)
    const input = { name: state.name.trim(), category, tags: tags ?? [], enabled: state.enabled, draft }
    if (state.workflowId) {
      if (state.revision === undefined) throw new Error('工作流修订缺失')
      return updateWorkflow(state.workflowId, { ...input, expected_revision: state.revision })
    }
    return createWorkflow(input)
  }, onSuccess: workflow => {
    leaving.current = true
    client.setQueryData(['studio', 'workflow', workflow.id], workflow)
    void client.invalidateQueries({ queryKey: ['studio', 'workflows'] })
    props.onSaved(workflow)
  } })
  const busy = save.isPending || saveConnection.isPending || upload.isPending
  function applyDefinition() {
    try {
      const next = parseOnlineDefinition(state.rawDefinition!)
      setState(current => ({ ...current, definition: next, rawDefinition: undefined, exposed: next.inputs.map(field => field.key), metadata: {
        category: current.metadata?.category ?? (next.execution.operation === 'image' ? 'image' : 'video'), tags: current.metadata?.tags ?? [],
        description: next.description, usage: next.usage, preview: next.preview, examples: next.examples,
      } }))
      setValidation('')
    } catch (cause) { setValidation(String(cause)) }
  }
  function chooseTemplate(id: string) {
    if (id === 'custom-http') { const base = definition ?? templates.data?.[0]?.definition; setState(current => ({ ...current, templateId: id, rawDefinition: base ? JSON.stringify({ ...base, execution: { ...base.execution, provider: 'http_json', connection_id: '' } }, null, 2) : '' })); setConnectionProvider('http_json'); return }
    const template = templates.data?.find(item => item.id === id)
    if (!template) throw new Error('模型模板不存在')
    const next = structuredClone(template.definition)
    next.execution.connection_id = connections.data?.find(connection => connection.provider === next.execution.provider)?.id ?? ''
    next.execution.defaults = { ...workflowDefaultValues(next), ...next.execution.defaults }
    setState(current => ({ ...current, templateId: id, name: current.name || template.name, definition: next, rawDefinition: undefined, exposed: next.inputs.map(field => field.key) }))
    setConnectionProvider(next.execution.provider)
    setConnectionName(next.execution.provider === 'minimax' ? 'MiniMax' : '火山方舟')
    setValidation('')
  }
  return <form id='workflow-editor' className='flex min-h-0 flex-1 flex-col gap-3' onSubmit={event => { event.preventDefault(); setValidation(''); save.mutate() }}>
    <div className='flex min-h-0 flex-1 flex-col gap-6 overflow-y-auto min-[1200px]:flex-row min-[1200px]:overflow-hidden'>
    <div className='relative flex min-w-0 flex-1 flex-col gap-6 min-[1200px]:min-h-0 min-[1200px]:overflow-y-auto'>
    <FieldGroup>
      <Field><FieldLabel htmlFor='online-workflow-template'>模型模板</FieldLabel><Select value={state.templateId || undefined} onValueChange={chooseTemplate} disabled={busy || templates.isPending}><SelectTrigger id='online-workflow-template'><SelectValue placeholder={definition ? `${definition.execution.model} · ${definition.execution.capability}` : '选择模型模板'} /></SelectTrigger><SelectContent><SelectGroup>{templates.data?.map(template => <SelectItem key={template.id} value={template.id}>{template.name}</SelectItem>)}<SelectItem value='custom-http'>自定义 HTTP JSON 模板</SelectItem></SelectGroup></SelectContent></Select></Field>
      {definition ? <><Field><FieldLabel htmlFor='online-workflow-model'>Model ID / Endpoint ID</FieldLabel><Input id='online-workflow-model' value={definition.execution.model} disabled={busy} onChange={event => setState(current => ({ ...current, definition: { ...definition, execution: { ...definition.execution, model: event.target.value } } }))} /></Field><Field><FieldLabel htmlFor='online-workflow-connection'>服务连接</FieldLabel><Select value={definition.execution.connection_id || undefined} disabled={busy || connections.isPending} onValueChange={connection_id => setState(current => ({ ...current, definition: { ...definition, execution: { ...definition.execution, connection_id } } }))}><SelectTrigger id='online-workflow-connection'><SelectValue placeholder='选择服务连接' /></SelectTrigger><SelectContent><SelectGroup>{connections.data?.filter(connection => connection.provider === definition.execution.provider).map(connection => <SelectItem key={connection.id} value={connection.id}>{connection.name}{connection.has_api_key ? ' · 已配置密钥' : ''}</SelectItem>)}</SelectGroup></SelectContent></Select></Field></> : null}
    </FieldGroup>
    <details className='rounded-lg border bg-muted p-4' open={state.templateId === 'custom-http'}><summary className='cursor-pointer text-sm font-medium'>编辑 JSON 定义</summary><Field className='mt-4'><FieldLabel htmlFor='online-workflow-json'>工作流定义</FieldLabel><CodeEditor id='online-workflow-json' minHeight={256} maxHeight={400} readOnly={busy} value={state.rawDefinition ?? (definition ? JSON.stringify(definition, null, 2) : '')} onChange={value => setState(current => ({ ...current, rawDefinition: value }))} /></Field><Button type='button' size='sm' variant='outline' className='mt-3' disabled={busy || state.rawDefinition === undefined} onClick={applyDefinition}>应用 JSON 定义</Button></details>
    <details className='rounded-lg border bg-muted p-4' open={!connections.data?.length}>
      <summary className='cursor-pointer text-sm font-medium'>添加服务连接</summary>
      <FieldGroup className='mt-4'>
        <Field><FieldLabel htmlFor='generation-provider'>服务类型</FieldLabel><Select value={connectionProvider} disabled={busy} onValueChange={value => setConnectionProvider(value as GenerationProvider)}><SelectTrigger id='generation-provider'><SelectValue /></SelectTrigger><SelectContent><SelectGroup><SelectItem value='seedance'>火山方舟 · Seedance</SelectItem><SelectItem value='seedream'>火山方舟 · Seedream</SelectItem><SelectItem value='minimax'>MiniMax</SelectItem><SelectItem value='http_json'>HTTP JSON</SelectItem></SelectGroup></SelectContent></Select></Field>
        <Field><FieldLabel htmlFor='generation-connection-name'>连接名称</FieldLabel><Input id='generation-connection-name' value={connectionName} disabled={busy} onChange={event => setConnectionName(event.target.value)} /></Field>
        {connectionProvider === 'http_json' ? <Field><FieldLabel htmlFor='generation-base-url'>服务地址</FieldLabel><Input id='generation-base-url' type='url' placeholder='https://api.example.com' value={baseUrl} disabled={busy} onChange={event => setBaseUrl(event.target.value)} /></Field> : null}
        <Field><FieldLabel htmlFor='generation-api-key'>API Key</FieldLabel><Input id='generation-api-key' type='password' autoComplete='new-password' value={apiKey} disabled={busy} onChange={event => setApiKey(event.target.value)} /></Field>
      </FieldGroup>
      <Button className='mt-4' type='button' variant='outline' size='sm' disabled={busy || (connectionProvider !== 'http_json' && !apiKey.trim()) || (connectionProvider === 'http_json' && !baseUrl.trim()) || !connectionName.trim()} onClick={() => saveConnection.mutate()}>{saveConnection.isPending ? <Spinner /> : null}保存连接</Button>
    </details>
    {definition ? <section className='flex flex-col gap-4' aria-label='工作流默认参数'><h3 className='text-sm font-semibold'>默认参数与开放输入</h3>{definition.inputs.map(field => <div key={field.key} className='flex flex-col gap-3 rounded-lg border p-4'><div className='flex items-center justify-between gap-3'><label htmlFor={`expose-${field.key}`} className='text-sm'>开放输入</label><Checkbox id={`expose-${field.key}`} checked={state.exposed.includes(field.key)} disabled={busy} onCheckedChange={checked => setState(current => ({ ...current, exposed: checked ? [...current.exposed, field.key] : current.exposed.filter(key => key !== field.key) }))} /></div><WorkflowParameterFields definition={{ inputs: [{ ...field, required: false }], input_schema: definition.input_schema }} values={definition.execution.defaults} disabled={busy} assets={uploadedAssets} onUploadAsset={(key,file) => upload.mutate({ key,file })} prefix='online-default' onChange={(key, value) => setState(current => ({ ...current, definition: { ...definition, execution: { ...definition.execution, defaults: { ...definition.execution.defaults, [key]: value } } } }))} /></div>)}</section> : null}
    {templates.error || connections.error || saveConnection.error || save.error || upload.error || validation ? <Alert><AlertTitle>{(templates.error ?? connections.error ?? saveConnection.error ?? save.error ?? upload.error)?.message || validation}</AlertTitle></Alert> : null}
    </div>
    <aside className='w-full shrink-0 min-[1200px]:w-80 min-[1200px]:overflow-y-auto min-[1440px]:w-[25rem]'><div className='rounded-xl bg-muted p-5'><BasicsSection value={{ ...metadata, name: state.name, enabled: state.enabled }} onChange={({ name, enabled, ...metadata }) => setState(current => ({ ...current, name, enabled, metadata }))} showEnabled disabled={busy} /></div></aside>
    </div>
    <div className='flex shrink-0 gap-2 bg-card'><Button type='submit' size='sm' disabled={busy || !definition}>{save.isPending ? <Spinner /> : null}保存工作流</Button><Button type='button' size='sm' variant='outline' disabled={busy} onClick={props.onCancel}>取消</Button></div>
    <AlertDialog open={blocker.status === 'blocked'} onOpenChange={open => { if (!open) blocker.reset?.() }}><AlertDialogContent><AlertDialogHeader><AlertDialogTitle>放弃未保存的修改？</AlertDialogTitle><AlertDialogDescription>当前工作流尚未保存。</AlertDialogDescription></AlertDialogHeader><AlertDialogFooter><AlertDialogCancel>继续编辑</AlertDialogCancel><AlertDialogAction disabled={busy} onClick={() => { props.onDiscard?.(); blocker.proceed?.() }}>放弃修改</AlertDialogAction></AlertDialogFooter></AlertDialogContent></AlertDialog>
  </form>
}
