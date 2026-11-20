import { useMutation, useQueryClient } from '@tanstack/react-query'
import { controlLocalAgent, type ComputeNode } from '@/lib/api/compute-nodes'
import { Button } from '@/components/ui/button'

export function LocalAgentControls({ node }: { node: ComputeNode }) {
  const client = useQueryClient()
  const control = useMutation({ mutationFn: (enabled: boolean) => controlLocalAgent(node.id, enabled), onSuccess: async next => {
    client.setQueryData(['studio', 'compute-node', node.id], next)
    await client.invalidateQueries({ queryKey: ['studio', 'compute-nodes'] })
  } })
  if (node.location !== 'local') return null
  return <div className='flex flex-wrap items-center gap-2'>
    <Button type='button' size='sm' variant='outline' disabled={control.isPending} onClick={() => control.mutate(true)}>{node.local_agent_enabled ? '重新启动 Agent' : '启动 Agent'}</Button>
    {node.local_agent_enabled ? <Button type='button' size='sm' variant='ghost' disabled={control.isPending} onClick={() => control.mutate(false)}>停止 Agent</Button> : null}
    {control.error ? <p role='alert' className='text-sm text-destructive'>{control.error.message}</p> : null}
  </div>
}
