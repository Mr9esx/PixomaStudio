export type ImportedMedia = {
  id: string
  name: string
  kind: 'image' | 'video' | 'audio'
  src: string
  duration?: number
  firstFrame?: string
  lastFrame?: string
  width?: number
  height?: number
}

export async function importMedia(file: File): Promise<ImportedMedia> {
  if (file.size > 24 * 1024 * 1024) throw new Error('演示文件最大为 24 MiB')
  const kind = file.type.split('/')[0]
  if (kind !== 'image' && kind !== 'video' && kind !== 'audio') throw new Error('选择图片、视频或音频文件')
  const src = await new Promise<string>((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => resolve(String(reader.result))
    reader.onerror = () => reject(new Error('读取文件失败'))
    reader.readAsDataURL(file)
  })
  const item: ImportedMedia = { id: crypto.randomUUID(), name: file.name, kind, src }
  if (kind === 'image') {
    const image = new Image()
    image.src = src
    await image.decode()
    return { ...item, width: image.naturalWidth, height: image.naturalHeight }
  }
  const element = document.createElement(kind)
  element.preload = 'auto'
  element.src = src
  await new Promise<void>((resolve, reject) => {
    element.onloadedmetadata = () => resolve()
    element.onerror = () => reject(new Error('媒体格式无法解码'))
  })
  if (!Number.isFinite(element.duration) || element.duration <= 0) throw new Error('媒体时长无效')
  item.duration = Math.max(1, Math.floor(element.duration * 30))
  if (element instanceof HTMLVideoElement) {
    item.width = element.videoWidth
    item.height = element.videoHeight
    const canvas = document.createElement('canvas')
    canvas.width = element.videoWidth
    canvas.height = element.videoHeight
    const context = canvas.getContext('2d')
    if (!context) throw new Error('无法提取视频画面')
    const readFrame = async (time: number) => {
      await new Promise<void>((resolve, reject) => {
        element.onseeked = () => resolve()
        element.onerror = () => reject(new Error('提取视频画面失败'))
        element.currentTime = time
      })
      context.drawImage(element, 0, 0)
      return canvas.toDataURL('image/jpeg', 0.88)
    }
    item.firstFrame = await readFrame(Math.min(0.001, element.duration / 2))
    item.lastFrame = await readFrame(Math.max(0, element.duration - 0.000001))
  }
  element.removeAttribute('src')
  element.load()
  return item
}

export function downloadJson(value: unknown, name: string) {
  const url = URL.createObjectURL(new Blob([JSON.stringify(value, null, 2)], { type: 'application/json' }))
  const link = document.createElement('a')
  link.href = url
  link.download = name
  link.click()
  setTimeout(() => URL.revokeObjectURL(url), 1000)
}

export async function extractVideoFrame(src: string, time: number): Promise<string> {
  const video = document.createElement('video')
  video.preload = 'auto'
  video.src = src
  await new Promise<void>((resolve, reject) => {
    video.onloadedmetadata = () => resolve()
    video.onerror = () => reject(new Error('视频无法解码'))
  })
  await new Promise<void>((resolve, reject) => {
    video.onseeked = () => resolve()
    video.onerror = () => reject(new Error('读取片段尾帧失败'))
    video.currentTime = Math.min(video.duration - 0.000001, Math.max(0.000001, time))
  })
  const canvas = document.createElement('canvas')
  canvas.width = video.videoWidth
  canvas.height = video.videoHeight
  const context = canvas.getContext('2d')
  if (!context) throw new Error('无法提取视频画面')
  context.drawImage(video, 0, 0)
  const frame = canvas.toDataURL('image/jpeg', 0.88)
  video.removeAttribute('src')
  video.load()
  return frame
}
