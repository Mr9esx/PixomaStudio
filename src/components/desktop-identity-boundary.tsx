import { Fragment, useEffect, useState, type ReactNode } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { useNavigate } from '@tanstack/react-router'
import { listen } from '@tauri-apps/api/event'
import type { StudioUser } from '@/lib/api/operations'
import { replaceStudioIdentity } from '@/lib/identity-cache'

export function DesktopIdentityBoundary({ children }: { children: ReactNode }) {
  const client = useQueryClient()
  const navigate = useNavigate()
  const [generation, setGeneration] = useState(0)
  const [error, setError] = useState<Error>()
  useEffect(() => {
    let disposed = false
    let unsubscribe: (() => void) | undefined
    const fail = (error: unknown) => { if (!disposed) setError(error instanceof Error ? error : new Error(String(error))) }
    void listen<{ identity: StudioUser | null }>('studio:identity-changed', (event) => {
      if (disposed) return
      replaceStudioIdentity(client, event.payload.identity)
      setGeneration((current) => current + 1)
      void navigate({ to: '/studio', search: {} }).catch(fail)
    }).then((stop) => {
      if (disposed) stop()
      else unsubscribe = stop
    }).catch(fail)
    return () => { disposed = true; unsubscribe?.() }
  }, [client, navigate])
  if (error) throw error
  return <Fragment key={generation}>{children}</Fragment>
}
