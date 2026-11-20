import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Check, RefreshCw, TriangleAlert } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { getComputeNode, type ComputeNode, type ComputeNodeCredential } from '@/lib/api/compute-nodes'
import { cn } from '@/lib/utils'
import { Alert, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Spinner } from '@/components/ui/spinner'
import { PresenceTags } from './presence-tags'
import { DeployCredentials } from './deploy-credentials'
import { EdgeForm } from './edge-form'

export function CreateEdgeWizard({ onDone, onCancel, onCredential }: {
  onDone: (node: ComputeNode) => void
  onCancel: () => void
  onCredential: (credential: ComputeNodeCredential) => void
}) {
  const { t } = useTranslation()
  const [credential, setCredential] = useState<ComputeNodeCredential>()
  const [step, setStep] = useState<'form' | 'deploy'>('form')
  const [checked, setChecked] = useState(false)
  const status = useQuery({ queryKey: ['studio', 'compute-node', credential?.node.id], queryFn: () => getComputeNode(credential!.node.id), enabled: Boolean(credential), refetchInterval: 3000 })
  const node = status.data ?? credential?.node
  const ready = status.isSuccess && status.data.connected && status.data.comfyui_ready
  return <div className='mx-auto flex w-full max-w-2xl flex-col gap-5' data-testid='create-edge-wizard'>
    <ol className='flex shrink-0 items-center justify-center gap-2 py-3' data-testid='create-edge-steps'>
      {[t('edges.stepInfo'), t('edges.stepDeploy')].map((label, index) => <li key={label} className='flex items-center gap-2' aria-current={(step === 'deploy' ? 1 : 0) === index ? 'step' : undefined}>
        <span className={cn('flex size-6 shrink-0 items-center justify-center rounded-full text-xs font-medium', (step === 'deploy' ? 1 : 0) === index ? 'bg-primary text-primary-foreground' : step === 'deploy' && index === 0 ? 'bg-primary/20 text-primary' : 'bg-muted text-muted-foreground')}>{step === 'deploy' && index === 0 ? <Check className='size-3.5' aria-hidden /> : index + 1}</span><span className={cn('truncate text-xs', (step === 'deploy' ? 1 : 0) === index ? 'font-medium text-foreground' : 'text-muted-foreground')}>{label}</span>{index === 0 ? <span aria-hidden className={cn('h-px w-6 shrink-0', step === 'deploy' ? 'bg-primary/40' : 'bg-border')} /> : null}
      </li>)}
    </ol>
    {step === 'form' ? <EdgeForm onCancel={onCancel} onCredential={(next) => { setCredential(next); onCredential(next) }} onSaved={() => setStep('deploy')} /> : <>
      <div className='flex min-w-0 flex-col gap-5' data-testid='edge-deploy-step'>
        <DeployCredentials node={node!} credential={credential} />
        <div className='flex flex-wrap items-center gap-2'>
          <Button type='button' size='sm' variant='outline' disabled={status.isFetching} onClick={() => { setChecked(true); void status.refetch() }}>{status.isFetching ? <Spinner /> : <RefreshCw />}{t('edges.checkNodeStatus')}</Button>
          <PresenceTags edgeOnline={status.isSuccess && node?.connected === true} comfyRunning={status.isSuccess && node?.comfyui_ready === true} />
        </div>
        {status.error ? <Alert variant='warn'><TriangleAlert /><AlertTitle>{status.error.message}</AlertTitle></Alert> : checked && !ready && !status.isFetching ? <Alert variant='warn'><TriangleAlert /><AlertTitle>{node?.comfyui_error || t('edges.checkStatusFailed')}</AlertTitle></Alert> : null}
      </div>
      <div className='flex justify-end gap-2'><Button type='button' size='sm' variant='ghost' onClick={() => onDone(node!)}>{t('edges.deploySkip')}</Button><Button type='button' size='sm' disabled={!ready} onClick={() => onDone(node!)}>{t('edges.deployContinue')}</Button></div>
    </>}
  </div>
}
