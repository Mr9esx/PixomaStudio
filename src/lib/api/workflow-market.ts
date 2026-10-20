import { apiFetch, toQuery } from './client'
import type { CaseRecord, WorkflowExample } from './types'
import type { StudioWorkflowInputField } from './studio'

export type WorkflowVersion = {
  id: number
  case_id: number
  version: string
  notes: string
  document: Omit<CaseRecord, 'enabled' | 'default_version_id'>
  created_by: string
  created_at: string
}

export type WorkflowListing = {
  case_id: number
  version_id: number
  version: string
  published: boolean
  published_by: string
  published_at?: string
  updated_at: string
}

export type OperationWorkflow = {
  workflow: CaseRecord
  listing: WorkflowListing | null
}

export type PublishedOperationWorkflow = OperationWorkflow & { listing: WorkflowListing }

export type MarketWorkflow = {
  id: string
  version_id: number
  name: string
  description: string
  usage: string
  preview?: string
  tags: string[]
  categories: string[]
  examples: WorkflowExample[]
  inputs: number
  outputs: number
  input_schema: Record<string, unknown>
  input_fields: StudioWorkflowInputField[]
  output_fields: CaseRecord['outputs']
}

export type WorkflowTestInput = {
  key: string
  text?: string
  number?: number
  bool?: boolean
  blob?: { key: string; mime: string; size?: number }
}

export function listWorkflowVersions(caseId: number) {
  return apiFetch<WorkflowVersion[]>(`/api/v1/cases/${caseId}/versions`)
}

export function createWorkflowVersion(caseId: number, version: string, notes: string) {
  return apiFetch<WorkflowVersion>(`/api/v1/cases/${caseId}/versions`, {
    method: 'POST', body: JSON.stringify({ version, notes }),
  })
}

export function setDefaultWorkflowVersion(caseId: number, versionId: number) {
  return apiFetch<{ default_version_id: number }>(`/api/v1/cases/${caseId}/versions/${versionId}/default`, { method: 'POST' })
}

export function restoreWorkflowVersion(caseId: number, versionId: number) {
  return apiFetch<{ restored: boolean }>(`/api/v1/cases/${caseId}/versions/${versionId}/restore`, { method: 'POST' })
}

export function testWorkflowVersion(caseId: number, versionId: number, inputs: WorkflowTestInput[]) {
  return apiFetch<{ TaskID: string; WorkflowID: string }>(`/api/v1/cases/${caseId}/versions/${versionId}/test`, {
    method: 'POST', body: JSON.stringify({ inputs }),
  })
}

export function listOperationWorkflows(query?: { q?: string }) {
  return apiFetch<PublishedOperationWorkflow[]>(`/api/v1/studio-operations/workflows${toQuery(query)}`)
}

export function getOperationWorkflow(caseId: number) {
  return apiFetch<OperationWorkflow>(`/api/v1/studio-operations/workflows/${caseId}`)
}

export function publishWorkflowVersion(caseId: number, versionId: number) {
  return apiFetch<WorkflowListing>(`/api/v1/studio-operations/workflows/${caseId}/publish`, {
    method: 'POST', body: JSON.stringify({ version_id: versionId, effect_confirmed: true }),
  })
}

export function unpublishWorkflow(caseId: number) {
  return apiFetch<{ published: false }>(`/api/v1/studio-operations/workflows/${caseId}/unpublish`, { method: 'POST' })
}

export function listMarketWorkflows() {
  return apiFetch<MarketWorkflow[]>('/api/v1/studio/workflow-market')
}

export function getMarketWorkflow(id: string) {
  return apiFetch<MarketWorkflow>(`/api/v1/studio/workflow-market/${encodeURIComponent(id)}`)
}
