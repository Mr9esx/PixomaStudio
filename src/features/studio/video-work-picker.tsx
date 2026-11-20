import { useState } from 'react'
import { useMutation, useQuery } from '@tanstack/react-query'
import { Video } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { Skeleton } from '@/components/ui/skeleton'
import { listStudioVideoWorks, openStudioVideoWork, type StudioVideoWork } from '@/lib/api/video-projects'

export function VideoWorkPicker({ projectId, onOpen }: { projectId: string; onOpen: (work: StudioVideoWork) => Promise<void> }) {
  const [open, setOpen] = useState(false)
  const [search, setSearch] = useState('')
  const works = useQuery({ queryKey: ['studio', 'video-works', projectId], queryFn: () => listStudioVideoWorks(projectId), enabled: open })
  const select = useMutation({
    mutationFn: async (id: string) => onOpen(await openStudioVideoWork(id)),
    onSuccess: () => setOpen(false),
  })
  const filtered = works.data?.filter(work => work.name.toLocaleLowerCase().includes(search.trim().toLocaleLowerCase()))
  return <Popover open={open} onOpenChange={setOpen}>
    <PopoverTrigger asChild><Button variant='outline' size='sm'><Video />视频作品</Button></PopoverTrigger>
    <PopoverContent align='end' className='flex w-80 flex-col gap-3'>
      <Input aria-label='搜索视频作品' placeholder='搜索视频作品' value={search} onChange={event => setSearch(event.target.value)} />
      {works.isPending ? <Skeleton className='h-16' /> : works.isError ? <div className='flex flex-col gap-2'><p role='alert' className='text-sm'>{works.error.message}</p><Button variant='outline' onClick={() => void works.refetch()}>重试读取作品</Button></div> : <div className='flex max-h-80 flex-col gap-1 overflow-y-auto'>
        {filtered?.map(work => <Button key={work.id} variant='ghost' className='h-auto justify-start py-2 text-left' disabled={select.isPending} onClick={() => select.mutate(work.id)}>
          <Video className='shrink-0' /><span className='flex min-w-0 flex-col gap-1'><span className='truncate'>{work.name}</span><span className='text-xs font-normal text-muted-foreground'>{work.shot_count} 个分镜 · 修订 {work.asset_version}</span></span>
        </Button>)}
        {!filtered?.length ? <p className='py-3 text-sm text-muted-foreground'>{search ? '没有匹配的作品' : '暂无视频作品'}</p> : null}
      </div>}
      {select.isError ? <p role='alert' className='text-sm'>{select.error.message}</p> : null}
    </PopoverContent>
  </Popover>
}
