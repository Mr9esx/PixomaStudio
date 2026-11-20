import { useRef, useState } from 'react'
import { useForm, useWatch, type FieldPath, type FieldPathValue } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useBlocker } from '@tanstack/react-router'
import { ArrowDown, ArrowLeft, ArrowUp, Bot, Plus, Sparkles, Trash2, Workflow } from 'lucide-react'
import { toast } from 'sonner'
import { expertAvatarURL, getExpert, listExpertVersions, listReferenceSkills, listReferenceWorkflows, saveExpert, uploadExpertAvatar, type Expert } from '@/lib/api/agent-assets'
import { listStudioModels, type StudioComposerPart } from '@/lib/api/studio'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Field, FieldError, FieldLabel } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { Command, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList } from '@/components/ui/command'
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from '@/components/ui/alert-dialog'
import { StudioComposer, type StudioComposerHandle, type StudioReference } from '@/features/studio/studio-composer'
import { type StudioComposerValue } from '@/features/studio/studio-composer-content'
import { nextSkillVersion, skillVersionError } from '@/features/studio/skill-version'
import { expertFormSchema, expertPartsText, expertReferencedCapabilities, type ExpertContent, type ExpertPart } from './expert-form'

export function ExpertEditor({ scope, id, onClose }: { scope: 'studio'; id?: string; onClose: () => void }) {
  const detail = useQuery({ queryKey: ['agent-assets', scope, 'experts', id], queryFn: () => getExpert(scope, id!), enabled: Boolean(id) })
  if (id && detail.isPending) return <p role='status' className='p-6'>正在读取专家</p>
  if (detail.error) return <div role='alert' className='flex gap-3 p-6'><span>{detail.error.message}</span><Button variant='outline' onClick={() => void detail.refetch()}>重新读取</Button><Button variant='ghost' onClick={onClose}>返回</Button></div>
  return <ExpertEditorForm key={id ?? 'new'} scope={scope} previous={detail.data} onClose={onClose} />
}

function ExpertEditorForm({ scope, previous, onClose }: { scope: 'studio'; previous?: Expert; onClose: () => void }) {
  const client = useQueryClient()
  const initial: ExpertContent = previous ? expertFormSchema.parse(previous) : { schema_version: 2, name: '', description: '', model_id: '', prompt_parts: [], avatar_asset_id: '', avatar_version_id: '', examples: [], skill_refs: [], workflow_refs: [], sop: [] }
  const form = useForm<ExpertContent>({ resolver: zodResolver(expertFormSchema), defaultValues: initial })
  const content = useWatch({ control: form.control }) as ExpertContent
  const [tab, setTab] = useState('basic')
  const [version, setVersion] = useState(nextSkillVersion(previous?.version))
  const [notes, setNotes] = useState('')
  const [review, setReview] = useState(false)
  const [leave, setLeave] = useState(false)
  const leaving = useRef(false)
  const avatarInput = useRef<HTMLInputElement>(null)
  const [avatarPreview, setAvatarPreview] = useState(previous?.avatar_asset_id ? expertAvatarURL(scope, previous.id, previous.version) : '')
  const models = useQuery({ queryKey: ['agent-assets', scope, 'models'], queryFn: listStudioModels })
  const skills = useQuery({ queryKey: ['agent-assets', scope, 'reference', 'skills'], queryFn: () => listReferenceSkills(scope) })
  const workflows = useQuery({ queryKey: ['agent-assets', scope, 'reference', 'workflows'], queryFn: () => listReferenceWorkflows(scope) })
  const versions = useQuery({ queryKey: ['agent-assets', scope, 'experts', previous?.id, 'versions'], queryFn: () => listExpertVersions(scope, previous!.id), enabled: tab === 'versions' && Boolean(previous) })
  const dirty = form.formState.isDirty || notes !== '' || version !== nextSkillVersion(previous?.version)
  const blocker = useBlocker({ shouldBlockFn: () => !leaving.current && (dirty || busy), enableBeforeUnload: () => dirty || busy, withResolver: true })
  const references: StudioReference[] = [
    ...(skills.data ?? []).map(skill => ({ kind: 'skill' as const, id: skill.id, label: skill.name, capabilityVersion: skill.version })),
    ...(workflows.data ?? []).map(workflow => ({ kind: 'workflow' as const, id: workflow.id, label: workflow.name, capabilityVersion: String(workflow.version_id) })),
  ]
  const capabilities = expertReferencedCapabilities(content)
  const update = <K extends FieldPath<ExpertContent>>(field: K, value: FieldPathValue<ExpertContent, K>) => form.setValue(field, value, { shouldDirty: true, shouldValidate: form.formState.isSubmitted })
  const avatar = useMutation({ mutationFn: (file: File) => uploadExpertAvatar(scope, file, crypto.randomUUID()), onSuccess: asset => {
    const current = asset.versions.find(item => item.version === asset.current_version)
    if (!current) throw new Error('头像版本不存在')
    update('avatar_asset_id', asset.id); update('avatar_version_id', current.id)
    setAvatarPreview(current.content_url)
  }, onError: error => toast.error(error.message) })
  const save = useMutation({ mutationFn: (value: ExpertContent) => {
    const error = skillVersionError(version, previous?.version)
    if (error) throw new Error(error)
    return saveExpert(scope, value, version, notes, previous)
  }, onSuccess: async () => { await client.invalidateQueries({ queryKey: ['agent-assets'] }); toast.success(`已保存版本 ${version}`); leaving.current = true; onClose() } })
  const submit = form.handleSubmit(value => save.mutate(value), errors => setTab(errors.name || errors.description ? 'basic' : errors.model_id || errors.prompt_parts ? 'role' : 'method'))
  const reorder = (index: number, offset: number) => { const next = [...content.sop]; [next[index], next[index + offset]] = [next[index + offset], next[index]]; update('sop', next) }
  const busy = save.isPending || avatar.isPending
  return <section className='flex min-h-0 flex-1 flex-col overflow-hidden bg-card'>
    <header data-tauri-drag-region='deep' className='studio-page-header studio-continuous-header justify-between'>
      <div className='flex min-w-0 flex-1 items-center gap-3'><Button variant='ghost' size='icon-sm' aria-label='返回专家列表' disabled={busy} onClick={() => dirty ? setLeave(true) : onClose()}><ArrowLeft /></Button><Bot className='size-4 shrink-0' /><h1 className='truncate text-sm font-semibold'>{previous ? content.name : '创建专家'}</h1>{dirty ? <span className='shrink-0 text-xs text-muted-foreground'>未保存</span> : previous ? <Badge variant='secondary'>v{previous.version}</Badge> : null}</div>
      <Button size='sm' disabled={busy || models.isPending || Boolean(models.error || skills.error || workflows.error)} onClick={() => setReview(true)}>保存版本</Button>
    </header>
    <Tabs value={tab} onValueChange={setTab} className='flex min-h-0 flex-1 flex-col gap-0'>
      <TabsList className='mx-5 mb-3 shrink-0 self-start'><TabsTrigger value='basic'>基本信息</TabsTrigger><TabsTrigger value='role'>提示词</TabsTrigger><TabsTrigger value='method'>工作方法</TabsTrigger><TabsTrigger value='versions' disabled={!previous}>版本记录</TabsTrigger></TabsList>
      <div className='relative min-h-0 flex-1 overflow-y-auto px-5 pb-5'>
        <div className='flex w-full max-w-5xl flex-col gap-6'>
          {review ? <section aria-label='保存专家版本' className='flex flex-col gap-4 rounded-xl bg-muted p-5'><div className='flex items-center justify-between gap-3'><h2 className='text-sm font-semibold'>保存新版本</h2><Button size='sm' variant='ghost' disabled={busy} onClick={() => setReview(false)}>收起</Button></div><div className='flex flex-wrap gap-4'><Field className='max-w-56'><FieldLabel htmlFor='expert-version'>版本号</FieldLabel><Input id='expert-version' value={version} disabled={busy} onChange={event => setVersion(event.target.value)} /></Field><Field className='min-w-40 flex-1'><FieldLabel htmlFor='expert-notes'>版本说明</FieldLabel><Input id='expert-notes' value={notes} disabled={busy} onChange={event => setNotes(event.target.value)} /></Field></div><div className='flex justify-end gap-2'><Button size='sm' variant='outline' disabled={busy} onClick={() => setReview(false)}>取消</Button><Button size='sm' disabled={busy} onClick={() => void submit()}>{save.isPending ? '正在保存' : '确认保存'}</Button></div></section> : null}
          <TabsContent value='basic' className='flex flex-col gap-5'>
            <Field><FieldLabel htmlFor='expert-name'>名称</FieldLabel><Input id='expert-name' placeholder='例如：短剧创作专家' {...form.register('name')} aria-invalid={Boolean(form.formState.errors.name)} /><FieldError errors={[form.formState.errors.name]} /></Field>
            <Field><FieldLabel htmlFor='expert-description'>介绍</FieldLabel><Textarea id='expert-description' placeholder='描述可以完成的创作任务' {...form.register('description')} /></Field>
            <Field><FieldLabel htmlFor='expert-avatar'>头像</FieldLabel><div className='flex items-center gap-3'>{avatarPreview ? <img src={avatarPreview} alt='专家头像' className='size-12 rounded-full border object-cover' /> : <Bot className='size-8 text-muted-foreground' />}<input ref={avatarInput} id='expert-avatar' type='file' accept='image/*' className='hidden' onChange={event => { const file = event.target.files?.[0]; if (file) avatar.mutate(file); event.target.value = '' }} /><Button variant='outline' size='sm' disabled={avatar.isPending} onClick={() => avatarInput.current?.click()}>上传头像</Button><Button variant='ghost' size='sm' disabled={!content.avatar_asset_id} onClick={() => { update('avatar_asset_id', ''); update('avatar_version_id', ''); setAvatarPreview('') }}>移除</Button></div></Field>
            <Field><FieldLabel htmlFor='expert-examples'>作品示例</FieldLabel><Textarea id='expert-examples' placeholder='每行输入一个示例' value={content.examples.join('\n')} onChange={event => update('examples', event.target.value.split('\n'))} /></Field>
          </TabsContent>
          <TabsContent value='role' className='flex flex-col gap-5'>
            <Field><FieldLabel htmlFor='expert-model'>模型</FieldLabel><Choice id='expert-model' value={content.model_id} onChange={value => update('model_id', value)} options={(models.data ?? []).filter(model => model.enabled && model.agent_enabled && model.capabilities.tools).map(model => ({ value: model.id, label: model.name }))} placeholder='选择模型' /><FieldError errors={[form.formState.errors.model_id]} /></Field>
            <Field><FieldLabel htmlFor='expert-prompt'>专家提示词</FieldLabel><ExpertInstructionEditor id='expert-prompt' label='专家提示词' parts={content.prompt_parts} items={references} loading={skills.isPending || workflows.isPending} onChange={parts => update('prompt_parts', parts)} placeholder='输入专家职责、沟通方式与要求，输入 @ 引用能力' /><FieldError errors={[form.formState.errors.prompt_parts]} /></Field>
          </TabsContent>
          <TabsContent value='method' className='flex flex-col gap-5'>
            <div className='flex items-center justify-between gap-3'><h2 className='text-sm font-medium'>SOP</h2><Button size='sm' variant='outline' onClick={() => update('sop', [...content.sop, { id: crypto.randomUUID(), title: '', instruction_parts: [] }])}><Plus />增加步骤</Button></div>
            {content.sop.map((step, index) => <section key={step.id} className='flex flex-col gap-4 rounded-xl bg-muted p-4'>
              <div className='flex items-center gap-2'><span className='text-sm text-muted-foreground'>{index + 1}</span><Input aria-label={`步骤 ${index + 1} 名称`} placeholder='步骤名称' value={step.title} onChange={event => update('sop', content.sop.map(item => item.id === step.id ? { ...item, title: event.target.value } : item))} /><Button variant='ghost' size='icon-sm' disabled={index === 0} aria-label='向前移动步骤' onClick={() => reorder(index, -1)}><ArrowUp /></Button><Button variant='ghost' size='icon-sm' disabled={index === content.sop.length - 1} aria-label='向后移动步骤' onClick={() => reorder(index, 1)}><ArrowDown /></Button><Button variant='ghost' size='icon-sm' aria-label='删除步骤' onClick={() => update('sop', content.sop.filter(item => item.id !== step.id))}><Trash2 /></Button></div>
              <FieldError errors={[form.formState.errors.sop?.[index]?.title]} />
              <ExpertInstructionEditor id={`expert-step-${step.id}`} label={`步骤 ${index + 1} 执行说明`} parts={step.instruction_parts} items={references} loading={skills.isPending || workflows.isPending} placeholder='说明需要哪些资料、如何处理、产出和检查要求，以及什么时候等待用户确认。输入 @ 引用能力' onChange={parts => update('sop', content.sop.map(item => item.id === step.id ? { ...item, instruction_parts: parts } : item))} />
              <FieldError errors={[form.formState.errors.sop?.[index]?.instruction_parts]} />
            </section>)}
          </TabsContent>
          <TabsContent value='versions' className='flex flex-col gap-5'>{versions.isPending ? <p role='status' className='text-sm text-muted-foreground'>正在读取版本…</p> : versions.error ? <p role='alert' className='text-sm text-destructive'>{versions.error.message}</p> : versions.data?.map(item => <div key={item.version} className='flex flex-wrap items-center justify-between gap-3 rounded-lg py-3 text-sm'><span className='font-mono'>v{item.version}</span><span className='min-w-0 flex-1'>{item.notes}</span><time className='text-xs text-muted-foreground'>{new Date(item.created_at).toLocaleString()}</time></div>)}</TabsContent>
          {tab !== 'versions' ? <section className='flex flex-col gap-3 rounded-xl bg-muted p-4'><h2 className='text-sm font-medium'>引用能力</h2><div className='flex flex-wrap gap-2'>{capabilities.map(part => <Badge key={part.type === 'skill' ? `skill:${part.skill_id}` : `workflow:${part.workflow_id}`} variant='secondary'>{part.type === 'skill' ? <Sparkles /> : <Workflow />}{expertPartsText([part])}</Badge>)}</div></section> : null}
          {save.error || models.error || skills.error || workflows.error ? <p role='alert' className='text-sm text-destructive'>{(save.error ?? models.error ?? skills.error ?? workflows.error)?.message}</p> : null}
        </div>
      </div>
    </Tabs>
    <AlertDialog open={leave || blocker.status === 'blocked'} onOpenChange={open => { if (!open) { setLeave(false); blocker.reset?.() } }}><AlertDialogContent><AlertDialogHeader><AlertDialogTitle>放弃未保存的修改？</AlertDialogTitle><AlertDialogDescription>当前编辑内容尚未保存。</AlertDialogDescription></AlertDialogHeader><AlertDialogFooter><AlertDialogCancel>继续编辑</AlertDialogCancel><AlertDialogAction disabled={busy} onClick={() => { leaving.current = true; if (blocker.status === 'blocked') blocker.proceed(); else onClose() }}>放弃修改</AlertDialogAction></AlertDialogFooter></AlertDialogContent></AlertDialog>
  </section>
}

function composerParts(parts: ExpertPart[]): StudioComposerPart[] {
  return parts.map(part => part.type === 'text' ? part : part.type === 'skill' ? { type: 'skill_ref', skill_id: part.skill_id, name: part.name ?? part.skill_id, version: part.version } : { type: 'workflow_ref', workflow_id: part.workflow_id, name: part.name ?? part.workflow_id, version_id: part.version_id })
}

export function ExpertInstructionEditor({ id, label, parts, items, loading, placeholder, onChange }: { id: string; label: string; parts: ExpertPart[]; items: StudioReference[]; loading: boolean; placeholder: string; onChange: (parts: ExpertPart[]) => void }) {
  const ref = useRef<StudioComposerHandle>(null)
  const [open, setOpen] = useState(false)
  const [initial] = useState<StudioComposerValue>({ text: expertPartsText(parts), parts: composerParts(parts), selectedSkillIds: [], selectedAssets: [] })
  const change = (value: StudioComposerValue) => onChange(value.parts.map(part => {
    if (part.type === 'text') return part
    if (part.type === 'skill_ref' && part.version) return { type: 'skill', skill_id: part.skill_id, version: part.version, name: part.name }
    if (part.type === 'workflow_ref' && part.version_id) return { type: 'workflow', workflow_id: part.workflow_id, version_id: part.version_id, name: part.name }
    throw new Error('专家引用缺少能力版本')
  }))
  return <div className='rounded-md border bg-background focus-within:ring-2 focus-within:ring-ring/50'>
    <StudioComposer ref={ref} inputId={id} ariaLabel={label} initialValue={initial} referenceItems={items} referenceMode='capabilities' submitOnEnter={false} placeholder={placeholder} suggestionLoading={loading} onValueChange={change} />
    <div className='p-1'><Popover open={open} onOpenChange={setOpen}><PopoverTrigger asChild><Button size='sm' variant='ghost' aria-label={`${label}引用能力`}>@ 引用能力</Button></PopoverTrigger><PopoverContent align='start' className='w-80 p-1' onCloseAutoFocus={event => event.preventDefault()}><Command><CommandInput placeholder='搜索 Skill 或工作流' /><CommandList><CommandEmpty>{loading ? '正在读取能力' : '没有可引用的能力'}</CommandEmpty>{(['skill', 'workflow'] as const).map(kind => <CommandGroup key={kind} heading={kind === 'skill' ? 'Skill' : '工作流'}>{items.filter(item => item.kind === kind).map(item => <CommandItem key={`${kind}:${item.id}`} value={item.label} onSelect={() => { ref.current?.insertReference(item); setOpen(false) }}>{kind === 'skill' ? <Sparkles /> : <Workflow />}<span className='flex-1'>{item.label}</span><span className='text-xs text-muted-foreground'>{kind === 'skill' ? 'v' : '#'}{item.capabilityVersion}</span></CommandItem>)}</CommandGroup>)}</CommandList></Command></PopoverContent></Popover></div>
  </div>
}

export function Choice({ id, ariaLabel, value, onChange, options, placeholder, invalid, required }: { id?: string; ariaLabel?: string; value: string; onChange: (value: string) => void; options: { value: string; label: string }[]; placeholder?: string; invalid?: boolean; required?: boolean }) {
  return <Select value={value} onValueChange={onChange} required={required}><SelectTrigger id={id} aria-label={ariaLabel} aria-invalid={invalid} className='bg-background'><SelectValue placeholder={placeholder} /></SelectTrigger><SelectContent><SelectGroup>{options.map(option => <SelectItem key={option.value} value={option.value}>{option.label}</SelectItem>)}</SelectGroup></SelectContent></Select>
}
