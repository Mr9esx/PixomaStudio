export type DesktopRequest = { method: string; path: string; body: unknown }

export function createDesktopRequest(path: string, init?: RequestInit): DesktopRequest {
  if (!path.startsWith('/api/v1/')) throw new Error('本地请求地址无效')
  const url = new URL(path, 'http://studio.localhost')
  if (url.origin !== 'http://studio.localhost') throw new Error('本地请求地址超出允许范围')
  if (init?.body != null && typeof init.body !== 'string') throw new Error('请求内容需要使用 JSON')
  return { method: init?.method ?? 'GET', path: url.pathname + url.search, body: init?.body ? JSON.parse(init.body) : null }
}
