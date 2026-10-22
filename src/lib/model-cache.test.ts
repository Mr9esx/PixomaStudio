import { QueryClient } from '@tanstack/react-query'
import { expect, test } from 'vitest'
import { invalidateStudioModels } from './model-cache'

test('保存模型后刷新设置、对话和专家编辑的模型缓存', async () => {
  const client = new QueryClient()
  const modelQueries = [
    ['studio-operations', 'models'],
    ['studio', 'models'],
    ['agent-assets', 'studio', 'models'],
  ]
  for (const queryKey of modelQueries) client.setQueryData(queryKey, [])
  client.setQueryData(['studio', 'projects'], [])

  await invalidateStudioModels(client)

  for (const queryKey of modelQueries) {
    expect(client.getQueryState(queryKey)?.isInvalidated, queryKey.join('/')).toBe(true)
  }
  expect(client.getQueryState(['studio', 'projects'])?.isInvalidated).toBe(false)
})
