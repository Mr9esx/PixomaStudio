import { apiFetch } from './client'
import type { OnlineWorkflowDefinition } from './workflows'

export type GenerationProvider = 'seedance' | 'seedream' | 'minimax' | 'http_json'
export type GenerationConnection = { id: string; name: string; provider: GenerationProvider; base_url: string; has_api_key: boolean; created_at: string }
export type GenerationTemplate = { id: string; name: string; description: string; definition: OnlineWorkflowDefinition }

export function listGenerationConnections() {
  return apiFetch<GenerationConnection[]>('/api/v1/studio/generation-connections')
}
export function saveGenerationConnection(input: { name: string; provider: GenerationProvider; base_url?: string; api_key: string }) {
  return apiFetch<GenerationConnection>('/api/v1/studio/generation-connections', { method: 'POST', body: JSON.stringify(input) })
}
export function listGenerationTemplates() {
  return apiFetch<GenerationTemplate[]>('/api/v1/studio/generation-templates')
}
