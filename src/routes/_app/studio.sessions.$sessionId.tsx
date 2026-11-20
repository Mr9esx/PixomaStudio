import { createFileRoute } from '@tanstack/react-router'

export const Route = createFileRoute('/_app/studio/sessions/$sessionId')({
  validateSearch: (search: Record<string, unknown>): { panel?: 'video' | 'assets' | 'tasks'; video?: string; shot?: string; reference?: string } => ({
    panel: search.panel === 'video' || search.panel === 'assets' || search.panel === 'tasks' ? search.panel : undefined,
    video: typeof search.video === 'string' && search.video.trim() ? search.video : undefined,
    shot: typeof search.shot === 'string' && search.shot.trim() ? search.shot : undefined,
    reference: typeof search.reference === 'string' && search.reference.trim() ? search.reference : undefined,
  }),
})
