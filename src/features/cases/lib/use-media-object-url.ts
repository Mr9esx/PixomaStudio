import { useEffect, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { fetchMediaBlob, resolveMediaKey } from '@/lib/api/media'

/** 读取本地资产版本并在组件卸载时释放预览地址。 */
export function useMediaResource(value?: string) {
  const key = resolveMediaKey(value)
  const [loaded, setLoaded] = useState<
    { key: string; data: Blob; url: string } | undefined
  >(undefined)
  const media = useQuery({
    queryKey: ['studio', 'media-preview', key],
    queryFn: () => fetchMediaBlob(key!),
    enabled: Boolean(key),
    retry: false,
  })
  useEffect(() => {
    if (!key || !media.data) { setLoaded(undefined); return }
    const objectUrl = URL.createObjectURL(media.data)
    setLoaded({ key, data: media.data, url: objectUrl })
    return () => URL.revokeObjectURL(objectUrl)
  }, [key, media.data])
  return { src: loaded && loaded.key === key && loaded.data === media.data ? loaded.url : undefined, error: media.error, refetch: media.refetch, isFetching: media.isFetching }
}

export function useMediaObjectUrl(value?: string): string | undefined {
  const media = useMediaResource(value)
  if (media.error) throw media.error
  return media.src
}
