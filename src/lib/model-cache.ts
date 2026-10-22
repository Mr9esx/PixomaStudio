import type { QueryClient } from '@tanstack/react-query'

export async function invalidateStudioModels(client: QueryClient): Promise<void> {
  await Promise.all([
    client.invalidateQueries({ queryKey: ['studio-operations', 'models'] }),
    client.invalidateQueries({ queryKey: ['studio', 'models'] }),
    client.invalidateQueries({ queryKey: ['agent-assets', 'studio', 'models'] }),
  ])
}
