import { createFileRoute } from '@tanstack/react-router'

export const Route = createFileRoute('/_app/studio/experts')({
  validateSearch: (search: Record<string, unknown>): { q?: string } => ({ q: typeof search.q === 'string' ? search.q : undefined }),
})
