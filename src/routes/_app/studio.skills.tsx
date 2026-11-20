import { createFileRoute } from '@tanstack/react-router'

export const Route = createFileRoute('/_app/studio/skills')({
  validateSearch: (search: Record<string, unknown>): { q?: string; edit?: string } => ({ q: typeof search.q === 'string' ? search.q : undefined, edit: typeof search.edit === 'string' ? search.edit : undefined }),
})
