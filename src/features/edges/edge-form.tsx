import { useState } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { RefreshCw } from 'lucide-react'
import { createComputeNode, getComputeNode, updateComputeNode, type ComputeNode, type ComputeNodeCredential } from '@/lib/api/compute-nodes'
import type { EdgeHardwareGPU } from '@/lib/api/types'
import { Button } from '@/components/ui/button'
import { Field, FieldGroup, FieldLabel } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Switch } from '@/components/ui/switch'
import { Textarea } from '@/components/ui/textarea'

export function EdgeForm({ initial, onSaved, onCredential, onCancel }: {
  initial?: ComputeNode; onSaved: (node: ComputeNode) => void
  onCredential: (credential: ComputeNodeCredential) => void; onCancel: () => void
}) {
  const { t } = useTranslation()
  const client = useQueryClient()
  const [name, setName] = useState(initial?.name ?? '')
  const [description, setDescription] = useState(initial?.description ?? '')
  const [enabled, setEnabled] = useState(initial?.enabled ?? true)
  const [location, setLocation] = useState<ComputeNode['location']>(initial?.location ?? 'remote')
  const [baseUrl, setBaseUrl] = useState(initial?.comfyui_base_url ?? '')
  const [capabilities, setCapabilities] = useState(initial?.capabilities.join(', ') ?? '')
  const [revision, setRevision] = useState(initial?.configuration_revision)
  const [cpuModel, setCpuModel] = useState(initial?.hardware?.cpu_model ?? '')
  const [cpuCores, setCpuCores] = useState(initial?.hardware?.cpu_cores?.toString() ?? '')
  const [ramBytes, setRamBytes] = useState(initial?.hardware?.ram_bytes?.toString() ?? '')
  const [gpus, setGpus] = useState<EdgeHardwareGPU[]>(initial?.hardware?.gpus ?? [])
  const refresh = useMutation({ mutationFn: async () => {
    if (!initial || revision === undefined) throw new Error('计算节点缺失')
    const previous = (await getComputeNode(initial.id)).hardware?.collected_at
    const updated = await updateComputeNode(initial.id, { expected_revision: revision, refresh_hardware: true })
    setRevision(updated.configuration_revision)
    const deadline = Date.now() + 30000
    while (Date.now() < deadline) {
      const node = await getComputeNode(initial.id)
      if (node.hardware?.collected_at && node.hardware.collected_at !== previous) return node
      await new Promise(resolve => setTimeout(resolve, 1000))
    }
    throw new Error('硬件信息尚未更新，检查 Agent 连接状态')
  }, onSuccess: async node => {
    setCpuModel(node.hardware?.cpu_model ?? '')
    setCpuCores(node.hardware?.cpu_cores?.toString() ?? '')
    setRamBytes(node.hardware?.ram_bytes?.toString() ?? '')
    setGpus(node.hardware?.gpus ?? [])
    await client.invalidateQueries({ queryKey: ['studio', 'compute-nodes'] })
  } })
  const save = useMutation({ mutationFn: async () => {
    const input = { name: name.trim(), description, enabled, comfyui_base_url: baseUrl.trim(), capabilities: [...new Set(capabilities.split(',').map(value => value.trim()).filter(Boolean))] }
    if (initial) {
      if (revision === undefined) throw new Error('节点修订缺失')
      return updateComputeNode(initial.id, { ...input, expected_revision: revision, hardware: { cpu_model: cpuModel.trim() || undefined, cpu_cores: cpuCores ? Number(cpuCores) : undefined, ram_bytes: ramBytes ? Number(ramBytes) : undefined, gpus: gpus.filter(gpu => gpu.name.trim()) } })
    }
    const credential = await createComputeNode({ ...input, location })
    onCredential(credential)
    return credential.node
  }, onSuccess: async (node) => {
    await client.invalidateQueries({ queryKey: ['studio', 'compute-nodes'] })
    onSaved(node)
  } })
  const pending = save.isPending || refresh.isPending
  return <form id='edge-form' className='flex flex-col gap-4' data-testid='edge-form' onSubmit={(event) => { event.preventDefault(); save.mutate() }}>
    <FieldGroup>
      <Field><FieldLabel htmlFor='edge-name'>{t('edges.fieldName')}</FieldLabel><Input id='edge-name' value={name} onChange={(event) => setName(event.target.value)} disabled={save.isPending} required autoComplete='off' /></Field>
      {!initial ? <Field><FieldLabel htmlFor='edge-location'>节点位置</FieldLabel><Select value={location} onValueChange={(value) => setLocation(value as ComputeNode['location'])} disabled={save.isPending}><SelectTrigger id='edge-location'><SelectValue /></SelectTrigger><SelectContent><SelectGroup><SelectItem value='local'>本机</SelectItem><SelectItem value='remote'>远程设备</SelectItem></SelectGroup></SelectContent></Select></Field> : null}
      <Field><FieldLabel htmlFor='edge-comfy-url'>ComfyUI 地址</FieldLabel><Input id='edge-comfy-url' type='url' value={baseUrl} onChange={(event) => setBaseUrl(event.target.value)} placeholder='http://127.0.0.1:8188' required disabled={save.isPending} /></Field>
      <Field><FieldLabel htmlFor='edge-description'>{t('edges.fieldDescription')}</FieldLabel><Textarea id='edge-description' value={description} onChange={(event) => setDescription(event.target.value)} disabled={save.isPending} /></Field>
      <Field><FieldLabel htmlFor='edge-capabilities'>{t('edges.fieldCapabilities')}</FieldLabel><Input id='edge-capabilities' value={capabilities} onChange={(event) => setCapabilities(event.target.value)} disabled={save.isPending} /></Field>
      <Field orientation='horizontal'><FieldLabel htmlFor='edge-enabled'>{t('edges.fieldEnabled')}</FieldLabel><Switch id='edge-enabled' checked={enabled} onCheckedChange={setEnabled} disabled={save.isPending} /></Field>
      {initial ? <>
        <Button type='button' size='sm' variant='outline' disabled={pending} onClick={() => refresh.mutate()}><RefreshCw />{t('edges.refreshHardware')}</Button>
        <Field><FieldLabel htmlFor='edge-cpu'>{t('edges.fieldCpu')}</FieldLabel><Input id='edge-cpu' value={cpuModel} onChange={event => setCpuModel(event.target.value)} disabled={pending} /></Field>
        <Field><FieldLabel htmlFor='edge-cpu-cores'>{t('edges.fieldCpuCores')}</FieldLabel><Input id='edge-cpu-cores' type='number' min={1} step={1} value={cpuCores} onChange={event => setCpuCores(event.target.value)} disabled={pending} /></Field>
        <Field><FieldLabel htmlFor='edge-ram'>{t('edges.fieldRamBytes')}</FieldLabel><Input id='edge-ram' type='number' min={1} step={1} value={ramBytes} onChange={event => setRamBytes(event.target.value)} disabled={pending} /></Field>
        <Field><FieldLabel>{t('edges.fieldGpu')}</FieldLabel>{gpus.map((gpu, index) => <div key={index} className='flex flex-wrap gap-2'><Input className='min-w-0 flex-1' aria-label={`GPU ${index + 1} 名称`} value={gpu.name} disabled={pending} onChange={event => setGpus(current => current.map((item, i) => i === index ? { ...item, name: event.target.value } : item))} /><Input type='number' min={1} step={1} className='min-w-0 flex-1' aria-label={`GPU ${index + 1} 显存字节数`} value={gpu.vram_bytes ?? ''} disabled={pending} onChange={event => setGpus(current => current.map((item, i) => i === index ? { ...item, vram_bytes: event.target.value ? event.target.valueAsNumber : undefined } : item))} /><Button type='button' size='sm' variant='outline' disabled={pending} onClick={() => setGpus(current => current.filter((_, i) => i !== index))}>删除</Button></div>)}<Button type='button' size='sm' variant='outline' disabled={pending} onClick={() => setGpus(current => [...current, { name: '' }])}>{t('edges.addGpu')}</Button></Field>
      </> : null}
    </FieldGroup>
    {save.error || refresh.error ? <p role='alert' className='text-sm text-destructive'>{(save.error ?? refresh.error)?.message}</p> : null}
    <div className='flex justify-end gap-2'><Button type='button' size='sm' variant='outline' onClick={onCancel} disabled={pending}>取消</Button><Button type='submit' size='sm' disabled={pending || !name.trim()}>{initial ? t('common.save') : t('edges.createAndContinue')}</Button></div>
  </form>
}
