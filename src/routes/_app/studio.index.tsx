import { createFileRoute, redirect } from '@tanstack/react-router'

type LegacyStudioSearch = {
  project?: string
  workflow?: string
  example?: number
  view?: 'library' | 'settings'
  session?: string
  section?: 'models' | 'skills' | 'mcp' | 'workflows' | 'data' | 'sessions'
  trace?: boolean
  panel?: 'assets' | 'tasks'
}

export const Route = createFileRoute('/_app/studio/')({
  validateSearch: (search: Record<string, unknown>): LegacyStudioSearch => ({
    project: typeof search.project === 'string' ? search.project : undefined,
    workflow: typeof search.workflow === 'string' && /^[1-9]\d*$/.test(search.workflow) ? search.workflow : undefined,
    example: typeof search.example === 'number' && Number.isInteger(search.example) && search.example >= 0 ? search.example : undefined,
    view:
      search.view === 'library' || search.view === 'settings'
        ? search.view
        : undefined,
    session: typeof search.session === 'string' ? search.session : undefined,
    section:
      search.section === 'models' ||
      search.section === 'skills' ||
      search.section === 'mcp' ||
      search.section === 'workflows' ||
      search.section === 'data' ||
      search.section === 'sessions'
        ? search.section
        : undefined,
    trace: search.trace === true || search.trace === 'true' ? true : undefined,
    panel: search.panel === 'assets' || search.panel === 'tasks' ? search.panel : undefined,
  }),
  beforeLoad: ({ search }) => {
    if (search.view === 'library') {
      throw redirect({ to: '/studio/library', replace: true })
    }
    if (search.view === 'settings') {
      if (search.section === 'workflows') {
        throw redirect({ to: '/studio/workflows', replace: true })
      }
      if (
        search.section !== 'models' &&
        search.section !== 'skills' &&
        search.section !== 'mcp'
      ) {
        throw redirect({
          to: '/studio/settings/$section',
          params: { section: 'general' },
          replace: true,
        })
      }
      const section =
        search.section === 'skills' ||
        search.section === 'mcp'
          ? search.section
          : 'models'
      throw redirect({
        to: '/studio/settings/$section',
        params: { section },
        replace: true,
      })
    }
    if (typeof search.session === 'string' && search.session.trim()) {
      if (search.trace) {
        throw redirect({
          to: '/studio/sessions/$sessionId/trace',
          params: { sessionId: search.session },
          search: {},
          replace: true,
        })
      }
      throw redirect({
        to: '/studio/sessions/$sessionId',
        params: { sessionId: search.session },
        search: search.panel ? { panel: search.panel } : {},
        replace: true,
      })
    }
  },
})
