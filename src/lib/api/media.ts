import { ApiError } from './client'
import { uploadStudioAsset } from './studio'

type UploadedMedia = {
  key: string
  url: string
  mime: string
  size: number
}

/** 图片 / 视频白名单（与服务端一致）。 */
const MEDIA_MIME_WHITELIST: Record<string, string> = {
  'image/png': '.png',
  'image/jpeg': '.jpg',
  'image/webp': '.webp',
  'image/gif': '.gif',
  'video/mp4': '.mp4',
  'video/webm': '.webm',
}

/** 工作流封面与示例媒体的大小上限。 */
export const DEFAULT_MEDIA_MAX_BYTES = 25 * 1024 * 1024

export function resolveMediaMaxBytes(value: number | undefined): number {
  return typeof value === 'number' && value > 0
    ? value
    : DEFAULT_MEDIA_MAX_BYTES
}

/** 上传媒体并保存本地资产版本地址。 */
export async function uploadMedia(file: File): Promise<UploadedMedia> {
  if (!isAllowedMedia(file.type, file.name) || file.size > DEFAULT_MEDIA_MAX_BYTES) {
    throw new ApiError(400, '媒体类型无效或文件超过 25 MiB')
  }
  const asset = await uploadStudioAsset(file)
  const version = asset.versions.find((item) => item.version === asset.current_version)
  if (!version || !['image', 'video'].includes(asset.kind)) throw new ApiError(400, '工作流预览需要图片或视频')
  const address = `${version.content_url}#${encodeURIComponent(file.name)}`
  return { key: address, url: address, mime: version.mime_type, size: version.size_bytes }
}

export function mediaApiPath(key: string): string {
  const address = resolveMediaKey(key)
  if (!address) throw new ApiError(400, '本地媒体地址无效')
  return address
}

export async function fetchMediaBlob(key: string): Promise<Blob> {
  const res = await fetch(mediaApiPath(key))
  if (!res.ok) throw new ApiError(res.status, `加载媒体失败 (${res.status})`)
  return res.blob()
}

export function resolveMediaKey(preview?: string): string | null {
  if (!preview?.trim()) return null
  let address: URL
  try { address = new URL(preview.trim()) } catch { return null }
  const local = address.protocol === 'pixoma-media:' && address.hostname === 'localhost'
  const windows = address.protocol === 'http:' && address.hostname === 'pixoma-media.localhost'
  const parts = address.pathname.split('/')
  if ((!local && !windows) || address.port || address.username || address.password || address.search
    || parts.length !== 3 || !parts[1] || !parts[2]) return null
  return address.toString()
}

/** 客户端预检：类型是否在白名单内。 */
export function isAllowedMedia(mime: string, filename: string): boolean {
  const ext = filename.split('.').pop()?.toLowerCase() ?? ''
  return (
    MEDIA_MIME_WHITELIST[mime.toLowerCase()] === `.${ext}` ||
    Object.entries(MEDIA_MIME_WHITELIST).some(
      ([m, e]) => m === mime.toLowerCase() || e === `.${ext}`
    )
  )
}
