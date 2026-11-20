import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { ArrowLeft, Plus, Server, TriangleAlert } from 'lucide-react'
import { deleteComputeNode, getComputeConnection, listComputeNodes, rotateComputeNodeCredential, type ComputeNode, type ComputeNodeCredential } from '@/lib/api/compute-nodes'
import { Alert, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Empty, EmptyHeader, EmptyMedia, EmptyTitle } from '@/components/ui/empty'
import { ConfirmDialog } from '@/components/confirm-dialog'
import { EdgeForm } from '@/features/edges/edge-form'
import { CreateEdgeWizard } from '@/features/edges/create-edge-wizard'
import { DeployCredentials } from '@/features/edges/deploy-credentials'
import { EdgeDetailPanel } from '@/features/edges/detail-panel'
import { ComputeConnectionSettings } from '@/features/edges/connection-settings'
import { ErrorBanner } from '@/components/feedback/error-banner'
import { Skeleton } from '@/components/ui/skeleton'
import { EdgeListPanel } from '@/features/edges/list-panel'

export function StudioComputeSettings({ create = false, onCreateHandled, onReturnWorkflow }: {
  create?: boolean; onCreateHandled: () => void; onReturnWorkflow?: () => void
}) {
  const client = useQueryClient()
  const [selectedId, setSelectedId] = useState<string>()
  const [form, setForm] = useState<{ initial?: ComputeNode } | undefined>(() => create ? {} : undefined)
  const [credential, setCredential] = useState<ComputeNodeCredential>()
  const [deploying, setDeploying] = useState(false)
  const [editingConnection, setEditingConnection] = useState(false)
  const [confirm, setConfirm] = useState<'delete' | 'credential'>()
  const nodes = useQuery({ queryKey: ['studio', 'compute-nodes'], queryFn: listComputeNodes, refetchInterval: 5000 })
  const connection = useQuery({ queryKey: ['studio', 'compute-connection'], queryFn: getComputeConnection })
  const node = nodes.data?.find((item) => item.id === selectedId)
  const refresh = () => client.invalidateQueries({ queryKey: ['studio', 'compute-nodes'] })
  const remove = useMutation({ mutationFn: () => { if (!node) throw new Error('计算节点缺失'); return deleteComputeNode(node.id) },
    onSuccess: async () => { setConfirm(undefined); setSelectedId(undefined); await refresh() },
  })
  const rotate = useMutation({ mutationFn: () => { if (!node) throw new Error('计算节点缺失'); return rotateComputeNodeCredential(node.id) },
    onSuccess: async (next) => { setConfirm(undefined); setCredential(next); setDeploying(true); await refresh() },
  })
  const closeForm = () => { setForm(undefined); onCreateHandled() }
  const completeCreation = (next: ComputeNode) => {
    client.setQueryData<ComputeNode[]>(['studio', 'compute-nodes'], (current = []) => current.some(item => item.id === next.id) ? current.map(item => item.id === next.id ? next : item) : [...current, next])
    setSelectedId(next.id)
    closeForm()
  }
  if (editingConnection) return <section className='flex min-h-0 flex-1 flex-col'><header className='studio-page-header gap-3'><Button size='icon-sm' variant='ghost' aria-label='返回计算节点' onClick={() => setEditingConnection(false)}><ArrowLeft /></Button><h2 className='text-sm font-semibold'>连接设置</h2></header><div className='mx-auto flex w-full max-w-2xl flex-col gap-4 overflow-y-auto px-5 pb-5'>
    {connection.isPending ? <Skeleton className='h-48' /> : connection.error ? <ErrorBanner message={connection.error.message} onRetry={() => void connection.refetch()} /> : <ComputeConnectionSettings connection={connection.data} />}
  </div></section>
  if (form || deploying && node) return <section className='flex min-h-0 flex-1 flex-col'>
    <header className='studio-page-header gap-3'><Button size='icon-sm' variant='ghost' aria-label='返回计算节点' onClick={() => { closeForm(); setDeploying(false) }}><ArrowLeft /></Button><h2 className='text-sm font-semibold'>{form ? form.initial ? '编辑节点' : '新建节点' : '部署节点'}</h2></header>
    <div className='flex min-h-0 flex-1 flex-col overflow-y-auto px-5 pb-5'>
      {form ? form.initial ? <div className='mx-auto w-full max-w-2xl'><EdgeForm initial={form.initial} onCredential={setCredential} onSaved={(next) => { setSelectedId(next.id); closeForm() }} onCancel={closeForm} /></div>
        : <CreateEdgeWizard onCredential={(next) => { setCredential(next); setSelectedId(next.node.id) }} onCancel={closeForm} onDone={completeCreation} />
        : <div className='mx-auto flex w-full max-w-2xl flex-col gap-5'><DeployCredentials node={node!} credential={credential?.node.id === node!.id ? credential : undefined} onRegenerate={() => { rotate.reset(); setConfirm('credential') }} /></div>}
    </div>
    <ConfirmDialog open={confirm === 'credential'} onOpenChange={(open) => { if (!open) setConfirm(undefined) }} title='重新生成 AGENT_TOKEN？' desc={<div className='flex flex-col gap-2'><p>旧 Token 立即失效，运行中的 Agent 需要使用新凭据重新启动。</p>{rotate.error ? <p role='alert' className='text-destructive'>{rotate.error.message}</p> : null}</div>} confirmText='重新生成' cancelBtnText='取消' destructive isLoading={rotate.isPending} handleConfirm={() => rotate.mutate()} />
  </section>
  return <section className='flex min-h-0 flex-1 flex-col'>
    <div className='flex shrink-0 flex-wrap items-center justify-between gap-3 border-b px-5 py-3'>
      <h2 className='text-lg font-semibold'>计算节点</h2>
      <div className='flex flex-wrap gap-2'>
        {onReturnWorkflow ? <Button size='sm' variant='outline' onClick={onReturnWorkflow}><ArrowLeft />返回工作流</Button> : null}
        <Button size='sm' variant='outline' onClick={() => setEditingConnection(true)}>连接设置</Button>
        <Button size='sm' onClick={() => setForm({})}><Plus />新建节点</Button>
      </div>
    </div>
    {connection.data?.listen_error && !node ? <Alert variant='warn' className='mx-5 my-3 w-auto'><TriangleAlert /><AlertTitle>{connection.data.listen_error}</AlertTitle></Alert> : null}
    <div className='flex min-h-0 flex-1 flex-col min-[920px]:flex-row'>
      <div className='max-h-60 min-h-40 shrink-0 border-b min-[920px]:max-h-none min-[920px]:w-64 min-[920px]:border-r min-[920px]:border-b-0'>
        <EdgeListPanel items={nodes.data ?? []} selectedId={selectedId} isLoading={nodes.isPending} error={nodes.error} onRetry={() => void nodes.refetch()} onSelect={setSelectedId} />
      </div>
      <div className='flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto'>
        {node ? <EdgeDetailPanel node={node} error={nodes.error} onEdit={() => setForm({ initial: node })} onDeploy={() => setDeploying(true)} onDelete={() => { remove.reset(); setConfirm('delete') }} onRotate={() => { rotate.reset(); setConfirm('credential') }} /> : <Empty><EmptyHeader><EmptyMedia variant='icon'><Server /></EmptyMedia><EmptyTitle>选择计算节点</EmptyTitle></EmptyHeader></Empty>}
      </div>
    </div>
    <ConfirmDialog open={Boolean(confirm)} onOpenChange={(open) => { if (!open) setConfirm(undefined) }} title={confirm === 'delete' ? '删除节点' : '重新生成凭据'}
      desc={<div className='flex flex-col gap-2'><p>{node?.name}</p>{(confirm === 'delete' ? remove.error : rotate.error) ? <p role='alert' className='text-destructive'>{(confirm === 'delete' ? remove.error : rotate.error)?.message}</p> : null}</div>}
      confirmText={confirm === 'delete' ? '删除' : '重新生成'} cancelBtnText='取消' destructive isLoading={remove.isPending || rotate.isPending}
      handleConfirm={() => confirm === 'delete' ? remove.mutate() : rotate.mutate()} />
  </section>
}
