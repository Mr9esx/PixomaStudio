import { CalendarCheck, Cpu, Gpu, Hash, MemoryStick, MoreHorizontal, Package, PenLine, Tags, Terminal, Timer, Trash2, TriangleAlert } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import type { ComputeNode } from '@/lib/api/compute-nodes'
import { formatDateTime } from '@/lib/format'
import { Alert, AlertTitle } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { DropdownMenu, DropdownMenuContent, DropdownMenuGroup, DropdownMenuItem, DropdownMenuTrigger } from '@/components/ui/dropdown-menu'
import { LongText } from '@/components/long-text'
import { MetaChip } from '@/components/meta-chip'
import { StatusDot } from '@/components/status-dot'
import { kit } from './kit-classes'
import { formatBytes } from './observation'
import { ObservationPanel } from './observation-panel'

function DetailStatusTag({ on, label }: { on: boolean; label: string }) {
  return <Badge variant='secondary' className={`gap-2 rounded-full px-3 py-1.5 font-normal ${on ? 'border-success/25 bg-success/10 text-success' : 'border-border bg-muted text-muted-foreground'}`}><StatusDot label={label} problems={on ? 0 : 1} />{label}</Badge>
}

export function EdgeDetailPanel({ node, error, onEdit, onDeploy, onDelete, onRotate }: {
  node: ComputeNode; error?: Error | null; onEdit: () => void; onDeploy: () => void; onDelete: () => void; onRotate: () => void
}) {
  const { t } = useTranslation()
  const hardware = node.hardware
  const gpus = hardware?.gpus?.filter(gpu => gpu.name.trim()) ?? []
  const healthy = !error && node.healthy
  return <section id='edge-detail' className={kit.pageSection} data-testid='edge-detail'>
    <div className='flex min-w-0 flex-col gap-[6px]'>
      <div className='flex flex-wrap items-center justify-between gap-3'>
        <div className='flex min-w-0 flex-wrap items-center gap-2'>
          <h2 className={kit.title}>{node.name || node.id}</h2>
          <DetailStatusTag on={node.enabled} label={t(node.enabled ? 'edges.enabled' : 'edges.disabled')} />
          <DetailStatusTag on={node.connected} label={t(node.connected ? 'edges.nodeOnline' : 'edges.nodeOffline')} />
          <DetailStatusTag on={node.comfyui_ready} label={t(node.comfyui_ready ? 'edges.comfyRunning' : 'edges.comfyStopped')} />
        </div>
        <div className='flex shrink-0 flex-wrap gap-2'>
          <Button type='button' size='sm' onClick={onEdit}><PenLine className='size-3.5' strokeWidth={2} />{t('edges.edit')}</Button>
          <Button type='button' variant='outline' size='sm' onClick={onDeploy}><Terminal className='size-3.5' strokeWidth={2} />{t('edges.deployCommand')}</Button>
          <DropdownMenu><DropdownMenuTrigger asChild><Button type='button' variant='outline' size='icon-sm' aria-label={t('common.moreActions')}><MoreHorizontal className='size-3.5' strokeWidth={2} /></Button></DropdownMenuTrigger><DropdownMenuContent align='end'><DropdownMenuGroup><DropdownMenuItem onSelect={onRotate}>重新生成凭据</DropdownMenuItem><DropdownMenuItem variant='destructive' onSelect={onDelete}><Trash2 className='size-3.5' strokeWidth={2} />{t('common.delete')}</DropdownMenuItem></DropdownMenuGroup></DropdownMenuContent></DropdownMenu>
        </div>
      </div>
      {node.description ? <LongText className='max-w-full text-sm text-muted-foreground'>{node.description}</LongText> : null}
      <div className='mt-1 flex max-w-full flex-wrap items-center gap-2 text-xs'>
        <MetaChip icon={<Hash className='size-3.5' strokeWidth={2} />} label={t('edges.fieldNodeId')} value={node.id} divider />
        <MetaChip icon={<CalendarCheck className='size-3.5' strokeWidth={2} />} label={t('edges.fieldCreatedAt')} value={formatDateTime(node.created_at)} divider />
        <MetaChip icon={<Timer className='size-3.5' strokeWidth={2} />} label={t('edges.fieldStartedAt')} value={node.enabled && node.connected ? formatDateTime(node.started_at) : '—'} divider />
        <MetaChip icon={<Package className='size-3.5' strokeWidth={2} />} label={t('edges.fieldComfyVersion')} value={node.comfy_version} divider />
        <MetaChip icon={<Tags className='size-3.5' strokeWidth={2} />} label={t('edges.fieldCapabilities')} value={node.capabilities.join(', ')} />
      </div>
    </div>
    {!healthy ? <Alert variant='warn'><TriangleAlert /><AlertTitle>{error?.message ?? node.issues.join('；')}</AlertTitle>{node.comfyui_error ? <p className='text-sm'>{node.comfyui_error}</p> : null}</Alert> : null}
    <section className={kit.specsWrap}>
      <div className={kit.specsCell}><div className={kit.specsLabel}><Cpu className='size-3.5 shrink-0' strokeWidth={2} />{t('edges.fieldCpu')}</div><div className='mt-2 flex min-w-0 flex-col gap-1 xl:flex-row xl:items-end xl:justify-between xl:gap-3'><LongText className={kit.specsValue}>{hardware?.cpu_model || '—'}</LongText></div></div>
      <div className={kit.specsCell}><div className={kit.specsLabel}><Gpu className='size-3.5 shrink-0' strokeWidth={2} />{t('edges.fieldGpu')}</div>{gpus.length ? <div className='mt-2 flex min-w-0 flex-col gap-2'>{gpus.map((gpu,index) => <div key={`${gpu.name}-${index}`} className='flex min-w-0 flex-col gap-1 xl:flex-row xl:items-end xl:justify-between xl:gap-3'><LongText className={kit.specsValue}>{gpu.name}</LongText><p className={kit.specsNote}>{gpu.vram_bytes ? formatBytes(gpu.vram_bytes) : '—'}</p></div>)}</div> : <div className='mt-2 flex min-w-0 flex-col gap-1 xl:flex-row xl:items-end xl:justify-between xl:gap-3'><LongText className={kit.specsValue}>—</LongText></div>}</div>
      <div className={kit.specsCell}><div className={kit.specsLabel}><Cpu className='size-3.5 shrink-0' strokeWidth={2} />{t('edges.fieldCpuCores')}</div><div className='mt-2 flex min-w-0 flex-col gap-1 xl:flex-row xl:items-end xl:justify-between xl:gap-3'><LongText className={kit.specsValue}>{hardware?.cpu_cores ? t('edges.coresValue', {count:hardware.cpu_cores}) : '—'}</LongText></div></div>
      <div className={kit.specsCell}><div className={kit.specsLabel}><MemoryStick className='size-3.5 shrink-0' strokeWidth={2} />{t('edges.fieldMemory')}</div><div className='mt-2 flex min-w-0 flex-col gap-1 xl:flex-row xl:items-end xl:justify-between xl:gap-3'><LongText className={kit.specsValue}>{hardware?.ram_bytes ? formatBytes(hardware.ram_bytes) : '—'}</LongText></div></div>
    </section>
    <ObservationPanel key={node.id} id={node.id} />
  </section>
}
