import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Copy, TriangleAlert } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { getComputeConnection, getComputeNodeCredential, type ComputeNode, type ComputeNodeCredential } from '@/lib/api/compute-nodes'
import { Alert, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Field, FieldGroup, FieldLabel } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { Skeleton } from '@/components/ui/skeleton'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { SecretInput } from '@/components/secret-input'
import { ErrorBanner } from '@/components/feedback/error-banner'
import { edgeDeployCommand, type AgentShell } from './deploy-command'
import { LocalAgentControls } from './local-agent-controls'
import { ComputeConnectionSettings } from './connection-settings'

export function DeployCredentials({ node, credential, onRegenerate }: {
  node: ComputeNode
  credential?: ComputeNodeCredential
  onRegenerate?: () => void
}) {
  const { t } = useTranslation()
  const [shell, setShell] = useState<AgentShell>('bash')
  const [remoteURL, setRemoteURL] = useState<string>()
  const [editingConnection, setEditingConnection] = useState(false)
  const [copyError, setCopyError] = useState<string>()
  const connection = useQuery({ queryKey: ['studio', 'compute-connection'], queryFn: getComputeConnection })
  const savedCredential = useQuery({ queryKey: ['studio', 'node-credential', node.id, node.configuration_revision], queryFn: () => getComputeNodeCredential(node.id), enabled: !credential })
  if (connection.isPending) return <Skeleton className='h-48 w-full' />
  if (connection.error) return <ErrorBanner message={connection.error.message} onRetry={() => void connection.refetch()} />
  const url = node.location === 'local' ? connection.data.local_url : (remoteURL ?? connection.data.public_url).trim()
  const checkedURL = URL.canParse(url) ? new URL(url) : undefined
  const validURL = Boolean(checkedURL && ['http:', 'https:'].includes(checkedURL.protocol) && !checkedURL.username && !checkedURL.password && !checkedURL.search && !checkedURL.hash)
  const remoteBlocked = node.location === 'remote' && !connection.data.remote_access
  const token = credential?.agent_token ?? savedCredential.data?.agent_token ?? ''
  const commandInput = { controlPlaneURL: url, token, edgeId: node.id, shell }
  const copy = async (value: string) => {
    setCopyError(undefined)
    try {
      await navigator.clipboard.writeText(value)
      toast.success(t('edges.copied'))
    } catch (error) {
      setCopyError(error instanceof Error ? error.message : '复制失败')
    }
  }
  return <section className='flex min-w-0 flex-col gap-5' data-testid='deploy-credentials'>
    <FieldGroup>
      <Field><FieldLabel htmlFor='edge-control-plane'>Studio 地址</FieldLabel>
        <Input id='edge-control-plane' type='url' value={url} readOnly={node.location === 'local'} onChange={(event) => setRemoteURL(event.target.value)} placeholder='http://192.168.1.10:18083' aria-invalid={Boolean(url) && !validURL} />
      </Field>
    </FieldGroup>
    <div className='flex flex-wrap gap-2'><Button type='button' size='sm' variant='ghost' onClick={() => setEditingConnection(value => !value)}>连接设置</Button><LocalAgentControls node={node} /></div>
    {editingConnection ? <div className='rounded-lg border bg-muted p-4'><ComputeConnectionSettings connection={connection.data} initialPublicURL={remoteURL} onSaved={next => { setRemoteURL(next.public_url); setEditingConnection(false) }} /></div> : null}
    {remoteBlocked || connection.data.restart_required ? <Alert variant='warn'><TriangleAlert /><AlertTitle>{connection.data.restart_required ? '重新启动 Studio 后使用新的监听地址' : '节点服务仅监听本机，远程接入需要修改连接设置'}</AlertTitle></Alert> : null}
    {connection.data.listen_error ? <Alert variant='warn'><TriangleAlert /><AlertTitle>{connection.data.listen_error}</AlertTitle></Alert> : null}
    {savedCredential.error ? <ErrorBanner message={savedCredential.error.message} /> : null}
    <div className='flex min-w-0 flex-col gap-2'>
      <div className='flex flex-wrap items-center justify-between gap-2'><FieldLabel htmlFor='edge-deploy'>{t('edges.deployCommand')}</FieldLabel>
        <Tabs value={shell} onValueChange={(value) => setShell(value as AgentShell)}><TabsList aria-label='命令环境'><TabsTrigger value='bash'>macOS / Linux</TabsTrigger><TabsTrigger value='powershell'>Windows PowerShell</TabsTrigger></TabsList></Tabs>
      </div>
      <pre id='edge-deploy' className='max-h-72 overflow-auto rounded-md border bg-background p-3 text-xs break-all whitespace-pre-wrap'>{edgeDeployCommand({ ...commandInput, maskToken: true })}</pre>
      <Button type='button' size='sm' disabled={!token || !validURL || remoteBlocked || Boolean(connection.data.listen_error) || connection.data.restart_required} onClick={() => void copy(edgeDeployCommand(commandInput))}><Copy />{t('edges.copyDeployCommand')}</Button>
    </div>
    <FieldGroup><Field><FieldLabel htmlFor='edge-agent-token'>{t('edges.fieldAgentToken')}</FieldLabel>
      <div className='flex flex-wrap gap-2'><SecretInput id='edge-agent-token' className='min-w-0 flex-1' readOnly value={token} autoComplete='off' /><Button type='button' size='sm' variant='outline' disabled={!token} onClick={() => void copy(token)}>{t('edges.copy')}</Button>
        {onRegenerate ? <Button type='button' size='sm' variant='outline' onClick={onRegenerate}>{t('edges.regenerate')}</Button> : null}
      </div>
      {!token ? <p className='text-sm text-muted-foreground'>重新生成凭据后复制部署命令。</p> : null}
    </Field></FieldGroup>
    {copyError ? <p role='alert' className='text-sm text-destructive'>{copyError}</p> : null}
  </section>
}
