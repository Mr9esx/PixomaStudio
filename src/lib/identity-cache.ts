import type { QueryClient } from '@tanstack/react-query'
import type { StudioUser } from '@/lib/api/operations'

export function replaceStudioIdentity(client: QueryClient, identity: StudioUser | null): void {
  const current = client.getQueryData<StudioUser>(['studio', 'identity'])
  if (identity && current?.id === identity.id && current.must_change_password === identity.must_change_password) {
    client.setQueryData(['studio', 'identity'], identity)
    return
  }
  client.clear()
  if (identity) client.setQueryData(['studio', 'identity'], identity)
}
