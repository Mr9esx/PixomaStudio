import { describe, expect, it } from 'vitest'
import { createDesktopRequest } from './desktop-request'

describe('本地命令参数', () => {
  it('保留项目名称和查询参数', () => {
    expect(createDesktopRequest('/api/v1/studio/projects', { method: 'POST', body: JSON.stringify({ name: '项目甲' }) }))
      .toEqual({ method: 'POST', path: '/api/v1/studio/projects', body: { name: '项目甲' } })
    expect(createDesktopRequest('/api/v1/studio/sessions?project_id=&limit=30'))
      .toEqual({ method: 'GET', path: '/api/v1/studio/sessions?project_id=&limit=30', body: null })
  })

  it('拒绝外部地址和无法识别的请求内容', () => {
    expect(() => createDesktopRequest('https://example.com/api/v1/studio/projects')).toThrow()
    expect(() => createDesktopRequest('/api/v1/studio/projects', { method: 'POST', body: 'invalid' })).toThrow()
    expect(() => createDesktopRequest('/api/v1/studio/projects', { method: 'POST', body: new FormData() })).toThrow()
  })
})
