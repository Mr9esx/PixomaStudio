import { invoke, type InvokeArgs, type InvokeOptions } from '@tauri-apps/api/core'
import { createDesktopRequest } from '@/lib/desktop-request'

export class ApiError extends Error {
  status: number
  /** 业务错误码，7 位整数。网络层失败时为 0。 */
  code: number
  /** 后端给出的原始技术原因，已脱敏，可直接展示。 */
  detail?: string
  constructor(status: number, message: string, code = 0, detail?: string) {
    super(message)
    this.name = 'ApiError'
    this.status = status
    this.code = code
    this.detail = detail
  }
}

export function baseURL(): string {
  return ''
}

export function sessionToken(): string {
  try {
    return sessionStorage.getItem('pixoma_admin_token') || ''
  } catch {
    return ''
  }
}

export function setSessionToken(token: string | null): void {
  try {
    if (token) sessionStorage.setItem('pixoma_admin_token', token)
    else sessionStorage.removeItem('pixoma_admin_token')
  } catch {
    // ignore
  }
}

export async function apiFetch<T>(
  path: string,
  init?: RequestInit
): Promise<T> {
  if (path.startsWith('pixoma-media:') || path.startsWith('http://pixoma-media.localhost/')) {
    const response = await fetch(path)
    if (!response.ok) throw new ApiError(response.status, '读取资产内容失败')
    return await response.text() as T
  }
  const request = createDesktopRequest(path, init)
  return desktopInvoke<T>('studio_request', { request })
}

export function desktopInvoke<T>(command: string, args?: InvokeArgs, options?: InvokeOptions): Promise<T> {
  return invoke<T>(command, args, options).catch((error: unknown) => {
    if (typeof error === 'object' && error !== null && 'status' in error && 'message' in error && typeof error.status === 'number' && typeof error.message === 'string') {
      const code = 'code' in error && typeof error.code === 'number' ? error.code : 0
      throw new ApiError(error.status, error.message, code)
    }
    throw new ApiError(0, String(error))
  })
}

export function toQuery(
  params?: Record<string, string | number | boolean | undefined | null>
): string {
  if (!params) return ''
  const sp = new URLSearchParams()
  for (const [k, v] of Object.entries(params)) {
    if (v === undefined || v === null || v === '') continue
    sp.set(k, String(v))
  }
  const s = sp.toString()
  return s ? `?${s}` : ''
}
