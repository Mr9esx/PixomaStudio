import { apiFetch, toQuery } from './client'

export type WorkflowInput = {
  key: string
  type: 'string' | 'number' | 'boolean' | 'enum' | 'image' | 'video' | 'audio' | 'file'
  required: boolean
  skip_allowed?: boolean
  description?: string
  preview?: string
}

export type WorkflowOutput = {
  key: string
  type: 'image' | 'video' | 'audio' | 'text' | 'file'
  description?: string
  media_type?: string
}

export type WorkflowInputBinding = { key: string; node_id: string; field_path: string }
export type WorkflowOutputBinding = { key: string; node_id: string; index?: number }

type DefinitionFields = {
  inputs: WorkflowInput[]
  input_schema: Record<string, unknown>
  outputs: WorkflowOutput[]
  description?: string
  usage?: string
  preview?: string
  examples?: Array<{ title: string; prompt: string; preview?: string }>
}

export type ComfyWorkflowDefinition = DefinitionFields & {
  type: 'comfyui'
  execution: {
    node_id: string
    workflow: Record<string, unknown>
    workflow_filename?: string
    input_bindings: WorkflowInputBinding[]
    output_bindings: WorkflowOutputBinding[]
  }
}

type GenerationTarget =
  | { provider: 'seedream'; operation: 'image' }
  | { provider: 'minimax'; operation: 'video' }
  | { provider: 'seedance'; operation: 'video' }
  | { provider: 'http_json'; operation: 'video' }

export type HttpJsonGenerationTemplate = {
  cancel_confirms_termination?: boolean
  submit: { method: 'POST'; path: string; body?: unknown }
  query: { method: 'GET' | 'POST'; path: string; body?: unknown }
  cancel?: { method: 'POST' | 'DELETE'; path: string; body?: unknown } | null
  body: Record<string, unknown>
  task_id_pointer: string
  status_pointer: string
  error_pointer: string
  status_mapping: Record<string, 'pending' | 'running' | 'collecting' | 'failed' | 'cancelled'>
  authentication: 'bearer' | 'header' | 'none'
  authentication_header?: string | null
  fixed_mapping?: Record<string, string>
  request_schema?: Record<string, unknown> | null
}

export type OnlineWorkflowDefinition = DefinitionFields & {
  type: 'online'
  execution: GenerationTarget & {
    connection_id: string
    model: string
    capability: string
    defaults: Record<string, unknown>
    input_mapping: Record<string, string>
    output_mapping: Record<string, string>
    http?: HttpJsonGenerationTemplate
  }
}

export type WorkflowDefinition = ComfyWorkflowDefinition | OnlineWorkflowDefinition

export type Workflow = {
  id: string
  name: string
  type: WorkflowDefinition['type']
  category: string
  tags: string[]
  enabled: boolean
  draft: WorkflowDefinition
  default_version_id: string | null
  revision: number
  created_at: string
  updated_at: string
}

export type WorkflowVersion = {
  id: string
  workflow_id: string
  version: string
  definition: WorkflowDefinition
  content_digest: string
  notes: string
  created_at: string
}

const path = '/api/v1/studio/workflows'
const workflowPath = (id: string) => `${path}/${encodeURIComponent(id)}`

export function listWorkflows(filters?: {
  type?: WorkflowDefinition['type']
  category?: string
  enabled?: boolean
  q?: string
}) {
  return apiFetch<Workflow[]>(`${path}${toQuery(filters)}`)
}

export function getWorkflow(id: string) {
  return apiFetch<Workflow>(workflowPath(id))
}

export function createWorkflow(input: {
  name: string
  category: string
  tags?: string[]
  enabled?: boolean
  draft: WorkflowDefinition
}) {
  return apiFetch<Workflow>(path, { method: 'POST', body: JSON.stringify(input) })
}

export function updateWorkflow(id: string, input: {
  expected_revision: number
  name?: string
  category?: string
  tags?: string[]
  enabled?: boolean
  draft?: WorkflowDefinition
}) {
  return apiFetch<Workflow>(workflowPath(id), { method: 'PATCH', body: JSON.stringify(input) })
}

export function deleteWorkflow(id: string) {
  return apiFetch<null>(workflowPath(id), { method: 'DELETE' })
}

export function listWorkflowVersions(id: string) {
  return apiFetch<WorkflowVersion[]>(`${workflowPath(id)}/versions`)
}

export function getWorkflowVersion(id: string, versionId: string) {
  return apiFetch<WorkflowVersion>(`${workflowPath(id)}/versions/${encodeURIComponent(versionId)}`)
}

export type WorkflowReference = {
  id: string
  version_id: string
  name: string
  description: string
  usage: string
  preview?: string
  tags: string[]
  categories: string[]
  examples: NonNullable<WorkflowDefinition['examples']>
  inputs: number
  outputs: number
  input_schema: Record<string, unknown>
  input_fields: WorkflowInput[]
  output_fields: WorkflowOutput[]
}

async function workflowReference(workflow: Workflow): Promise<WorkflowReference> {
  if (!workflow.enabled) throw new Error('工作流已停用')
  if (!workflow.default_version_id) throw new Error('工作流尚未设置默认版本')
  const { definition, id } = await getWorkflowVersion(workflow.id, workflow.default_version_id)
  return {
    id: workflow.id, version_id: id, name: workflow.name, tags: workflow.tags,
    categories: [workflow.category], description: definition.description ?? '',
    usage: definition.usage ?? '', preview: definition.preview, examples: definition.examples ?? [],
    inputs: definition.inputs.length, outputs: definition.outputs.length,
    input_schema: definition.input_schema, input_fields: definition.inputs, output_fields: definition.outputs,
  }
}

export async function getWorkflowReference(id: string) {
  return workflowReference(await getWorkflow(id))
}

export async function listWorkflowReferences() {
  const workflows = await listWorkflows({ enabled: true })
  return Promise.all(workflows.filter(workflow => workflow.default_version_id).map(workflowReference))
}

export function createWorkflowVersion(id: string, input: {
  expected_revision: number
  version: string
  notes?: string
}) {
  return apiFetch<WorkflowVersion>(`${workflowPath(id)}/versions`, {
    method: 'POST', body: JSON.stringify(input),
  })
}

export function setDefaultWorkflowVersion(id: string, versionId: string) {
  return apiFetch<Workflow>(`${workflowPath(id)}/default-version`, {
    method: 'POST', body: JSON.stringify({ version_id: versionId }),
  })
}

export type WorkflowExecutionStatus =
  | 'result_unknown'
  | 'pending'
  | 'dispatching'
  | 'running'
  | 'collecting'
  | 'succeeded'
  | 'failed'
  | 'cancelled'
  | 'submission_unknown'

export type WorkflowExecutionOutput = {
  output_id: string
  field_key: string
  index: number
  sha256: string
  size_bytes: number
  media_type: string
  asset_id: string
  asset_version_id: string
}

export type WorkflowExecution = {
  id: string
  workflow_id: string
  workflow_version_id: string | null
  node_id: string
  session_id: string | null
  status: WorkflowExecutionStatus
  attempt_id: string | null
  prompt_id: string | null
  sequence: number
  cancel_requested: boolean
  can_cancel?: boolean
  usage?: Record<string, unknown> | null
  waiting: boolean
  error: string
  outputs: WorkflowExecutionOutput[]
  inputs?: Record<string, unknown>
  events?: Array<{ sequence: number; kind: string; payload: Record<string, unknown>; created_at: string }>
  created_at: string
  updated_at: string
}

type ExecutionVersion =
  | { version_id: string; draft_revision?: never }
  | { draft_revision: number; version_id?: never }
  | { version_id?: never; draft_revision?: never }

const executionPath = '/api/v1/studio/workflow-executions'

export function createWorkflowExecution(input: ExecutionVersion & {
  request_id: string
  workflow_id: string
  session_id?: string
  inputs: Record<string, unknown>
}) {
  return apiFetch<WorkflowExecution>(executionPath, { method: 'POST', body: JSON.stringify(input) })
}

export function listWorkflowExecutions(filters?: { workflow_id?: string; session_id?: string }) {
  return apiFetch<WorkflowExecution[]>(`${executionPath}${toQuery(filters)}`)
}

export function getWorkflowExecution(id: string) {
  return apiFetch<WorkflowExecution>(`${executionPath}/${encodeURIComponent(id)}`)
}

export function cancelWorkflowExecution(id: string) {
  return apiFetch<WorkflowExecution>(`${executionPath}/${encodeURIComponent(id)}/cancel`, { method: 'POST' })
}

export function stopWaitingForWorkflowExecution(id: string) {
  return apiFetch<WorkflowExecution>(`${executionPath}/${encodeURIComponent(id)}/stop-waiting`, { method: 'POST' })
}
