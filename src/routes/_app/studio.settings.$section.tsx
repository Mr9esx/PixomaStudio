import { createFileRoute, redirect } from '@tanstack/react-router'

export const Route = createFileRoute('/_app/studio/settings/$section')({
  beforeLoad: ({ params }) => {
    if (params.section === 'sessions' || params.section === 'account') {
      return
    }
    if (params.section === 'workflows') {
      throw redirect({ to: '/studio/workflows', replace: true })
    }
    if (params.section === 'data') {
      throw redirect({
        to: '/studio/settings/$section',
        params: { section: 'sessions' },
        replace: true,
      })
    }
    if (!['models', 'skills', 'mcp'].includes(params.section)) {
      throw redirect({
        to: '/studio/settings/$section',
        params: { section: 'sessions' },
        replace: true,
      })
    }
  },
})
