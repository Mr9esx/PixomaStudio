import type { ExpertContent } from '@/features/agent-assets/expert-form'
import { convertFileSrc } from '@tauri-apps/api/core'
import { createDesktopUpload } from '@/lib/desktop-upload'
import { apiFetch, desktopInvoke } from './client'
import { listWorkflowReferences } from './workflows'
import type { StudioAsset, StudioSkill, StudioSkillSummary, StudioSkillVersion } from './studio'

export type AssetScope = 'studio'
export type AgentAssetKind = 'skills' | 'experts'
type Availability = { available: boolean; unavailable_reason: string }
export type AgentSkill = StudioSkill & Availability
export type AgentSkillSummary = StudioSkillSummary & Availability
export type Expert = ExpertContent & Availability & {
  id: string
  version: string
  enabled: boolean
  created_at: string
  updated_at: string
}
export type ExpertVersion = ExpertContent & {
  expert_id: string
  version: string
  notes: string
  created_at: string
}

export function assetPath(_scope: AssetScope, kind: AgentAssetKind) {
  return `/api/v1/studio/${kind}`
}

export function listAgentSkills(scope: AssetScope) {
  return apiFetch<AgentSkillSummary[]>(assetPath(scope, 'skills'))
}
export function listExperts(scope: AssetScope) {
  return apiFetch<Expert[]>(assetPath(scope, 'experts'))
}
export function getExpert(scope: AssetScope, id: string) {
  return apiFetch<Expert>(`${assetPath(scope, 'experts')}/${encodeURIComponent(id)}`)
}
export function listExpertVersions(scope: AssetScope, id: string) {
  return apiFetch<ExpertVersion[]>(`${assetPath(scope, 'experts')}/${encodeURIComponent(id)}/versions`)
}
export function saveExpert(scope: AssetScope, content: ExpertContent, version: string, notes: string, previous?: Expert) {
  return apiFetch<Expert>(`${assetPath(scope, 'experts')}${previous ? `/${encodeURIComponent(previous.id)}` : ''}`, {
    method: previous ? 'PATCH' : 'POST',
    body: JSON.stringify({ ...content, version, notes, enabled: previous?.enabled ?? true, ...(previous ? { updated_at: previous.updated_at } : {}) }),
  })
}
export function setExpertEnabled(scope: AssetScope, id: string, enabled: boolean) {
  return apiFetch<Expert>(`${assetPath(scope, 'experts')}/${encodeURIComponent(id)}/enabled`, { method: 'PATCH', body: JSON.stringify({ enabled }) })
}
export function listReferenceSkills(scope: AssetScope) {
  return listAgentSkills(scope)
}
export function listReferenceWorkflows(_scope: AssetScope) {
  return listWorkflowReferences()
}
export function listAssetVersions(scope: AssetScope, kind: AgentAssetKind, id: string) {
  return apiFetch<(ExpertVersion | StudioSkillVersion)[]>(`${assetPath(scope, kind)}/${encodeURIComponent(id)}/versions`)
}
export function getAssetVersion(scope: AssetScope, kind: AgentAssetKind, id: string, version: string) {
  return apiFetch<ExpertVersion | StudioSkillVersion>(`${assetPath(scope, kind)}/${encodeURIComponent(id)}/versions/${encodeURIComponent(version)}`)
}
export async function uploadExpertAvatar(_scope: AssetScope, file: File, requestId: string) {
  const upload = await createDesktopUpload(file, undefined, '', requestId, 'expert_avatar')
  return desktopInvoke<StudioAsset>('studio_upload', upload.bytes, { headers: { 'x-pixoma-upload': upload.metadata } })
}
export function expertAvatarURL(scope: AssetScope, id: string, version: string) {
  return `${convertFileSrc('', 'pixoma-media')}expert-avatars/${scope}/${encodeURIComponent(id)}?version=${encodeURIComponent(version)}`
}
