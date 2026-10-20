import { expect, it } from 'vitest'
import { readFile } from 'node:fs/promises'
import { createDesktopUpload } from './desktop-upload'

it('通过二进制内容与独立元数据传递文件', async () => {
  const file = new File(['真实正文'], '正文.txt', { lastModified: Date.parse('2026-10-17T10:00:00Z') })
  const upload = await createDesktopUpload(file, undefined, 'project-a', 'request-a')
  expect(new TextDecoder().decode(upload.bytes)).toBe('真实正文')
  expect(JSON.parse(decodeURIComponent(upload.metadata))).toEqual({
    file_name: '正文.txt', project_id: 'project-a', request_id: 'request-a', source_modified_at: '2026-10-17T10:00:00.000Z',
  })
})

it('为头像传递上传用途并检查文件大小', async () => {
  const bytes = await readFile(new URL('../../public/images/pixoma-ghost.png', import.meta.url))
  const file = new File([bytes], '头像.png', { lastModified: 0 })
  const upload = await createDesktopUpload(file, undefined, '', 'avatar-upload', 'expert_avatar')
  expect(JSON.parse(decodeURIComponent(upload.metadata))).toEqual({
    file_name: '头像.png', project_id: '', request_id: 'avatar-upload', purpose: 'expert_avatar',
  })
  expect(new Uint8Array(upload.bytes)).toEqual(new Uint8Array(bytes))
  const oversized = new File([new Uint8Array(5 * 1024 * 1024 + 1)], '头像.png')
  await expect(createDesktopUpload(oversized, undefined, '', 'large-avatar', 'expert_avatar')).rejects.toThrow()
})
