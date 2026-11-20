import { describe, expect, it } from 'vitest'
import { mediaApiPath, resolveMediaKey } from './media'

describe('本地工作流预览地址', () => {
  it('保留本地资产版本与文件名称', () => {
    const asset = 'pixoma-media://localhost/4da29b65-8ecf-4b41-ba70-4bb6cbaeab39/4a3747a4-c067-4277-af67-656635a68c9a#result.png'
    expect(resolveMediaKey(asset)).toBe(asset)
    expect(mediaApiPath(asset)).toBe(asset)
  })

  it('识别 Windows 的本地资产协议地址', () => {
    const asset = 'http://pixoma-media.localhost/4da29b65-8ecf-4b41-ba70-4bb6cbaeab39/4a3747a4-c067-4277-af67-656635a68c9a#clip.mp4'
    expect(resolveMediaKey(asset)).toBe(asset)
  })

  it('拒绝其他来源与不完整的资产版本地址', () => {
    expect(resolveMediaKey('https://example.com/image.png')).toBeNull()
    expect(resolveMediaKey('pixoma-media://localhost/missing-version')).toBeNull()
    expect(resolveMediaKey('pixoma-media://other/asset/version')).toBeNull()
    expect(() => mediaApiPath('missing')).toThrow()
  })
})
