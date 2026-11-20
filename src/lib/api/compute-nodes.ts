import { apiFetch, toQuery } from './client'
import type { EdgeHardware, EdgeMetrics, EdgeMetricsResponse } from './types'
import type { WorkflowExecution } from './workflows'

export type ComputeNode = {
  id: string
  name: string
  description: string
  location: 'local' | 'remote'
  enabled: boolean
  comfyui_base_url: string
  configuration_revision: number
  agent_version: string | null
  last_seen_at: string | null
  connected: boolean
  comfyui_ready: boolean
  healthy: boolean
  issues: string[]
  comfyui_error: string
  hardware: EdgeHardware | null
  metrics: EdgeMetrics | null
  created_at: string
  updated_at: string
  capabilities: string[]
  started_at: string | null
  comfy_version: string
  local_agent_enabled: boolean
  local_agent_error: string
}

export type ComputeNodeCredential = {
  node: ComputeNode
  agent_token: string
}

const path = '/api/v1/studio/compute-nodes'
const nodePath = (id: string) => `${path}/${encodeURIComponent(id)}`

export type ComputeConnection = { listen_address: string; configured_listen_address: string; local_url: string; public_url: string; remote_access: boolean; environment_override: boolean; restart_required: boolean; listen_error: string | null }
export function getComputeConnection() { return apiFetch<ComputeConnection>('/api/v1/studio/compute-connection') }
export function saveComputeConnection(input: { listen_address: string; public_url: string }) { return apiFetch<ComputeConnection>('/api/v1/studio/compute-connection', { method: 'PUT', body: JSON.stringify(input) }) }
export function getComputeNodeCredential(id: string) { return apiFetch<ComputeNodeCredential>(`${nodePath(id)}/credential`) }
export function controlLocalAgent(id: string, enabled: boolean) { return apiFetch<ComputeNode>(`${nodePath(id)}/agent`, { method: 'POST', body: JSON.stringify({ enabled }) }) }

export function listComputeNodes() {
  return apiFetch<ComputeNode[]>(path)
}

export function getComputeNode(id: string) {
  return apiFetch<ComputeNode>(nodePath(id))
}

export function createComputeNode(input: {
  name: string
  description?: string
  location: ComputeNode['location']
  enabled?: boolean
  comfyui_base_url: string
  capabilities?: string[]
}) {
  return apiFetch<ComputeNodeCredential>(path, { method: 'POST', body: JSON.stringify(input) })
}

export function updateComputeNode(id: string, input: {
  expected_revision: number
  name?: string
  description?: string
  enabled?: boolean
  comfyui_base_url?: string
  capabilities?: string[]
  hardware?: Omit<EdgeHardware, 'collected_at'>
  refresh_hardware?: boolean
}) {
  return apiFetch<ComputeNode>(nodePath(id), { method: 'PATCH', body: JSON.stringify(input) })
}

export function rotateComputeNodeCredential(id: string) {
  return apiFetch<ComputeNodeCredential>(`${nodePath(id)}/credential`, { method: 'POST' })
}

export function deleteComputeNode(id: string) {
  return apiFetch<null>(nodePath(id), { method: 'DELETE' })
}

export type MetricsRange = { kind: 'preset'; window: '1h' | '6h' | '24h' } | { kind: 'custom'; from: string; to: string }

export function getComputeNodeMetrics(id: string, range: MetricsRange) {
  const filters = range.kind === 'preset' ? { window: range.window } : { window: 'custom', from: range.from, to: range.to }
  return apiFetch<EdgeMetricsResponse>(`${nodePath(id)}/metrics${toQuery(filters)}`)
}

export function getComputeNodeObservation(id: string, offset = 0) {
  return apiFetch<{
    stats: { task_count: number; runtime_ms: number; success_rate: number | null }
    executions: WorkflowExecution[]
    workflows: Array<{ id: string; name: string; enabled: boolean }>
    offset: number
  }>(`${nodePath(id)}/observation${toQuery({ offset })}`)
}
