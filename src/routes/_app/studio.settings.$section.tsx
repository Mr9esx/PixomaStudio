import { createFileRoute, redirect } from '@tanstack/react-router'

export const Route = createFileRoute('/_app/studio/settings/$section')({
  beforeLoad: ({ params }) => {
    if (params.section === 'workflows') {
      throw redirect({ to: '/studio/workflows', replace: true })
    }
    if (!['general', 'models', 'skills', 'mcp', 'compute-nodes'].includes(params.section)) {
      throw redirect({
        to: '/studio/settings/$section',
        params: { section: 'general' },
        replace: true,
      })
    }
  },
})
