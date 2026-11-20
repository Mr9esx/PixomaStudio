import { useTranslation } from 'react-i18next'
import { Badge } from '@/components/ui/badge'
import { StatusDot } from '@/components/status-dot'

export function PresenceTags({ edgeOnline, comfyRunning }: { edgeOnline: boolean; comfyRunning: boolean }) {
  const { t } = useTranslation()
  return <div className='flex items-center gap-1'>{[{ on: edgeOnline, label: t(edgeOnline ? 'edges.nodeOnline' : 'edges.nodeOffline') }, { on: comfyRunning, label: t(comfyRunning ? 'edges.comfyRunning' : 'edges.comfyStopped') }].map(status => <Badge key={status.label} variant='outline' className={status.on ? 'border-success/25 bg-success/10 text-success' : 'border-border bg-muted text-muted-foreground'}><StatusDot label={status.label} problems={status.on ? 0 : 1} />{status.label}</Badge>)}</div>
}
