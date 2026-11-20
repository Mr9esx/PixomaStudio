import { expect, it } from 'vitest'
import { newStudioAssetReferences, serializeStudioComposer, studioComposerContent } from './studio-composer-content'
import { expertPartSchema } from '@/features/agent-assets/expert-form'

it('工作流引用经过编辑器转换后保留本地版本标识', () => {
  const reference = {
    type: 'workflow_ref' as const,
    workflow_id: '92c97b20-54b3-48c6-90b7-b5ff3c6cca9a',
    version_id: 'b037dfa9-7f69-46a9-8be6-c1417b4acbda',
    name: 'Klein9B 文生图',
  }
  const result = serializeStudioComposer(studioComposerContent([reference]))
  expect(result.parts).toEqual([reference])
  expect(expertPartSchema.parse({ type: 'workflow', workflow_id: reference.workflow_id, version_id: reference.version_id })).toEqual({
    type: 'workflow', workflow_id: reference.workflow_id, version_id: reference.version_id,
  })
})

it('外部资产引用仅插入尚未存在的资产，同一请求去重', () => {
  const first = { assetId: 'asset-a', assetVersionId: 'version-a' }
  const second = { assetId: 'asset-b', assetVersionId: 'version-b' }
  expect(newStudioAssetReferences([first, second, second], [first])).toEqual([second])
  expect(newStudioAssetReferences([first], [])).toEqual([first])
  const parts = [{ type: 'asset_ref' as const, asset_id: second.assetId, asset_version_id: second.assetVersionId, name: '分镜上下文' }]
  expect(serializeStudioComposer(studioComposerContent(parts)).selectedAssets).toEqual([second])
})
