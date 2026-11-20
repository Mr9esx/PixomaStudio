import { useMemo, useState } from 'react'
import { z } from 'zod'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Pencil } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { updateWorkflow, type Workflow, type WorkflowDefinition } from '@/lib/api/workflows'
import { Badge } from '@/components/ui/badge'
import { CodeEditor } from '@/components/code-editor'
import { Button } from '@/components/ui/button'
import { EditableInputFields, EditableOutputFields, InputFieldsTable, OutputFieldsTable } from './field-cards'
import { WorkflowGraphViewer } from './workflow-import'
import { parseWorkflow } from '../lib/workflow-parse'
import { deriveBindings, deriveInputSchema, normalizeOutputTypes, validateEditor, type InputFieldDraft, type OutputFieldDraft } from '../lib/derive'
import { useIsWide } from './use-is-wide'
import { buildWorkflowDefinition, workflowEditorRecord } from '../lib/workflow-record'

export function WorkflowConfigView({ definition, workflow }: { definition: WorkflowDefinition; workflow?: Workflow }) {
  const { t } = useTranslation()
  const client = useQueryClient()
  const wide = useIsWide()
  const [editing, setEditing] = useState<'inputs' | 'outputs'>()
  const [inputDrafts, setInputDrafts] = useState<InputFieldDraft[]>([])
  const [outputDrafts, setOutputDrafts] = useState<OutputFieldDraft[]>([])
  const comfy = definition.type === 'comfyui' ? definition.execution : undefined
  const graphText = comfy ? JSON.stringify(comfy.workflow, null, 2) : ''
  const parsed = useMemo(() => graphText ? parseWorkflow(graphText) : undefined, [graphText])
  const nodes = parsed?.ok ? parsed.graph.nodes : []
  const inputs = definition.inputs.map(field => ({ ...field, node_id: comfy?.input_bindings.find(binding => binding.key === field.key)?.node_id ?? '', field_path: comfy?.input_bindings.find(binding => binding.key === field.key)?.field_path ?? '', enum_values: (definition.input_schema.properties as Record<string, { enum?: string[] }>)?.[field.key]?.enum }))
  const outputs = definition.outputs.map(field => ({ ...field, node_id: comfy?.output_bindings.find(binding => binding.key === field.key)?.node_id ?? '', index: comfy?.output_bindings.find(binding => binding.key === field.key)?.index ?? 0 }))
  const save = useMutation({ mutationFn: async () => {
    if (!workflow || definition.type !== 'comfyui') throw new Error('工作流编辑信息缺失')
    const nextInputs = editing === 'inputs' ? inputDrafts : inputs
    const nextOutputs = editing === 'outputs' ? outputDrafts : outputs
    const validation = validateEditor(nextInputs, nextOutputs)
    if (validation.duplicateKey || validation.unboundInput || validation.noOutput) throw new Error(t(validation.duplicateKey ? 'cases.errDuplicateKey' : validation.unboundInput ? 'cases.errInputNotBound' : 'cases.errNoOutput'))
    const bindings = deriveBindings(nextInputs, nextOutputs)
    const record = workflowEditorRecord(workflow)
    const schema = deriveInputSchema(nextInputs)
    if (editing === 'inputs') {
      const previousProperties = definition.input_schema.properties as Record<string, object>
      const properties = schema.properties as Record<string, object>
      schema.properties = Object.fromEntries(nextInputs.map(field => [field.key, { ...(inputs.some(previous => previous.key === field.key && previous.type === field.type) ? previousProperties[field.key] : {}), ...properties[field.key] }]))
    }
    return updateWorkflow(workflow.id, { expected_revision: workflow.revision, draft: buildWorkflowDefinition({ ...record,
      inputs: nextInputs.map(({ key, type, required, description, skip_allowed, preview }) => ({ key, type: z.enum(['string', 'number', 'boolean', 'enum', 'image', 'video', 'audio', 'file']).parse(type), required, description, skip_allowed, preview })),
      outputs: normalizeOutputTypes(nextOutputs, nodes).map(({ key, type, description, media_type }) => ({ key, type: z.enum(['image', 'video', 'audio', 'text', 'file']).parse(type), description, media_type })),
      input_schema: editing === 'inputs' ? schema : definition.input_schema,
      bindings: { ...record.bindings, inputs: bindings.inputs, outputs: bindings.outputs },
    }) })
  }, onSuccess: next => { client.setQueryData(['studio', 'workflow', next.id], next); void client.invalidateQueries({ queryKey: ['studio', 'workflows'] }); setEditing(undefined) } })
  const actions = (kind: 'inputs' | 'outputs') => workflow && comfy ? <Button type='button' variant='outline' size='sm' className='h-7 gap-1.5 px-2.5 text-xs' disabled={save.isPending} onClick={() => { save.reset(); setInputDrafts(inputs); setOutputDrafts(outputs); setEditing(kind) }}><Pencil className='size-3' />{t(kind === 'inputs' ? 'cases.editInputs' : 'cases.editOutputs')}</Button> : null
  const footer = <div className='flex justify-end gap-2'><Button size='sm' variant='outline' disabled={save.isPending} onClick={() => setEditing(undefined)}>{t('common.cancel')}</Button><Button size='sm' disabled={save.isPending} onClick={() => save.mutate()}>{t('common.save')}</Button></div>
  return <div className='flex min-w-0 flex-col gap-4' data-testid='case-workflow-config-view'>
    <section className='flex min-w-0 flex-col gap-2'><header className='flex flex-wrap items-center justify-between gap-2'><div className='flex min-w-0 flex-wrap items-center gap-1.5'><Badge variant='default' className='h-auto shrink-0 px-1.5 py-0.5 text-xs leading-none'>{t('cases.inputPill')}</Badge><h3 className='text-sm font-semibold'>{t('cases.inputsHeading')}</h3></div>{editing !== 'inputs' ? actions('inputs') : null}</header>{editing === 'inputs' ? <><EditableInputFields nodes={nodes} fields={inputDrafts} wide={wide} disabled={save.isPending} onChange={(index,next) => setInputDrafts(current => current.map((field,i) => index === i ? next : field))} onRemove={index => setInputDrafts(current => current.filter((_,i) => i !== index))} onReorder={setInputDrafts} onAdd={() => setInputDrafts(current => [...current, { key: '', type: 'string', required: false, node_id: '', field_path: '' }])} />{footer}</> : <InputFieldsTable nodes={nodes} fields={inputs} onChange={() => {}} onRemove={() => {}} disabled hideActions />}</section>
    <section className='flex min-w-0 flex-col gap-2'><header className='flex flex-wrap items-center justify-between gap-2'><div className='flex min-w-0 flex-wrap items-center gap-1.5'><Badge variant='default' className='h-auto shrink-0 px-1.5 py-0.5 text-xs leading-none'>{t('cases.outputPill')}</Badge><h3 className='text-sm font-semibold'>{t('cases.outputsHeading')}</h3></div>{editing !== 'outputs' ? actions('outputs') : null}</header>{editing === 'outputs' ? <><EditableOutputFields nodes={nodes} fields={outputDrafts} wide={wide} disabled={save.isPending} onChange={(index,next) => setOutputDrafts(current => current.map((field,i) => index === i ? next : field))} onRemove={index => setOutputDrafts(current => current.filter((_,i) => i !== index))} onAdd={() => setOutputDrafts(current => [...current, { key: '', type: 'image', node_id: '', index: 0 }])} />{footer}</> : <OutputFieldsTable nodes={nodes} fields={outputs} onChange={() => {}} onRemove={() => {}} disabled hideActions />}</section>
    {save.error ? <p role='alert' className='text-sm text-destructive'>{save.error.message}</p> : null}
    {comfy ? <>
      {parsed?.ok ? <WorkflowGraphViewer value={graphText} graph={parsed.graph} onChange={() => {}} readOnly /> : <p role='alert' className='text-sm text-destructive'>{parsed && !parsed.ok ? parsed.error : '工作流内容缺失'}</p>}
    </> : <section className='flex min-w-0 flex-col gap-3'><h3 className='text-sm font-semibold'>在线模型配置</h3><CodeEditor value={JSON.stringify(definition.execution, null, 2)} readOnly minHeight={200} maxHeight={500} /></section>}
  </div>
}
