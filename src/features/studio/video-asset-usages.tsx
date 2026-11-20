import { useMutation, useQuery } from '@tanstack/react-query'
import { Button } from '@/components/ui/button'
import { FieldError } from '@/components/ui/field'
import { getAssetVideoUsages, openStudioVideoWork, type StudioVideoWork, type VideoReferenceLocation } from '@/lib/api/video-projects'
import { videoReferenceRoleLabels } from './video-editor-model'

export function VideoAssetUsages({ assetId, onOpen }: { assetId: string; onOpen: (work: StudioVideoWork, location?: VideoReferenceLocation) => Promise<void> }) {
  const usages = useQuery({ queryKey: ['studio', 'asset-video-usages', assetId], queryFn: () => getAssetVideoUsages(assetId) })
  const open = useMutation({ mutationFn: async ({ videoId, location }: { videoId: string; location: VideoReferenceLocation }) => onOpen(await openStudioVideoWork(videoId), location) })
  return <section className='flex flex-col gap-2' aria-label='视频作品中的素材引用'>
    <h2 className='text-sm font-semibold'>视频引用</h2>
    {usages.isPending ? <span className='text-sm text-muted-foreground'>正在读取使用位置</span> : usages.error ? <><FieldError>{usages.error.message}</FieldError><Button size='sm' variant='outline' onClick={() => void usages.refetch()}>重新读取</Button></> : usages.data.length ? usages.data.map(work => <div key={work.video_id} className='flex flex-col gap-2 rounded-md border bg-muted p-3'>
      <span className='text-sm font-medium'>{work.video_name}</span>
      <span className='text-xs text-muted-foreground'>{work.project_name}</span>
      {work.references.map(item => <div key={`${item.shot_id}:${item.reference.id}`} className='flex flex-col gap-1'>
        <span className='text-sm'>{item.shot_name} · {videoReferenceRoleLabels[item.reference.role]} · 版本 {item.version}</span>
        <Button size='sm' variant='outline' disabled={open.isPending} onClick={() => open.mutate({ videoId: work.video_id, location: { shot: item.shot_id, reference: item.reference.id } })}>选择更新范围</Button>
      </div>)}
    </div>) : <p className='text-sm text-muted-foreground'>尚未被分镜引用</p>}
    {open.error ? <FieldError>{open.error.message}</FieldError> : null}
  </section>
}
