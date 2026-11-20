import { Button } from '@/components/ui/button'

export function VideoMediaError({ message, loading, onRetry }: { message: string; loading?: boolean; onRetry: () => void }) {
  return <div className='flex min-w-0 flex-col items-center gap-2 p-2'>
    <span role='alert' className='break-words text-xs text-muted-foreground'>{message}</span>
    <Button size='sm' variant='outline' disabled={loading} onClick={onRetry}>{loading ? '读取中' : '重新读取'}</Button>
  </div>
}
