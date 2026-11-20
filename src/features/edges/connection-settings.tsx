import { useState } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { saveComputeConnection, type ComputeConnection } from '@/lib/api/compute-nodes'
import { Alert, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Field, FieldGroup, FieldLabel } from '@/components/ui/field'
import { Input } from '@/components/ui/input'

export function ComputeConnectionSettings({ connection, initialPublicURL, onSaved }: {
  connection: ComputeConnection; initialPublicURL?: string; onSaved?: (connection: ComputeConnection) => void
}) {
  const client = useQueryClient()
  const [listenAddress, setListenAddress] = useState(connection.configured_listen_address)
  const [publicURL, setPublicURL] = useState(initialPublicURL ?? connection.public_url)
  const save = useMutation({ mutationFn: () => saveComputeConnection({ listen_address: listenAddress, public_url: publicURL }), onSuccess: next => {
    client.setQueryData(['studio', 'compute-connection'], next)
    onSaved?.(next)
  } })
  return <form className='flex flex-col gap-5' onSubmit={event => { event.preventDefault(); save.mutate() }}>
    <FieldGroup>
      <Field><FieldLabel htmlFor='compute-listen-address'>监听地址</FieldLabel><Input id='compute-listen-address' value={listenAddress} onChange={event => setListenAddress(event.target.value)} disabled={connection.environment_override || save.isPending} required /></Field>
      <Field><FieldLabel htmlFor='compute-public-url'>远程访问地址</FieldLabel><Input id='compute-public-url' type='url' value={publicURL} onChange={event => setPublicURL(event.target.value)} placeholder='http://192.168.1.10:18083' disabled={save.isPending} /></Field>
    </FieldGroup>
    {connection.environment_override ? <p className='text-sm text-muted-foreground'>监听地址由启动配置指定：{connection.listen_address}</p> : null}
    {connection.listen_error ? <Alert variant='warn'><AlertTitle>{connection.listen_error}</AlertTitle></Alert> : null}
    {connection.restart_required ? <Alert variant='warn'><AlertTitle>重新启动 Studio 后使用新的监听地址</AlertTitle></Alert> : null}
    {save.error ? <p role='alert' className='text-sm text-destructive'>{save.error.message}</p> : null}
    <Button type='submit' size='sm' className='self-end' disabled={save.isPending}>保存连接设置</Button>
  </form>
}
