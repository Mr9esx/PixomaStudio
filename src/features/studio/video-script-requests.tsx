import { useQuery } from '@tanstack/react-query'
import { MessageCircle } from 'lucide-react'
import { getStudioRun } from '@/lib/api/studio'
import { Button } from '@/components/ui/button'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { videoScriptRequestStatus, type VideoScriptRequest } from './video-script-request'

function ScriptRequest({ request, onLocate }: { request: VideoScriptRequest; onLocate: (messageId?: string, clientMessageId?: string) => void }) {
  const run = useQuery({
    queryKey: ['studio', 'run', request.runId],
    queryFn: () => getStudioRun(request.runId!),
    enabled: Boolean(request.runId),
    refetchInterval: query => query.state.data && ['succeeded', 'failed', 'cancelled'].includes(query.state.data.status) ? false : 1500,
  })
  const error = run.error?.message || (run.data ? run.data.error_message : request.error)
  return <li className='flex flex-col gap-1 border-b py-2 last:border-b-0'>
    <div className='flex items-center gap-2'>
      <span className='min-w-0 flex-1 truncate text-sm'>{request.label}</span>
      <Button size='sm' variant='ghost' onClick={() => onLocate(run.data?.trigger_message_id ?? request.messageId, request.clientMessageId)}>查看对话</Button>
    </div>
    <span className='text-xs text-muted-foreground' role='status'>{videoScriptRequestStatus(request, run.data)}</span>
    {error ? <p role='alert' className='text-xs'>{error}</p> : null}
    {run.isError ? <Button size='sm' variant='outline' onClick={() => void run.refetch()}>重试读取进度</Button> : null}
  </li>
}

export function VideoScriptRequests({ requests, onLocate }: { requests: VideoScriptRequest[]; onLocate: (messageId?: string, clientMessageId?: string) => void }) {
  if (!requests.length) return null
  return <Popover><PopoverTrigger asChild><Button size='sm' variant='ghost'><MessageCircle data-icon='inline-start' />剧本请求 {requests.length}</Button></PopoverTrigger>
    <PopoverContent align='end' aria-label='剧本请求' className='max-h-80 w-80 overflow-y-auto shadow-lg'>
      <ul className='flex flex-col'>{[...requests].reverse().map(request => <ScriptRequest key={request.id} request={request} onLocate={onLocate} />)}</ul>
    </PopoverContent>
  </Popover>
}
