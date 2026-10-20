import type { ExpertContent } from '@/features/agent-assets/expert-form'
import { convertFileSrc } from '@tauri-apps/api/core'
import { createDesktopUpload } from '@/lib/desktop-upload'
import { apiFetch, desktopInvoke } from './client'
import type {
  StudioAsset,
  StudioRun,
  StudioSkill,
  StudioSkillSummary,
  StudioSkillVersion,
} from './studio'
import type { MarketWorkflow } from './workflow-market'

export type AssetScope = 'official' | 'operations' | 'studio'
export type AgentAssetKind = 'skills' | 'experts'
export type AssetPublication = {
  account_id: string
  source: 'official' | 'user'
  published: boolean
  published_version: string
  published_at?: string | null
  market_locked: boolean
  market_lock_reason: string
  available?: boolean
  unavailable_reason?: string
}
export type AgentSkill = StudioSkill & AssetPublication
export type AgentSkillSummary = StudioSkillSummary & AssetPublication
export type Expert = ExpertContent &
  AssetPublication & {
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
  created_by: string
  created_by_type: 'admin' | 'studio_user'
  created_at: string
}
export type AssetSettings = { allow_user_create_experts: boolean }

export function assetPath(scope: AssetScope, kind: AgentAssetKind) {
  if (scope === 'official') return `/api/v1/agent-assets/${kind}`
  if (scope === 'operations')
    return `/api/v1/studio-operations/${kind === 'skills' ? 'skill-market' : 'expert-market'}`
  return `/api/v1/studio/${kind}`
}

export function listAgentSkills(scope: AssetScope) {
  return apiFetch<AgentSkillSummary[]>(assetPath(scope, 'skills'))
}
export function listExperts(scope: AssetScope) {
  return apiFetch<Expert[]>(assetPath(scope, 'experts'))
}
export function listMarketSkills() {
  return apiFetch<AgentSkillSummary[]>('/api/v1/studio/skill-market')
}
export function listMarketExperts() {
  return apiFetch<Expert[]>('/api/v1/studio/expert-market')
}
export function getExpert(scope: AssetScope, id: string) {
  return apiFetch<Expert>(
    `${assetPath(scope, 'experts')}/${encodeURIComponent(id)}`
  )
}
export function getMarketExpert(id: string) {
  return apiFetch<Expert>(
    `/api/v1/studio/expert-market/${encodeURIComponent(id)}`
  )
}
export function listExpertVersions(scope: AssetScope, id: string) {
  return apiFetch<ExpertVersion[]>(
    `${assetPath(scope, 'experts')}/${encodeURIComponent(id)}/versions`
  )
}
export function getExpertVersion(
  scope: AssetScope,
  id: string,
  version: string
) {
  return apiFetch<ExpertVersion>(
    `${assetPath(scope, 'experts')}/${encodeURIComponent(id)}/versions/${encodeURIComponent(version)}`
  )
}
export function saveExpert(
  scope: AssetScope,
  content: ExpertContent,
  version: string,
  notes: string,
  previous?: Expert
) {
  return apiFetch<Expert>(
    `${assetPath(scope, 'experts')}${previous ? `/${encodeURIComponent(previous.id)}` : ''}`,
    {
      method: previous ? 'PATCH' : 'POST',
      body: JSON.stringify({
        ...content,
        version,
        notes,
        enabled: previous?.enabled ?? true,
        ...(previous ? { updated_at: previous.updated_at } : {}),
      }),
    }
  )
}
export function setExpertEnabled(
  scope: AssetScope,
  id: string,
  enabled: boolean
) {
  return apiFetch<Expert>(
    `${assetPath(scope, 'experts')}/${encodeURIComponent(id)}/enabled`,
    { method: 'PATCH', body: JSON.stringify({ enabled }) }
  )
}
export function changeAssetPublication(
  scope: AssetScope,
  kind: AgentAssetKind,
  id: string,
  action: 'publish' | 'unpublish' | 'unlock',
  version?: string,
  reason?: string,
  requestId: string = crypto.randomUUID()
) {
  return apiFetch<unknown>(
    `${assetPath(scope, kind)}/${encodeURIComponent(id)}/${action}`,
    {
      method: 'POST',
      body: JSON.stringify({
        request_id: requestId,
        ...(version ? { version } : {}),
        ...(reason ? { reason } : {}),
      }),
    }
  )
}
export function getAssetSettings(scope: 'operations' | 'studio') {
  return apiFetch<AssetSettings>(
    `/api/v1/${scope === 'operations' ? 'studio-operations' : 'studio'}/asset-settings`
  )
}
export function saveAssetSettings(input: AssetSettings) {
  return apiFetch<AssetSettings>('/api/v1/studio-operations/asset-settings', {
    method: 'PUT',
    body: JSON.stringify(input),
  })
}
export function listAssetSkillVersions(scope: AssetScope, id: string) {
  return apiFetch<StudioSkillVersion[]>(
    `${assetPath(scope, 'skills')}/${encodeURIComponent(id)}/versions`
  )
}
export async function listReferenceSkills(scope: AssetScope) {
  const market = apiFetch<AgentSkill[]>(
    `/api/v1/${scope === 'studio' ? 'studio' : 'agent-assets'}/skill-market`
  )
  if (scope !== 'studio') return market
  const [official, personal] = await Promise.all([market, listAgentSkills('studio')])
  return [...new Map([...official, ...personal].map(skill => [skill.id, skill])).values()]
}
export function listReferenceWorkflows(scope: AssetScope) {
  return apiFetch<MarketWorkflow[]>(
    `/api/v1/${scope === 'studio' ? 'studio' : 'agent-assets'}/workflow-market`
  )
}
export function getMarketSkill(id: string) {
  return apiFetch<AgentSkill>(
    `/api/v1/studio/skill-market/${encodeURIComponent(id)}`
  )
}
export function listAssetVersions(
  scope: AssetScope,
  kind: AgentAssetKind,
  id: string,
  market = false
) {
  const base = market
    ? `/api/v1/${scope === 'official' ? 'agent-assets' : 'studio'}/${kind === 'skills' ? 'skill-market' : 'expert-market'}`
    : assetPath(scope, kind)
  return apiFetch<(ExpertVersion | StudioSkillVersion)[]>(
    `${base}/${encodeURIComponent(id)}/versions`
  )
}
export type AssetPublicationRecord = {
  id: number
  version: string
  action: 'publish' | 'unpublish' | 'unlock'
  reason: string
  created_by: string
  actor_type: string
  created_at: string
}
export function listAssetPublications(
  scope: AssetScope,
  kind: AgentAssetKind,
  id: string
) {
  return apiFetch<AssetPublicationRecord[]>(
    `${assetPath(scope, kind)}/${encodeURIComponent(id)}/publications`
  )
}
export function getAssetVersion(
  scope: AssetScope,
  kind: AgentAssetKind,
  id: string,
  version: string,
  market = false
) {
  const base = market
    ? `/api/v1/${scope === 'official' ? 'agent-assets' : 'studio'}/${kind === 'skills' ? 'skill-market' : 'expert-market'}`
    : assetPath(scope, kind)
  return apiFetch<ExpertVersion | StudioSkillVersion>(
    `${base}/${encodeURIComponent(id)}/versions/${encodeURIComponent(version)}`
  )
}
export async function uploadExpertAvatar(
  _scope: 'official' | 'studio',
  file: File,
  requestId: string
) {
  const upload = await createDesktopUpload(file, undefined, '', requestId, 'expert_avatar')
  return desktopInvoke<StudioAsset>('studio_upload', upload.bytes, {
    headers: { 'x-pixoma-upload': upload.metadata },
  })
}
export function expertAvatarURL(
  scope: AssetScope,
  id: string,
  version: string,
  market = false
) {
  return `${convertFileSrc('', 'pixoma-media')}expert-avatars/${market ? 'market' : scope}/${encodeURIComponent(id)}?version=${encodeURIComponent(version)}`
}
export function testOfficialExpert(
  id: string,
  version: string,
  text: string,
  requestId: string,
  locale: 'zh' | 'en'
) {
  return apiFetch<{ run: StudioRun; session_id: string }>(
    `${assetPath('official', 'experts')}/${encodeURIComponent(id)}/test`,
    {
      method: 'POST',
      body: JSON.stringify({
        version,
        text,
        request_id: requestId,
        locale,
      }),
    }
  )
}
export type OfficialExpertTest = {
  run: StudioRun
  session_id: string
}
export function getOfficialExpertTest(runId: string) {
  return apiFetch<OfficialExpertTest>(
    `/api/v1/agent-assets/test-runs/${encodeURIComponent(runId)}`
  )
}
