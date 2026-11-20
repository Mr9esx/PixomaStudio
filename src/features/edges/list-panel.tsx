import { useMemo, useState } from 'react'
import type { ComputeNode } from '@/lib/api/compute-nodes'
import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import { Empty, EmptyHeader, EmptyMedia, EmptyTitle } from '@/components/ui/empty'
import { Input } from '@/components/ui/input'
import { Skeleton } from '@/components/ui/skeleton'
import { StatusDot } from '@/components/status-dot'
import { Server } from 'lucide-react'

export function EdgeListPanel({ items, selectedId, isLoading, error, onRetry, onSelect }: {
  items: ComputeNode[]; selectedId?: string; isLoading?: boolean; error?: Error | null
  onRetry: () => void; onSelect: (id: string) => void
}) {
  const [q, setQ] = useState('')
  const filtered = useMemo(() => items.filter((item) => item.name.toLocaleLowerCase().includes(q.trim().toLocaleLowerCase())), [items, q])
  return <div id='edge-list' className='flex h-full min-h-0 flex-col' data-testid='edges-list-panel'>
    <div className='flex flex-col gap-2 border-b px-4 py-3'><Input aria-label='搜索计算节点' placeholder='搜索计算节点' value={q} onChange={(event) => setQ(event.target.value)} autoComplete='off' /></div>
    {error ? <div className='flex flex-col gap-2 p-4'><p role='alert' className='text-sm text-destructive'>{error.message}</p><Button size='sm' variant='outline' onClick={onRetry}>重试</Button></div> : null}
    {isLoading ? <div className='flex flex-col gap-2 p-4'>{[0, 1, 2].map((item) => <Skeleton key={item} className='h-14' />)}</div> : null}
    {!isLoading && !error && !filtered.length ? <Empty><EmptyHeader><EmptyMedia variant='icon'><Server /></EmptyMedia><EmptyTitle>暂无计算节点</EmptyTitle></EmptyHeader></Empty> : null}
    {!isLoading && !error && filtered.length ? <ul className='min-h-0 flex-1 divide-y overflow-auto'>{filtered.map((item) => {
      const vram = (item.hardware?.gpus ?? []).reduce((sum, gpu) => sum + (gpu.vram_bytes ?? 0), 0)
      const specs = [item.hardware?.cpu_cores ? `${item.hardware.cpu_cores} 核心` : '', vram ? `${(vram / 2 ** 30).toFixed(1)} GiB` : ''].filter(Boolean).join(' · ')
      return <li key={item.id}><button type='button' aria-current={selectedId === item.id ? 'true' : undefined} onClick={() => onSelect(item.id)}
        className={cn('block w-full border-l-2 border-l-transparent px-4 py-3 text-left text-sm outline-none hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring', selectedId === item.id && 'border-l-foreground bg-accent')}>
        <div className='flex items-center justify-between gap-2'><span className='truncate font-medium'>{item.name}</span><StatusDot problems={item.healthy ? 0 : 1} label={item.healthy ? '节点就绪' : item.issues.join('；')} /></div>
        {specs ? <p className='mt-0.5 truncate text-xs text-muted-foreground'>{specs}</p> : null}
      </button></li>
    })}</ul> : null}
  </div>
}
