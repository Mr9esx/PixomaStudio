import type { ReactNode } from 'react'
import { cn } from '@/lib/utils'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { ChartContainer, type ChartConfig } from '@/components/ui/chart'
import { Skeleton } from '@/components/ui/skeleton'

export type MonitorStat = { label: string; value: string }

export function MonitorCard({ title, config, stats, compact = false, children }: {
  title: string; config: ChartConfig; stats: MonitorStat[]; compact?: boolean; children: ReactNode
}) {
  return <Card className='min-w-0 flex-1 gap-3 py-4'>
    <CardHeader className='gap-3 px-4'>
      <CardTitle className='text-base font-semibold'>{title}</CardTitle>
      {Object.keys(config).length > 0 ? <CardDescription className='flex flex-wrap items-center gap-4 text-xs text-muted-foreground'>
        {Object.entries(config).map(([key, entry]) => <span key={key} className='inline-flex items-center gap-1.5'><span className='size-2 rounded-full' style={{ backgroundColor: entry.color }} />{entry.label}</span>)}
      </CardDescription> : null}
    </CardHeader>
    <CardContent className={cn('grid min-h-0 flex-1 gap-1.5 px-4', stats.length === 0 ? 'grid-cols-1' : compact ? 'grid-cols-1 lg:grid-cols-[minmax(0,1fr)_122px]' : 'grid-cols-1 lg:grid-cols-[minmax(0,1fr)_72px]')}>
      <div className='h-full min-h-[150px] w-full min-w-0'><ChartContainer config={config} className='aspect-auto h-full w-full'>{children}</ChartContainer></div>
      {stats.length > 0 ? <div className={cn('grid content-center gap-2 text-center', compact ? 'grid-cols-2 gap-1.5 lg:text-right' : 'grid-cols-3 lg:grid-cols-1 lg:text-right')}>
        {stats.map(stat => <div key={stat.label}><p className={cn('font-semibold tabular-nums', compact ? 'text-xs leading-5' : 'text-lg leading-6')}>{stat.value}</p><p className='text-xs text-muted-foreground'>{stat.label}</p></div>)}
      </div> : null}
    </CardContent>
  </Card>
}

export function MonitorCardSkeleton() {
  return <Card className='min-w-0 flex-1 gap-3 py-4'><CardHeader className='gap-3 px-4'><Skeleton className='h-4 w-28' /><Skeleton className='h-3 w-44' /></CardHeader><CardContent className='px-4'><Skeleton className='h-40 w-full' /></CardContent></Card>
}
