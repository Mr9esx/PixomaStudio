import { apiFetch, toQuery } from './client'
import type { Channel, ChannelReachability } from './channels'

export type IMPlatform = 'telegram' | 'feishu' | 'dingtalk'
export function listIMSettings() {
  return apiFetch<{ items: Channel[] }>('/api/v1/im/settings')
}
export function saveIMSettings(platform: IMPlatform, credential: Record<string, string>) {
  return apiFetch<Channel>(`/api/v1/im/settings/${platform}`, {
    method: 'PUT', body: JSON.stringify({ credential }),
  })
}
export function probeIMSettings(platform: IMPlatform) {
  return apiFetch<ChannelReachability>(`/api/v1/im/settings/${platform}/probe`, { method: 'POST' })
}
export function setIMEnabled(platform: IMPlatform, enabled: boolean) {
  return apiFetch<Channel>(`/api/v1/im/settings/${platform}/${enabled ? 'enable' : 'disable'}`, { method: 'POST' })
}

export type MCPCredential = {
  id: string
  name: string
  token_prefix: string
  enabled: boolean
  expires_at: string
  last_used_at: string | null
  created_at: string
  updated_at: string
}
export type MCPTool = { name: string; description: string; input_schema: Record<string, unknown>; output_schema: Record<string, unknown> }
export function getMCPSettings() { return apiFetch<{ enabled: boolean; url: string }>('/api/v1/mcp/settings') }
export function setMCPEnabled(enabled: boolean) { return apiFetch<{ enabled: boolean; url: string }>('/api/v1/mcp/settings', { method: 'PATCH', body: JSON.stringify({ enabled }) }) }
export function listMCPCredentials() { return apiFetch<{ items: MCPCredential[] }>('/api/v1/mcp/credentials') }
export function createMCPCredential(name: string, expires_at: string) { return apiFetch<{ credential: MCPCredential; token: string }>('/api/v1/mcp/credentials', { method: 'POST', body: JSON.stringify({ name, expires_at }) }) }
export function updateMCPCredential(id: string, enabled: boolean) { return apiFetch<MCPCredential>(`/api/v1/mcp/credentials/${encodeURIComponent(id)}`, { method: 'PATCH', body: JSON.stringify({ enabled }) }) }
export function rotateMCPCredential(id: string, expires_at: string) { return apiFetch<{ credential: MCPCredential; token: string }>(`/api/v1/mcp/credentials/${encodeURIComponent(id)}/rotate`, { method: 'POST', body: JSON.stringify({ expires_at }) }) }
export function listMCPTools() { return apiFetch<{ items: MCPTool[] }>('/api/v1/mcp/tools') }

export type StudioUser = {
  id: string
  username: string
  nickname: string
  avatar_url: string
  email: string
  enabled: boolean
  must_change_password: boolean
  last_login_at: string | null
  created_at: string
  updated_at: string
}
export type StudioProfile = Pick<StudioUser, 'nickname' | 'avatar_url' | 'email'>
export function listStudioUsers(params: { q?: string; enabled?: boolean }) { return apiFetch<StudioUser[]>(`/api/v1/studio-operations/users${toQuery(params)}`) }
export function createStudioUser(input: StudioProfile & { username: string }) { return apiFetch<{ user: StudioUser; initial_password: string }>('/api/v1/studio-operations/users', { method: 'POST', body: JSON.stringify(input) }) }
export function updateStudioUser(id: string, input: Partial<StudioProfile> & { enabled?: boolean }) { return apiFetch<StudioUser>(`/api/v1/studio-operations/users/${encodeURIComponent(id)}`, { method: 'PATCH', body: JSON.stringify(input) }) }
export function resetStudioPassword(id: string) { return apiFetch<{ user: StudioUser; initial_password: string }>(`/api/v1/studio-operations/users/${encodeURIComponent(id)}/reset-password`, { method: 'POST', body: '{}' }) }
export function getStudioIdentity() { return apiFetch<StudioUser>('/api/v1/studio/auth/me') }
export function getStudioRegistration() { return apiFetch<{ enabled: boolean }>('/api/v1/studio/auth/registration') }
export function getStudioRegistrationSettings() { return apiFetch<{ enabled: boolean }>('/api/v1/studio-operations/users/registration') }
export function setStudioRegistrationEnabled(enabled: boolean) { return apiFetch<{ enabled: boolean }>('/api/v1/studio-operations/users/registration', { method: 'PATCH', body: JSON.stringify({ enabled }) }) }
export function registerStudio(input: { username: string; email: string; nickname: string; password: string }) { return apiFetch<StudioUser>('/api/v1/studio/auth/register', { method: 'POST', body: JSON.stringify(input) }) }
export function loginStudio(username: string, password: string, remember_me = false) { return apiFetch<StudioUser>('/api/v1/studio/auth/login', { method: 'POST', body: JSON.stringify({ username, password, remember_me }) }) }
export function logoutStudio() { return apiFetch<{ ok: boolean }>('/api/v1/studio/auth/logout', { method: 'POST' }) }
export function updateStudioProfile(input: StudioProfile) { return apiFetch<StudioUser>('/api/v1/studio/auth/profile', { method: 'PATCH', body: JSON.stringify(input) }) }
export function changeStudioPassword(current_password: string, new_password: string) { return apiFetch<StudioUser>('/api/v1/studio/auth/password', { method: 'POST', body: JSON.stringify({ current_password, new_password }) }) }
