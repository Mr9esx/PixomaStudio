import { createFileRoute } from '@tanstack/react-router'

export const Route = createFileRoute('/_app/studio/workflows')({
  validateSearch: (search: Record<string, unknown>): { q?: string; category?: string; type?: 'comfyui' | 'online'; edit?: boolean; section?: 'info' | 'workflow' } => ({
    q: typeof search.q === 'string' ? search.q : undefined,
    category: typeof search.category === 'string' ? search.category : undefined,
    type: search.type === 'comfyui' || search.type === 'online' ? search.type : undefined,
    edit: search.edit === true || search.edit === 'true' ? true : undefined,
    section: search.section === 'info' || search.section === 'workflow' ? search.section : undefined,
  }),
})
