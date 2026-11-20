import { z } from 'zod'
import type { CaseRecord } from '@/lib/api/types'
import type { ComfyWorkflowDefinition, Workflow } from '@/lib/api/workflows'

export type WorkflowEditorRecord = Pick<CaseRecord,
  'name' | 'inputs' | 'outputs' | 'bindings' | 'input_schema' | 'workflow_filename' |
  'description' | 'usage' | 'examples' | 'preview' | 'tags' | 'enabled'
> & { category: string; node_id: string }

const inputType = z.enum(['string', 'number', 'boolean', 'enum', 'image', 'video', 'audio', 'file'])
const outputType = z.enum(['image', 'video', 'audio', 'text', 'file'])

export function workflowEditorRecord(workflow: Workflow): WorkflowEditorRecord {
  const definition = workflow.draft
  if (definition.type !== 'comfyui') throw new Error('工作流类型需要 ComfyUI')
  return {
    name: workflow.name, category: workflow.category, tags: workflow.tags, enabled: workflow.enabled,
    description: definition.description, usage: definition.usage, preview: definition.preview,
    examples: definition.examples, inputs: definition.inputs, outputs: definition.outputs,
    input_schema: definition.input_schema, workflow_filename: definition.execution.workflow_filename,
    node_id: definition.execution.node_id,
    bindings: { workflow: definition.execution.workflow, inputs: definition.execution.input_bindings, outputs: definition.execution.output_bindings },
  }
}

export function buildWorkflowDefinition(record: WorkflowEditorRecord): ComfyWorkflowDefinition {
  return {
    type: 'comfyui',
    description: record.description?.trim() ?? '', usage: record.usage?.trim() ?? '',
    preview: record.preview ?? '', examples: record.examples ?? [],
    inputs: record.inputs.map((field) => ({ ...field, type: inputType.parse(field.type) })),
    outputs: record.outputs.map((field) => ({ ...field, type: outputType.parse(field.type) })),
    input_schema: record.input_schema,
    execution: {
      node_id: record.node_id, workflow_filename: record.workflow_filename ?? '',
      workflow: record.bindings.workflow, input_bindings: record.bindings.inputs, output_bindings: record.bindings.outputs,
    },
  }
}
