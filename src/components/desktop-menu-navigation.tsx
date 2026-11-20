import { useEffect } from 'react'
import { flushSync } from 'react-dom'
import { useNavigate } from '@tanstack/react-router'
import { emit, listen } from '@tauri-apps/api/event'
import { desktopInvoke } from '@/lib/api/client'
import { prepareDesktopExit } from '@/lib/desktop-exit'

export function DesktopMenuNavigation() {
  const navigate = useNavigate()
  useEffect(() => {
    const subscription = listen<string>('studio:navigate', event => {
      if (!['models', 'skills', 'mcp', 'compute-nodes'].includes(event.payload)) throw new Error('设置页面不存在')
      void navigate({ to: '/studio/settings/$section', params: { section: event.payload } })
    })
    return () => { void subscription.then(unsubscribe => unsubscribe()) }
  }, [navigate])
  useEffect(() => {
    let closing = false
    const subscription = listen('studio:exit-requested', async () => {
      if (closing) return
      closing = true
      try {
        flushSync(() => { if (document.activeElement instanceof HTMLElement) document.activeElement.blur() })
        if (await prepareDesktopExit()) await desktopInvoke<void>('studio_confirm_exit')
      } finally { closing = false }
    })
    void subscription.then(() => emit('studio:exit-guard-ready', true))
    return () => { void subscription.then(async unsubscribe => { unsubscribe(); await emit('studio:exit-guard-ready', false) }) }
  }, [])
  return null
}
