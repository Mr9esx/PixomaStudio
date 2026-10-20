import { QueryClient } from '@tanstack/react-query'
import { expect, test } from 'vitest'
import type { StudioUser } from '@/lib/api/operations'
import { replaceStudioIdentity } from './identity-cache'

test('账号变化后清除各个业务目录的缓存并保存当前身份', () => {
  const client = new QueryClient()
  client.setQueryData(['studio', 'projects'], [{ id: 'private-project' }])
  client.setQueryData(['agent-assets', 'skills', 'studio'], [{ id: 'private-skill' }])
  client.setQueryData(['workflow-market', 'private-workflow'], { name: '旧账号资料' })
  const identity: StudioUser = {
    id: 'account-b', username: 'account-b', nickname: '当前账号', avatar_url: '', email: '',
    enabled: true, must_change_password: false, last_login_at: null, created_at: '', updated_at: '',
  }
  replaceStudioIdentity(client, identity)
  expect(client.getQueryData(['studio', 'projects'])).toBeUndefined()
  expect(client.getQueryData(['agent-assets', 'skills', 'studio'])).toBeUndefined()
  expect(client.getQueryData(['workflow-market', 'private-workflow'])).toBeUndefined()
  expect(client.getQueryData(['studio', 'identity'])).toEqual(identity)
  client.setQueryData(['studio', 'projects'], [{ id: 'current-project' }])
  replaceStudioIdentity(client, { ...identity, nickname: '当前昵称' })
  expect(client.getQueryData(['studio', 'projects'])).toEqual([{ id: 'current-project' }])
  expect(client.getQueryData<StudioUser>(['studio', 'identity'])?.nickname).toBe('当前昵称')
  replaceStudioIdentity(client, null)
  expect(client.getQueryCache().getAll()).toHaveLength(0)
})
