import { createFileRoute } from '@tanstack/react-router'

export const Route = createFileRoute('/_app/studio/workflows')({
  validateSearch: (search: Record<string, unknown>): { q?: string; category?: string } => ({
    q: typeof search.q === 'string' ? search.q : undefined,
    category: typeof search.category === 'string' ? search.category : undefined,
  }),
})
