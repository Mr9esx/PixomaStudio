import { useEffect } from 'react'
import { useNavigate } from '@tanstack/react-router'
import { listen } from '@tauri-apps/api/event'

export function DesktopMenuNavigation() {
  const navigate = useNavigate()
  useEffect(() => {
    const subscription = listen<string>('studio:navigate', event => {
      if (!['models', 'skills', 'mcp'].includes(event.payload)) throw new Error('设置页面不存在')
      void navigate({ to: '/studio/settings/$section', params: { section: event.payload } })
    })
    return () => { void subscription.then(unsubscribe => unsubscribe()) }
  }, [navigate])
  return null
}
