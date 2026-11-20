import { useQuery } from '@tanstack/react-query'
import { listVideoWorkExports } from '@/lib/api/video-work-exports'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { FieldError } from '@/components/ui/field'
import { Skeleton } from '@/components/ui/skeleton'
import { Empty, EmptyHeader, EmptyTitle } from '@/components/ui/empty'
import { videoExportPending, videoExportPhase, type VideoExportTask } from './video-export-model'

export function VideoExportHistory({ videoId, onClose, onOpen, onSource }: { videoId: string; onClose: () => void; onOpen: (task: VideoExportTask) => void; onSource: (task: VideoExportTask) => void }) {
  const history = useQuery({ queryKey: ['studio', 'video-export-history', videoId], queryFn: () => listVideoWorkExports(videoId), refetchInterval: query => query.state.data?.some(videoExportPending) ? 2000 : false })
  return <Dialog open onOpenChange={open => { if (!open) onClose() }}><DialogContent aria-describedby={undefined} className='max-h-[85vh] overflow-y-auto shadow-lg'>
    <DialogHeader><DialogTitle>导出记录</DialogTitle></DialogHeader>
    {history.error ? <><FieldError>{history.error.message}</FieldError><Button variant='outline' onClick={() => void history.refetch()}>重新读取</Button></> : history.isPending ? <Skeleton className='h-24 w-full' /> : !history.data.length ? <Empty><EmptyHeader><EmptyTitle>暂无导出记录</EmptyTitle></EmptyHeader></Empty> : history.data.map(task => <div key={task.request.request_id} className='flex flex-col gap-2 rounded-lg border p-3'>
      <p className='text-sm font-medium'>{task.request.name}</p><p className='text-xs text-muted-foreground'>{videoExportPhase[task.status]} · {task.request.width}×{task.request.height} · {task.request.fps} FPS{task.sourceVersion !== undefined ? ` · 来源修订 ${task.sourceVersion}` : ''}</p>
      {task.error ? <FieldError>{task.error}</FieldError> : null}
      <div className='flex flex-wrap gap-2'><Button size='sm' variant='outline' onClick={() => onOpen(task)}>查看导出</Button><Button size='sm' variant='ghost' onClick={() => onSource(task)}>查看来源作品</Button></div>
    </div>)}
  </DialogContent></Dialog>
}
