import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useBlocker } from '@tanstack/react-router'
import { Plus, Settings2, Sparkles, TriangleAlert } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { createWorkflow, updateWorkflow, type Workflow } from '@/lib/api/workflows'
import { listComputeNodes } from '@/lib/api/compute-nodes'
import { Alert, AlertTitle } from '@/components/ui/alert'
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from '@/components/ui/alert-dialog'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Field, FieldGroup, FieldLabel } from '@/components/ui/field'
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Spinner } from '@/components/ui/spinner'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { StatusDot } from '@/components/status-dot'
import { emptyCase } from './empty-case'
import { deriveBindings, deriveInputSchema, normalizeOutputTypes, validateEditor, type InputFieldDraft, type OutputFieldDraft } from './lib/derive'
import { outputKindFor } from './lib/node-catalog'
import { parseWorkflow, type WorkflowNode } from './lib/workflow-parse'
import { buildWorkflowDefinition, workflowEditorRecord, type WorkflowEditorRecord } from './lib/workflow-record'
import { BasicsSection } from './sections/basics'
import { EditableInputFields, EditableOutputFields } from './sections/field-cards'
import { useIsWide } from './sections/use-is-wide'
import { WorkflowImportSection } from './sections/workflow-import'
import { OnlineWorkflowEditor, type OnlineWorkflowEditorState } from './online-workflow-editor'

type ComfyWorkflowEditorState = {
  section?: 'info' | 'workflow'
  kind?: 'comfyui'
  baseline?: string
  workflowId?: string
  revision?: number
  draft: WorkflowEditorRecord
  workflowText: string
  inputDrafts: InputFieldDraft[]
  outputDrafts: OutputFieldDraft[]
}

export type WorkflowEditorState = ComfyWorkflowEditorState | OnlineWorkflowEditorState

export type WorkflowEditorProps = {
  section?: 'info' | 'workflow'
  initial?: Workflow
  state?: WorkflowEditorState
  onStateChange?: (state: WorkflowEditorState) => void
  onSaved: (next: Workflow) => void
  onCancel: () => void
  onDiscard?: () => void
  onManageNodes?: (mode: 'create' | 'manage') => void
}

function toInputDrafts(record: WorkflowEditorRecord): InputFieldDraft[] {
  const byKey = new Map(record.bindings.inputs.map((b) => [b.key, b]))
  const properties = record.input_schema.properties as Record<string, { enum?: unknown[] }> | undefined
  return record.inputs.map((field) => ({
    ...field,
    node_id: byKey.get(field.key)?.node_id ?? '', field_path: byKey.get(field.key)?.field_path ?? '',
    enum_values: properties?.[field.key]?.enum?.filter((value): value is string => typeof value === 'string'),
  }))
}

function toOutputDrafts(record: WorkflowEditorRecord): OutputFieldDraft[] {
  const byKey = new Map(record.bindings.outputs.map((b) => [b.key, b]))
  return record.outputs.map((field) => ({ ...field, node_id: byKey.get(field.key)?.node_id ?? '', index: byKey.get(field.key)?.index ?? 0 }))
}

function generateInputCandidates(nodes: WorkflowNode[]): InputFieldDraft[] {
  const out: InputFieldDraft[] = []
  const seen = new Set<string>()
  for (const n of nodes) {
    for (const input of n.inputs) {
      if (input.ref) continue
      let key = input.name
      let i = 2
      while (seen.has(key)) key = `${input.name}_${i++}`
      seen.add(key)
      out.push({ key, type: input.kind === 'unknown' ? 'string' : input.kind, required: false, node_id: n.id, field_path: input.name })
    }
  }
  return out
}

function generateOutputCandidates(nodes: WorkflowNode[]): OutputFieldDraft[] {
  const out: OutputFieldDraft[] = []
  const seen = new Set<string>()
  for (const n of nodes) {
    for (let i = 0; i < n.outputCount; i += 1) {
      const base = `${n.id}[${i}]`
      let key = base
      let k = 2
      while (seen.has(key)) key = `${base}_${k++}`
      seen.add(key)
      out.push({ key, type: outputKindFor(n.class_type), node_id: n.id, index: i })
    }
  }
  return out
}

function inputFieldErrors(inputs: InputFieldDraft[]): Record<number, boolean> {
  const errors: Record<number, boolean> = {}
  const seen = new Map<string, number>()
  inputs.forEach((field, index) => {
    const key = field.key.trim()
    if (!key) return
    if (seen.has(key)) errors[index] = true
    else seen.set(key, index)
    if (!field.node_id || !field.field_path) errors[index] = true
  })
  return errors
}

export function WorkflowEditor(props: WorkflowEditorProps) {
  const [kind, setKind] = useState<'comfyui' | 'online'>(props.initial?.type ?? props.state?.kind ?? 'comfyui')
  const drafts = useRef<Partial<Record<'comfyui' | 'online', WorkflowEditorState>>>({ [kind]: props.state })
  const onStateChange = useCallback((next: WorkflowEditorState) => { drafts.current[kind] = next; props.onStateChange?.(next) }, [kind, props.onStateChange])
  const editorProps = { ...props, state: drafts.current[kind], onStateChange }
  return <div className='flex min-h-0 flex-1 flex-col gap-3'>
    {!props.initial ? <Tabs className='shrink-0' value={kind} onValueChange={value => setKind(value as 'comfyui' | 'online')}><TabsList aria-label='工作流执行方式'><TabsTrigger value='comfyui'>自建 ComfyUI</TabsTrigger><TabsTrigger value='online'>在线模型</TabsTrigger></TabsList></Tabs> : null}
    {kind === 'online' ? <OnlineWorkflowEditor {...editorProps} /> : <ComfyWorkflowEditor {...editorProps} />}
  </div>
}

function ComfyWorkflowEditor(props: WorkflowEditorProps) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const isWide = useIsWide()
  const section = props.section ?? (props.state?.kind !== 'online' ? props.state?.section : undefined)
  const showWorkflow = section !== 'info'
  const showBasics = section !== 'workflow'
  const [state, setState] = useState<ComfyWorkflowEditorState>(() => {
    if (props.state && props.state.kind !== 'online' && props.state.workflowId === props.initial?.id && (props.section === undefined || props.section === props.state.section)) return props.state
    const draft = props.initial ? workflowEditorRecord(props.initial) : emptyCase()
    const initial = { section, workflowId: props.initial?.id, revision: props.initial?.revision, draft,
      workflowText: Object.keys(draft.bindings.workflow).length ? JSON.stringify(draft.bindings.workflow, null, 2) : '',
      inputDrafts: toInputDrafts(draft), outputDrafts: toOutputDrafts(draft) }
    return { ...initial, baseline: JSON.stringify(initial) }
  })
  const { draft, workflowText, inputDrafts, outputDrafts } = state
  const leaving = useRef(false)
  const dirty = JSON.stringify({ ...state, baseline: undefined }) !== state.baseline
  const blocker = useBlocker({ shouldBlockFn: ({ next }) => !leaving.current && (saveMutation.isPending || dirty && next.pathname !== '/studio/settings/compute-nodes'), enableBeforeUnload: () => dirty || saveMutation.isPending, withResolver: true })
  const parsed = useMemo(() => workflowText.trim() ? parseWorkflow(workflowText) : undefined, [workflowText])
  const graph = parsed?.ok ? parsed.graph : undefined
  const importError = parsed && !parsed.ok ? parsed.error : undefined
  const [nameError, setNameError] = useState<string>()
  const [showFieldErrors, setShowFieldErrors] = useState(false)
  const nameRef = useRef<HTMLInputElement>(null)
  const editorValidation = useMemo(() => validateEditor(inputDrafts, outputDrafts), [inputDrafts, outputDrafts])
  const nodes = useQuery({ queryKey: ['studio', 'compute-nodes'], queryFn: listComputeNodes, refetchInterval: 10000 })
  const saveMutation = useMutation({
    mutationFn: (record: WorkflowEditorRecord) => {
      const input = { name: record.name.trim(), category: record.category, tags: record.tags ?? [], enabled: record.enabled, draft: buildWorkflowDefinition(record) }
      if (state.workflowId) {
        if (state.revision === undefined) throw new Error('工作流修订缺失')
        return updateWorkflow(state.workflowId, { ...input, expected_revision: state.revision })
      }
      return createWorkflow(input)
    },
    onSuccess: (saved) => {
      leaving.current = true
      queryClient.setQueryData(['studio', 'workflow', saved.id], saved)
      void queryClient.invalidateQueries({ queryKey: ['studio', 'workflows'] })
      props.onSaved(saved)
    },
  })
  const pending = saveMutation.isPending
  useEffect(() => { props.onStateChange?.(state) }, [state, props.onStateChange])

  function patchDraft(patch: Partial<WorkflowEditorRecord>) {
    if (patch.name !== undefined) setNameError(undefined)
    setState((previous) => ({ ...previous, draft: { ...previous.draft, ...patch } }))
  }
  function generateInputs() {
    if (!graph) return
    const existing = new Set(inputDrafts.map((f) => `${f.node_id}\u0000${f.field_path}`))
    const add = generateInputCandidates(graph.nodes).filter((c) => !existing.has(`${c.node_id}\u0000${c.field_path}`))
    if (add.length) setState((previous) => ({ ...previous, inputDrafts: [...previous.inputDrafts, ...add] }))
  }
  function generateOutputs() {
    if (!graph) return
    const existing = new Set(outputDrafts.map((f) => `${f.node_id}\u0000${f.index ?? 0}`))
    const add = generateOutputCandidates(graph.nodes).filter((c) => !existing.has(`${c.node_id}\u0000${c.index ?? 0}`))
    if (add.length) setState((previous) => ({ ...previous, outputDrafts: [...previous.outputDrafts, ...add] }))
  }
  function buildPayload(): WorkflowEditorRecord | null {
    if (!showWorkflow) return { ...draft, name: draft.name.trim() }
    if (!graph || editorValidation.duplicateKey || editorValidation.unboundInput || editorValidation.noOutput) {
      setShowFieldErrors(true)
      return null
    }
    setShowFieldErrors(false)
    const bindings = deriveBindings(inputDrafts, outputDrafts)
    return {
      ...draft, name: draft.name.trim(),
      inputs: inputDrafts.map((f) => ({ key: f.key.trim(), type: f.type, required: f.required,
        description: f.description?.trim(), skip_allowed: f.skip_allowed, preview: f.preview })),
      outputs: normalizeOutputTypes(outputDrafts, graph.nodes).map((f) => ({ key: f.key.trim(), type: f.type, description: f.description?.trim(), media_type: f.media_type })),
      bindings: { workflow: graph.api, ...bindings }, input_schema: deriveInputSchema(inputDrafts),
    }
  }
  function onSubmit(e: React.FormEvent) {
    e.preventDefault()
    if (!draft.name.trim()) { setNameError(t('cases.errNameRequired')); nameRef.current?.focus(); return }
    const payload = buildPayload()
    if (payload) saveMutation.mutate(payload)
  }

  return (
    <form id='workflow-editor' onSubmit={onSubmit} className='flex min-h-0 flex-1 flex-col gap-3' noValidate data-testid='case-form'>
      <div className='flex min-h-0 flex-1 flex-col gap-6 overflow-y-auto min-[1200px]:flex-row min-[1200px]:overflow-hidden'>
        {showWorkflow ? <div className='relative flex min-w-0 flex-1 flex-col gap-6 min-[1200px]:min-h-0 min-[1200px]:overflow-y-auto'>
          <WorkflowImportSection value={workflowText} graph={graph} error={importError} filename={draft.workflow_filename || undefined}
            onChange={(workflowText) => setState((previous) => ({ ...previous, workflowText }))}
            onFileName={(workflow_filename) => patchDraft({ workflow_filename })} disabled={pending} />
          <section className='relative flex flex-col gap-3' data-testid='case-section-inputs'>
            <div className='flex items-center gap-2'>
              <span className='flex size-5 shrink-0 items-center justify-center rounded-full bg-primary text-xs font-semibold text-primary-foreground'>2</span>
              <div className='min-w-0 flex-1'><div className='flex min-w-0 flex-wrap items-center gap-1.5'>
                <Badge variant='default' className='h-auto shrink-0 px-1.5 py-0.5 text-xs leading-none'>{t('cases.inputPill')}</Badge>
                <h3 className='text-sm font-semibold'>{t('cases.inputsHeading')}</h3>
              </div></div>
              {graph ? <Button type='button' size='sm' variant='ghost' disabled={pending} onClick={generateInputs} className='shrink-0 gap-1'><Sparkles />{t('cases.autoGenerateInputs')}</Button> : null}
            </div>
            {showFieldErrors && (editorValidation.duplicateKey || editorValidation.unboundInput) ? <Alert variant='destructive'><TriangleAlert /><AlertTitle>
              {editorValidation.duplicateKey ? t('cases.errDuplicateKey') : t('cases.errInputNotBound')}
            </AlertTitle></Alert> : null}
            <EditableInputFields nodes={graph?.nodes ?? []} fields={inputDrafts}
              onChange={(index, next) => setState((previous) => ({ ...previous, inputDrafts: previous.inputDrafts.map((row, i) => i === index ? next : row) }))}
              onRemove={(index) => setState((previous) => ({ ...previous, inputDrafts: previous.inputDrafts.filter((_, i) => i !== index) }))}
              onReorder={graph ? (inputDrafts) => setState((previous) => ({ ...previous, inputDrafts })) : undefined}
              onAdd={() => setState((previous) => ({ ...previous, inputDrafts: [...previous.inputDrafts, { key: '', type: 'string', required: false, node_id: '', field_path: '' }] }))}
              wide={isWide || (!graph && inputDrafts.length === 0)} disabled={pending || !graph} lockReason={graph ? undefined : t('cases.emptyWorkflowLock')}
              fieldErrors={showFieldErrors ? inputFieldErrors(inputDrafts) : undefined} />
          </section>
          <section className='relative flex flex-col gap-3' data-testid='case-section-outputs'>
            <div className='flex items-center gap-2'>
              <span className='flex size-5 shrink-0 items-center justify-center rounded-full bg-primary text-xs font-semibold text-primary-foreground'>3</span>
              <div className='min-w-0 flex-1'><div className='flex min-w-0 flex-wrap items-center gap-1.5'>
                <Badge variant='default' className='h-auto shrink-0 px-1.5 py-0.5 text-xs leading-none'>{t('cases.outputPill')}</Badge>
                <h3 className='text-sm font-semibold'>{t('cases.outputsHeading')}</h3>
              </div></div>
              {graph ? <Button type='button' size='sm' variant='ghost' disabled={pending} onClick={generateOutputs} className='shrink-0 gap-1'><Sparkles />{t('cases.autoGenerateOutputs')}</Button> : null}
            </div>
            {showFieldErrors && editorValidation.noOutput ? <Alert variant='destructive'><TriangleAlert /><AlertTitle>{t('cases.errNoOutput')}</AlertTitle></Alert> : null}
            <EditableOutputFields nodes={graph?.nodes ?? []} fields={outputDrafts}
              onChange={(index, next) => setState((previous) => ({ ...previous, outputDrafts: previous.outputDrafts.map((row, i) => i === index ? next : row) }))}
              onRemove={(index) => setState((previous) => ({ ...previous, outputDrafts: previous.outputDrafts.filter((_, i) => i !== index) }))}
              onAdd={() => setState((previous) => ({ ...previous, outputDrafts: [...previous.outputDrafts, { key: '', type: 'image', node_id: '', index: 0 }] }))}
              wide={isWide || (!graph && outputDrafts.length === 0)} disabled={pending || !graph} lockReason={graph ? undefined : t('cases.emptyWorkflowLock')} />
          </section>
          <section className='relative flex flex-col gap-3' data-testid='case-section-compute-node'>
            <div className='flex items-center gap-2'>
              <span className='flex size-5 shrink-0 items-center justify-center rounded-full bg-primary text-xs font-semibold text-primary-foreground'>4</span>
              <h3 className='text-sm font-semibold'>计算节点</h3>
            </div>
            <FieldGroup>
              <Field>
                <FieldLabel htmlFor='workflow-node'>执行节点</FieldLabel>
                <Select value={draft.node_id || 'unselected'} onValueChange={(node_id) => patchDraft({ node_id: node_id === 'unselected' ? '' : node_id })} disabled={pending || nodes.isPending}>
                  <SelectTrigger id='workflow-node'><SelectValue placeholder='选择计算节点' /></SelectTrigger>
                  <SelectContent><SelectGroup>
                    <SelectItem value='unselected'>暂不选择</SelectItem>
                    {nodes.data?.map((node) => <SelectItem key={node.id} value={node.id}>
                      <StatusDot label={nodes.isSuccess && node.healthy ? '节点就绪' : nodes.error ? '节点状态暂不可用' : node.issues.join('；')} problems={nodes.isSuccess && node.healthy ? 0 : 1} />{node.name}
                    </SelectItem>)}
                  </SelectGroup></SelectContent>
                </Select>
                {nodes.error ? <p role='alert' className='text-sm text-destructive'>{nodes.error.message}</p> : null}
              </Field>
            </FieldGroup>
            {props.onManageNodes ? <div className='flex flex-wrap gap-2'>
              <Button type='button' variant='outline' size='sm' onClick={() => props.onManageNodes?.('create')}><Plus />新建节点</Button>
              <Button type='button' variant='ghost' size='sm' onClick={() => props.onManageNodes?.('manage')}><Settings2 />管理节点</Button>
            </div> : null}
          </section>
        </div> : null}
        {showBasics ? <div className={showWorkflow ? 'relative w-full shrink-0 min-[1200px]:w-80 min-[1200px]:overflow-y-auto min-[1440px]:w-[25rem]' : 'w-full max-w-3xl overflow-y-auto'}>
          <div className='rounded-lg border border-border bg-card p-5'>
            <BasicsSection value={draft} onChange={patchDraft} showEnabled disabled={pending} nameError={nameError} nameRef={nameRef} />
          </div>
        </div> : null}
      </div>
      {saveMutation.error ? <Alert variant='destructive' role='alert'><TriangleAlert /><AlertTitle>{saveMutation.error.message}</AlertTitle></Alert> : null}
      <div className='flex shrink-0 flex-wrap gap-2 bg-card'>
        <Button type='submit' size='sm' disabled={pending}>{pending ? <Spinner /> : null}{t('common.save')}</Button>
        <Button type='button' variant='outline' size='sm' disabled={pending} onClick={props.onCancel}>{t('common.cancel')}</Button>
      </div>
      <AlertDialog open={blocker.status === 'blocked'} onOpenChange={open => { if (!open) blocker.reset?.() }}><AlertDialogContent><AlertDialogHeader><AlertDialogTitle>放弃未保存的修改？</AlertDialogTitle><AlertDialogDescription>当前工作流尚未保存。</AlertDialogDescription></AlertDialogHeader><AlertDialogFooter><AlertDialogCancel>继续编辑</AlertDialogCancel><AlertDialogAction disabled={pending} onClick={() => { props.onDiscard?.(); blocker.proceed?.() }}>放弃修改</AlertDialogAction></AlertDialogFooter></AlertDialogContent></AlertDialog>
    </form>
  )
}
