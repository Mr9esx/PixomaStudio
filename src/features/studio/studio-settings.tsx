import { useRef, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import {
  BrainCircuit,
  Cable,
  Plus,
  Sparkles,
  Upload,
} from 'lucide-react'
import { toast } from 'sonner'
import { ApiError } from '@/lib/api/client'
import { invalidateStudioModels } from '@/lib/model-cache'
import {
  createStudioModel,
  createStudioConnector,
  discoverStudioConnector,
  inspectStudioSkillZip,
  listStudioOperationConnectors,
  listStudioOperationModels,
  listStudioOperationSkills,
  probeStudioConnector,
  testStudioModelConfig,
  testStudioModelConnection,
  type StudioConnectorPolicy,
  type StudioMCPConnector,
  type StudioMCPTool,
  type StudioModel,
  type StudioModelConfigInput,
  type StudioSkill,
  type StudioSkillSummary,
  updateStudioConnector,
  updateStudioModel,
  updateStudioSkillEnabled,
} from '@/lib/api/studio'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { ScrollArea } from '@/components/ui/scroll-area'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Switch } from '@/components/ui/switch'
import { LongText } from '@/components/long-text'
import { StatusDot } from '@/components/status-dot'
import { SkillWorkspace } from './skill-workspace'
import { connectorStatus } from './studio-connector-status'
import { StudioSettingsLayout } from './studio-settings-layout'
import { StudioComputeSettings } from './studio-compute-settings'
import { StudioSessionManagement } from './studio-session-management'

export type SettingSection = 'general' | 'models' | 'skills' | 'mcp' | 'compute-nodes'

const settingSections: Array<{
  id: SettingSection
  label: string
  description: string
}> = [
  {
    id: 'general',
    label: '通用',
    description: '',
  },
  {
    id: 'models',
    label: '模型',
    description: '配置模型连接、能力与调用限额',
  },
  {
    id: 'skills',
    label: '技能',
    description: '管理 Agent 可使用的技能',
  },
  {
    id: 'mcp',
    label: '外部 MCP 服务',
    description: '配置外部工具连接和调用策略',
  },
  {
    id: 'compute-nodes',
    label: '计算节点',
    description: '管理 ComfyUI 计算节点',
  },
]

export function StudioSettings({
  section,
  onSectionChange,
  contentOnly = false,
  createNode = false,
  onCreateNodeHandled = () => {},
  onReturnWorkflow,
  onSessionsCleared,
}: {
  section: SettingSection
  onSectionChange: (section: SettingSection) => void
  contentOnly?: boolean
  createNode?: boolean
  onCreateNodeHandled?: () => void
  onReturnWorkflow?: () => void
  onSessionsCleared?: () => void
}) {
  if (section !== 'compute-nodes' && section !== 'general') return <StudioResourceSettings section={section} onSectionChange={onSectionChange} contentOnly={contentOnly} />
  const content = section === 'general'
    ? <StudioSessionManagement onSessionsCleared={onSessionsCleared} />
    : <StudioComputeSettings create={createNode} onCreateHandled={onCreateNodeHandled} onReturnWorkflow={onReturnWorkflow} />
  return contentOnly ? content : <StudioSettingsLayout title='设置' sections={settingSections} section={section} onSectionChange={onSectionChange}>{content}</StudioSettingsLayout>
}

function StudioResourceSettings({
  section,
  onSectionChange,
  contentOnly = false,
}: {
  section: Exclude<SettingSection, 'compute-nodes' | 'general'>
  onSectionChange: (section: SettingSection) => void
  contentOnly?: boolean
}) {
  const queryClient = useQueryClient()
  const [modelDialogOpen, setModelDialogOpen] = useState(false)
  const [editingModel, setEditingModel] = useState<StudioModel>()
  const [skillDialogOpen, setSkillDialogOpen] = useState(false)
  const [editingSkill, setEditingSkill] = useState<StudioSkillSummary>()
  const [importedSkill, setImportedSkill] =
    useState<Pick<StudioSkill, 'name' | 'description' | 'files'>>()
  const skillZipInputRef = useRef<HTMLInputElement>(null)
  const [connectorDialogOpen, setConnectorDialogOpen] = useState(false)
  const [editingConnector, setEditingConnector] = useState<StudioMCPConnector>()
  const models = useQuery({
    queryKey: ['studio-operations', 'models'],
    queryFn: listStudioOperationModels,
  })
  const skills = useQuery({
    queryKey: ['studio-operations', 'skills'],
    queryFn: listStudioOperationSkills,
  })
  const connectors = useQuery({
    queryKey: ['studio-operations', 'connectors'],
    queryFn: listStudioOperationConnectors,
  })
  const savedConnectorProbe = useMutation({
    mutationFn: probeStudioConnector,
    onSuccess: () =>
      queryClient.invalidateQueries({ queryKey: ['studio-operations', 'connectors'] }),
    onError: (cause) => {
      void queryClient.invalidateQueries({ queryKey: ['studio-operations', 'connectors'] })
      toast.error('发现工具失败', {
        description: connectorErrorMessage(cause),
      })
    },
  })
  const testModel = useMutation({
    mutationFn: testStudioModelConnection,
    onSuccess: (result) => {
      toast.success(`连接成功 · ${result.latency_ms} ms`)
    },
    onError: (cause) => {
      const message =
        cause instanceof Error ? cause.message : '模型连接失败，请检查配置。'
      toast.error(message)
    },
  })
  const importSkill = useMutation({
    mutationFn: (file: File) => inspectStudioSkillZip(file),
    onSuccess: (result) => {
      if (skillDialogOpen) {
        toast.error('请先关闭当前技能编辑器')
        return
      }
      setEditingSkill(undefined)
      setImportedSkill(result)
      setSkillDialogOpen(true)
    },
    onError: (cause) => {
      toast.error(
        cause instanceof ApiError
          ? (cause.detail ?? cause.message)
          : cause instanceof Error
            ? cause.message
            : '导入技能失败'
      )
    },
  })
  const activeSection = settingSections.find((item) => item.id === section)!

  if (skillDialogOpen) return <SkillWorkspace scope='studio' skillId={editingSkill?.id} imported={importedSkill} open onOpenChange={open => { setSkillDialogOpen(open); if (!open) { setEditingSkill(undefined); setImportedSkill(undefined) } }} />

  const content = (
    <>
            <ScrollArea key={section} className='min-h-0 flex-1 [&_[data-slot=scroll-area-viewport]>div]:flex! [&_[data-slot=scroll-area-viewport]>div]:min-h-full [&_[data-slot=scroll-area-viewport]>div]:flex-col'>
              <div className='px-5 pt-3'>
                <div className='mx-auto flex max-w-4xl flex-wrap items-center justify-between gap-4'>
                  <div>
                    <h2 className='text-lg font-semibold'>
                      {activeSection.label}
                    </h2>
                    <p className='mt-1 text-xs text-muted-foreground'>
                      {activeSection.description}
                    </p>
                  </div>
                  {section === 'models' ? (
                    <Button
                      size='sm'
                      onClick={() => {
                        setEditingModel(undefined)
                        setModelDialogOpen(true)
                      }}
                    >
                      <Plus />
                      添加模型
                    </Button>
                  ) : null}
                  {section === 'skills' ? (
                    <div className='flex items-center gap-2'>
                      <input
                        ref={skillZipInputRef}
                        className='sr-only'
                        type='file'
                        accept='.zip,application/zip'
                        onChange={(event) => {
                          const file = event.currentTarget.files?.[0]
                          if (file) importSkill.mutate(file)
                          event.currentTarget.value = ''
                        }}
                      />
                      <Button
                        size='sm'
                        variant='outline'
                        disabled={importSkill.isPending}
                        onClick={() => skillZipInputRef.current?.click()}
                      >
                        <Upload />
                        {importSkill.isPending ? '正在检查…' : '导入 ZIP'}
                      </Button>
                      <Button
                        size='sm'
                        disabled={importSkill.isPending}
                        onClick={() => {
                          if (importSkill.isPending) return
                          setEditingSkill(undefined)
                          setImportedSkill(undefined)
                          setSkillDialogOpen(true)
                        }}
                      >
                        <Plus />
                        新建 SKILL
                      </Button>
                    </div>
                  ) : null}
                  {section === 'mcp' ? (
                    <Button
                      size='sm'
                      onClick={() => {
                        setEditingConnector(undefined)
                        setConnectorDialogOpen(true)
                      }}
                    >
                      <Plus />
                      添加连接器
                    </Button>
                  ) : null}
                </div>
              </div>
              {section === 'models' ? (
                <div className='flex flex-1 flex-col px-5 pt-3 pb-5'>
                  <div className='mx-auto flex w-full max-w-4xl flex-1 flex-col gap-4'>
                    {models.isLoading ? (
                      <p className='text-sm text-muted-foreground'>
                        正在读取模型…
                      </p>
                    ) : null}
                    {models.isError ? (
                      <p role='alert' className='text-sm text-destructive'>
                        模型读取失败，刷新后重试。
                      </p>
                    ) : null}
                    {models.data?.length ? (
                      <div className='overflow-hidden rounded-lg border bg-background'>
                        {models.data.map((model) => (
                          <div
                            key={model.id}
                            className='flex flex-wrap items-center gap-4 border-b p-4 last:border-b-0'
                          >
                            <div className='min-w-0 flex-1'>
                              <div className='flex flex-wrap items-center gap-2'>
                                <p className='text-sm font-medium'>
                                  {model.name}
                                </p>
                                <Badge
                                  variant={
                                    model.enabled ? 'secondary' : 'outline'
                                  }
                                >
                                  <StatusDot
                                    problems={
                                      model.enabled &&
                                      model.agent_enabled &&
                                      model.capabilities.tools &&
                                      model.has_api_key &&
                                      model.limits?.context_window_tokens > 0 &&
                                      model.limits?.max_input_tokens > 0 &&
                                      model.limits?.max_output_tokens > 0 &&
                                      model.limits.max_input_tokens <=
                                        model.limits.context_window_tokens &&
                                      model.limits.max_output_tokens <=
                                        model.limits.context_window_tokens
                                        ? 0
                                        : 1
                                    }
                                    label={
                                      model.enabled &&
                                      model.agent_enabled &&
                                      model.capabilities.tools &&
                                      model.has_api_key &&
                                      model.limits?.context_window_tokens > 0 &&
                                      model.limits?.max_input_tokens > 0 &&
                                      model.limits?.max_output_tokens > 0 &&
                                      model.limits.max_input_tokens <=
                                        model.limits.context_window_tokens &&
                                      model.limits.max_output_tokens <=
                                        model.limits.context_window_tokens
                                        ? '配置正常'
                                        : '需要检查配置'
                                    }
                                  />
                                  {model.enabled ? '已启用' : '已停用'}
                                </Badge>
                                {model.default ? <Badge>默认</Badge> : null}
                              </div>
                              <p className='mt-1 truncate text-xs text-muted-foreground'>
                                {model.model} · {model.base_url}
                              </p>
                            </div>
                            <div className='flex items-center gap-2'>
                              <Button
                                variant='outline'
                                size='sm'
                                onClick={() => {
                                  setEditingModel(model)
                                  setModelDialogOpen(true)
                                }}
                              >
                                编辑
                              </Button>
                              <Button
                                variant='outline'
                                size='sm'
                                disabled={testModel.isPending}
                                onClick={() => {
                                  testModel.mutate(model.id)
                                }}
                              >
                                {testModel.isPending &&
                                testModel.variables === model.id
                                  ? '正在检测…'
                                  : '测试连接'}
                              </Button>
                            </div>
                          </div>
                        ))}
                      </div>
                    ) : !models.isLoading && !models.isError ? (
                      <SettingsEmpty
                        icon={BrainCircuit}
                        title='还没有模型'
                        description='添加并启用可供 Agent 使用的模型后，在会话中选择。'
                      />
                    ) : null}
                  </div>
                </div>
              ) : null}
              {section === 'skills' ? (
                <div className='flex flex-1 flex-col px-5 pt-3 pb-5'>
                  <SkillSettings
                    skills={skills.data ?? []}
                    loading={skills.isLoading}
                    error={skills.isError}
                    disabled={importSkill.isPending}
                    onEdit={(skill) => {
                      if (importSkill.isPending) return
                      setEditingSkill(skill)
                      setImportedSkill(undefined)
                      setSkillDialogOpen(true)
                    }}
                  />
                </div>
              ) : null}
              {section === 'mcp' ? (
                <div className='flex flex-1 flex-col px-5 pt-3 pb-5'>
                  <ConnectorSettings
                    connectors={connectors.data ?? []}
                    loading={connectors.isLoading}
                    error={connectors.isError}
                    onEdit={(connector) => {
                      setEditingConnector(connector)
                      setConnectorDialogOpen(true)
                    }}
                  />
                </div>
              ) : null}
            </ScrollArea>
        {modelDialogOpen ? (
          <ModelDialog
            key={editingModel?.id ?? 'new'}
            model={editingModel}
            open={modelDialogOpen}
            onOpenChange={(open) => {
              setModelDialogOpen(open)
              if (!open) setEditingModel(undefined)
            }}
          />
        ) : null}
        {connectorDialogOpen ? (
          <ConnectorDialog
            key={editingConnector?.id ?? 'new'}
            connector={editingConnector}
            open={connectorDialogOpen}
            onOpenChange={(open) => {
              setConnectorDialogOpen(open)
              if (!open) setEditingConnector(undefined)
            }}
            onSaved={(saved, shouldProbe) => {
              setConnectorDialogOpen(false)
              setEditingConnector(undefined)
              void queryClient.invalidateQueries({
                queryKey: ['studio-operations', 'connectors'],
              })
              toast.success(editingConnector ? '连接器已更新' : '连接器已添加')
              if (shouldProbe) savedConnectorProbe.mutate(saved.id)
            }}
          />
        ) : null}
    </>
  )

  return contentOnly ? content : (
    <StudioSettingsLayout title='设置' sections={settingSections} section={section} onSectionChange={onSectionChange}>
      {content}
    </StudioSettingsLayout>
  )
}

function ConnectorSettings({
  connectors,
  loading,
  error,
  onEdit,
}: {
  connectors: StudioMCPConnector[]
  loading: boolean
  error: boolean
  onEdit: (connector: StudioMCPConnector) => void
}) {
  const queryClient = useQueryClient()
  const update = useMutation({
    mutationFn: updateStudioConnector,
    onSuccess: () =>
      queryClient.invalidateQueries({ queryKey: ['studio-operations', 'connectors'] }),
    onError: (cause) =>
      toast.error('更新连接器失败', {
        description: connectorErrorMessage(cause),
      }),
  })
  const probe = useMutation({
    mutationFn: probeStudioConnector,
    onSuccess: (connector) => {
      void queryClient.invalidateQueries({ queryKey: ['studio-operations', 'connectors'] })
      toast.success(`已发现 ${connector.tools?.length ?? 0} 个工具`)
    },
    onError: (cause) => {
      void queryClient.invalidateQueries({ queryKey: ['studio-operations', 'connectors'] })
      toast.error('发现工具失败', {
        description: connectorErrorMessage(cause),
      })
    },
  })
  return (
    <div className='mx-auto flex w-full max-w-4xl flex-1 flex-col gap-4'>
      {loading ? (
        <p className='text-sm text-muted-foreground'>正在读取连接器…</p>
      ) : null}
      {error ? (
        <p role='alert' className='text-sm text-destructive'>
          连接器读取失败，刷新后重试。
        </p>
      ) : null}
      {!loading && !error && connectors.length === 0 ? (
        <SettingsEmpty
          icon={Cable}
          title='还没有 MCP 连接器'
          description='添加后可集中管理其可用性和调用策略。'
        />
      ) : null}
      {connectors.length > 0 ? (
        <div className='overflow-hidden rounded-lg border bg-background'>
          {connectors.map((connector) => {
            const status = connectorStatus(connector)
            return (
              <div
                key={connector.id}
                className='flex flex-wrap items-center gap-4 border-b p-4 last:border-b-0'
              >
                <div className='min-w-0 flex-1'>
                  <div className='flex items-center gap-2'>
                    <p className='truncate text-sm font-medium'>
                      {connector.name}
                    </p>
                    <Badge variant='outline'>
                      <StatusDot state={status.state} label={status.reason} />
                      {status.text}
                    </Badge>
                  </div>
                  <p className='mt-1 truncate text-xs text-muted-foreground'>
                    {connector.url}
                  </p>
                  <p className='mt-1 text-xs text-muted-foreground'>
                    调用策略：{connectorPolicyLabel(connector.policy)} · 凭据：
                    {connector.credential_masked} · 已发现工具：
                    {connector.tools?.length ?? 0}
                  </p>
                </div>
                <div className='flex items-center gap-2'>
                  <Button
                    variant='outline'
                    size='sm'
                    onClick={() => onEdit(connector)}
                  >
                    编辑
                  </Button>
                  <Button
                    variant='outline'
                    size='sm'
                    disabled={probe.isPending}
                    onClick={() => probe.mutate(connector.id)}
                  >
                    {probe.isPending && probe.variables === connector.id
                      ? '正在发现…'
                      : '发现工具'}
                  </Button>
                  <Switch
                    aria-label={`启用 ${connector.name}`}
                    checked={connector.enabled}
                    disabled={update.isPending}
                    onCheckedChange={(enabled) =>
                      update.mutate({
                        id: connector.id,
                        name: connector.name,
                        url: connector.url,
                        enabled,
                        policy: connector.policy,
                      })
                    }
                  />
                </div>
              </div>
            )
          })}
        </div>
      ) : null}
    </div>
  )
}

function connectorPolicyLabel(policy: StudioConnectorPolicy) {
  switch (policy) {
    case 'auto':
      return '自动执行'
    case 'approval':
      return '请求确认'
    case 'forbidden':
      return '禁止调用'
  }
}

function connectorErrorMessage(cause: unknown) {
  if (cause instanceof ApiError) return cause.detail ?? cause.message
  return cause instanceof Error ? cause.message : '连接器操作失败'
}

function ConnectorDialog({
  connector,
  open,
  onOpenChange,
  onSaved,
}: {
  connector?: StudioMCPConnector
  open: boolean
  onOpenChange: (open: boolean) => void
  onSaved: (connector: StudioMCPConnector, shouldProbe: boolean) => void
}) {
  const queryClient = useQueryClient()
  const [form, setForm] = useState({
    name: connector?.name ?? '',
    url: connector?.url ?? '',
    credential: '',
    enabled: connector?.enabled ?? true,
    policy: connector?.policy ?? ('approval' as StudioConnectorPolicy),
  })
  const [tools, setTools] = useState<StudioMCPTool[]>(connector?.tools ?? [])
  const discover = useMutation({
    mutationFn: () => {
      if (
        connector &&
        form.url.trim() === connector.url &&
        !form.credential.trim()
      ) {
        return probeStudioConnector(connector.id).then(
          (result) => result.tools ?? []
        )
      }
      return discoverStudioConnector({
        connectorId: connector?.id,
        url: form.url.trim(),
        credential: form.credential,
      })
    },
    onSuccess: (result) => {
      setTools(result)
      void queryClient.invalidateQueries({ queryKey: ['studio-operations', 'connectors'] })
      toast.success(`已发现 ${result.length} 个工具`)
    },
    onError: (cause) => {
      setTools([])
      void queryClient.invalidateQueries({ queryKey: ['studio-operations', 'connectors'] })
      toast.error('发现工具失败', {
        description: connectorErrorMessage(cause),
      })
    },
  })
  const save = useMutation({
    mutationFn: () => {
      const input = { ...form, name: form.name.trim(), url: form.url.trim() }
      return connector
        ? updateStudioConnector({ id: connector.id, ...input })
        : createStudioConnector(input)
    },
    onSuccess: (saved) =>
      onSaved(
        saved,
        !connector ||
          form.url.trim() !== connector.url ||
          form.credential.trim() !== ''
      ),
    onError: (cause) =>
      toast.error('保存连接器失败', {
        description: connectorErrorMessage(cause),
      }),
  })
  return (
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => {
        if (!save.isPending && !discover.isPending) onOpenChange(nextOpen)
      }}
    >
      <DialogContent className='sm:max-w-xl'>
        <DialogHeader>
          <DialogTitle>
            {connector ? '编辑 MCP 连接器' : '添加 MCP 连接器'}
          </DialogTitle>
          <DialogDescription>
            {connector
              ? '留空访问凭据将保留当前凭据。'
              : '使用 Streamable HTTP 地址。凭据创建后不会再次显示。'}
          </DialogDescription>
        </DialogHeader>
        <form
          className='grid gap-4'
          onSubmit={(event) => {
            event.preventDefault()
            save.mutate()
          }}
        >
          <div className='grid gap-2'>
            <Label htmlFor='studio-connector-name'>名称</Label>
            <Input
              id='studio-connector-name'
              required
              value={form.name}
              onChange={(event) =>
                setForm({ ...form, name: event.target.value })
              }
              placeholder='例如：内部知识库'
            />
          </div>
          <div className='grid gap-2'>
            <Label htmlFor='studio-connector-url'>服务地址</Label>
            <Input
              id='studio-connector-url'
              required
              type='url'
              disabled={discover.isPending || save.isPending}
              value={form.url}
              onChange={(event) => {
                setForm({ ...form, url: event.target.value })
                setTools([])
              }}
              placeholder='https://mcp.example.com'
            />
          </div>
          <div className='grid gap-2'>
            <Label htmlFor='studio-connector-credential'>访问凭据</Label>
            <Input
              id='studio-connector-credential'
              required={!connector}
              type='password'
              autoComplete='new-password'
              disabled={discover.isPending || save.isPending}
              placeholder={connector ? '留空保持当前凭据' : undefined}
              value={form.credential}
              onChange={(event) => {
                setForm({ ...form, credential: event.target.value })
                setTools([])
              }}
            />
          </div>
          <div className='grid gap-2'>
            <Label>调用策略</Label>
            <Select
              value={form.policy}
              onValueChange={(policy: StudioConnectorPolicy) =>
                setForm({ ...form, policy })
              }
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value='approval'>请求确认</SelectItem>
                <SelectItem value='auto'>自动执行</SelectItem>
                <SelectItem value='forbidden'>禁止调用</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <ToggleRow
            label='启用连接器'
            description='关闭后 Agent 不会调用此连接器。'
            checked={form.enabled}
            onCheckedChange={(enabled) => setForm({ ...form, enabled })}
          />
          <div className='flex flex-col gap-2 rounded-md border bg-muted p-3'>
            <div className='flex items-center justify-between gap-3'>
              <Label>已发现工具</Label>
              <Button
                type='button'
                variant='outline'
                size='sm'
                disabled={
                  discover.isPending ||
                  save.isPending ||
                  !form.url.trim() ||
                  (!connector && !form.credential.trim())
                }
                onClick={() => discover.mutate()}
              >
                {discover.isPending ? '正在发现…' : '发现工具'}
              </Button>
            </div>
            {tools.length > 0 ? (
              <ul className='flex max-h-48 flex-col gap-2 overflow-y-auto'>
                {tools.map((tool) => (
                  <li key={tool.name} className='rounded-md border bg-card p-2'>
                    <p className='text-sm font-medium break-all'>{tool.name}</p>
                    {tool.description ? (
                      <p className='text-xs text-muted-foreground'>
                        {tool.description}
                      </p>
                    ) : null}
                  </li>
                ))}
              </ul>
            ) : (
              <p className='text-sm text-muted-foreground'>尚未发现工具</p>
            )}
          </div>
          <DialogFooter>
            <Button
              type='button'
              variant='outline'
              disabled={save.isPending || discover.isPending}
              onClick={() => onOpenChange(false)}
            >
              取消
            </Button>
            <Button
              type='submit'
              disabled={save.isPending || discover.isPending}
            >
              {save.isPending ? '正在保存…' : '保存连接器'}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

function SkillSettings({
  skills,
  loading,
  error,
  disabled,
  onEdit,
}: {
  skills: Awaited<ReturnType<typeof listStudioOperationSkills>>
  loading: boolean
  error: boolean
  disabled: boolean
  onEdit: (skill: StudioSkillSummary) => void
}) {
  const queryClient = useQueryClient()
  const update = useMutation({
    mutationFn: ({ id, enabled }: { id: string; enabled: boolean }) =>
      updateStudioSkillEnabled(id, enabled),
    onSuccess: () =>
      queryClient.invalidateQueries({ queryKey: ['studio-operations', 'skills'] }),
    onError: (cause) =>
      toast.error(cause instanceof Error ? cause.message : '更新启用状态失败'),
  })
  return (
    <div className='mx-auto flex w-full max-w-4xl flex-1 flex-col gap-4'>
      {loading ? (
        <p className='text-sm text-muted-foreground'>正在读取技能…</p>
      ) : null}
      {error ? (
        <p role='alert' className='text-sm text-destructive'>
          技能读取失败，刷新后重试。
        </p>
      ) : null}
      {!loading && !error && skills.length === 0 ? (
        <SettingsEmpty
          icon={Sparkles}
          title='还没有技能'
          description='新建或导入技能后，可在会话中选择。'
        />
      ) : null}
      {skills.length > 0 ? (
        <div className='overflow-hidden rounded-lg border bg-background'>
          {skills.map((skill) => (
            <div
              key={skill.id}
              className='flex flex-wrap items-center gap-4 border-b p-4 last:border-b-0'
            >
              <div className='min-w-0 flex-1'>
                <p className='text-sm font-medium'>{skill.name}</p>
                <LongText className='mt-1 text-xs text-muted-foreground'>
                  {skill.description || '未填写说明'}
                </LongText>
              </div>
              <div className='flex h-8 shrink-0 items-center gap-3'>
                <Button
                  size='sm'
                  variant='outline'
                  disabled={disabled}
                  onClick={() => onEdit(skill)}
                >
                  编辑
                </Button>
                <Switch
                  aria-label={`启用 ${skill.name}`}
                  checked={skill.enabled}
                  disabled={
                    update.isPending && update.variables?.id === skill.id
                  }
                  onCheckedChange={(enabled) =>
                    update.mutate({ id: skill.id, enabled })
                  }
                />
              </div>
            </div>
          ))}
        </div>
      ) : null}
    </div>
  )
}

const initialModel = {
  name: '',
  protocol: 'openai_chat_compatible' as StudioModel['protocol'],
  baseUrl: '',
  model: '',
  apiKey: '',
  contextWindowTokens: '128000',
  maxInputTokens: '120000',
  maxOutputTokens: '8192',
  enabled: true,
  agentEnabled: true,
  supportsTools: true,
  default: false,
  thinkingEnabled: false,
  thinkingEffort: 'medium',
  thinkingBudget: '2048',
  vision: false,
  imageOutput: false,
  streaming: true,
}

function modelConfigInput(form: typeof initialModel): StudioModelConfigInput {
  return {
    name: form.name.trim(),
    protocol: form.protocol,
    baseUrl: form.baseUrl.trim(),
    model: form.model.trim(),
    apiKey: form.apiKey,
    limits: {
      context_window_tokens: Number(form.contextWindowTokens) || 0,
      max_input_tokens: Number(form.maxInputTokens) || 0,
      max_output_tokens: Number(form.maxOutputTokens) || 0,
    },
    enabled: form.enabled,
    agentEnabled: form.agentEnabled,
    default: form.default,
    thinking: form.thinkingEnabled
      ? {
          enabled: true,
          effort: form.thinkingEffort,
          budget_tokens: Number(form.thinkingBudget) || undefined,
        }
      : { enabled: false },
    capabilities: {
      tools: form.supportsTools,
      vision: form.vision,
      image_output: form.imageOutput,
      streaming: form.streaming,
    },
  }
}

function modelToForm(model: StudioModel) {
  return {
    ...initialModel,
    name: model.name,
    protocol: model.protocol,
    baseUrl: model.base_url,
    model: model.model,
    contextWindowTokens: String(model.limits?.context_window_tokens ?? ''),
    maxInputTokens: String(model.limits?.max_input_tokens ?? ''),
    maxOutputTokens: String(model.limits?.max_output_tokens ?? ''),
    enabled: model.enabled,
    agentEnabled: model.agent_enabled,
    supportsTools: model.capabilities.tools,
    default: model.default,
    thinkingEnabled: model.thinking.enabled,
    thinkingEffort: model.thinking.effort ?? 'medium',
    thinkingBudget: String(model.thinking.budget_tokens ?? 2048),
    vision: model.capabilities.vision,
    imageOutput: model.capabilities.image_output,
    streaming: model.capabilities.streaming,
  }
}

function ModelDialog({
  model,
  open,
  onOpenChange,
}: {
  model?: StudioModel
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const queryClient = useQueryClient()
  const formRef = useRef<HTMLFormElement>(null)
  const [form, setForm] = useState(() =>
    model ? modelToForm(model) : initialModel
  )
  const updateForm = (patch: Partial<typeof initialModel>) => {
    setForm((current) => ({ ...current, ...patch }))
  }
  const endpointPlaceholder =
    form.protocol === 'openai_responses'
      ? 'https://api.example.com/v1/responses'
      : form.protocol === 'anthropic_messages_compatible'
        ? 'https://api.example.com/v1/messages'
        : 'https://api.example.com/v1/chat/completions'
  const save = useMutation({
    mutationFn: () => {
      const input = modelConfigInput(form)
      return model
        ? updateStudioModel(model.id, input)
        : createStudioModel(input)
    },
    onSuccess: async () => {
      await invalidateStudioModels(queryClient)
      onOpenChange(false)
      toast.success(model ? '模型已更新' : '模型已添加')
    },
    onError: (cause) => {
      const message =
        cause instanceof Error
          ? cause.message
          : model
            ? '更新模型失败，请稍后重试。'
            : '添加模型失败，请稍后重试。'
      toast.error(message)
    },
  })
  const test = useMutation({
    mutationFn: () => {
      const input = modelConfigInput(form)
      return testStudioModelConfig(
        model && !input.apiKey ? { ...input, existingModelId: model.id } : input
      )
    },
    onSuccess: (result) => toast.success(`测试成功 · ${result.latency_ms} ms`),
    onError: (cause) => {
      const message =
        cause instanceof Error ? cause.message : '模型连接失败，请检查配置。'
      toast.error(message)
    },
  })
  const testCurrentConfig = () => {
    if (!formRef.current?.reportValidity()) return
    test.mutate()
  }
  const submit = (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    save.mutate()
  }
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='max-h-[calc(100vh-2rem)] overflow-y-auto sm:max-w-[776px]'>
        <DialogHeader>
          <DialogTitle>{model ? '编辑模型' : '添加模型'}</DialogTitle>
          <DialogDescription className='sr-only'>
            配置模型连接信息
          </DialogDescription>
        </DialogHeader>
        <form ref={formRef} className='grid gap-5' onSubmit={submit}>
          <h3 className='text-sm font-semibold'>连接信息</h3>
          <div className='grid gap-2'>
            <Label htmlFor='studio-model-name'>名称</Label>
            <Input
              id='studio-model-name'
              required
              value={form.name}
              onChange={(event) => updateForm({ name: event.target.value })}
              placeholder='例如：我的 Claude'
            />
          </div>
          <div className='grid gap-4 md:grid-cols-[17rem_minmax(0,1fr)]'>
            <div className='grid min-w-0 gap-2'>
              <Label>接口协议</Label>
              <Select
                value={form.protocol}
                onValueChange={(protocol: StudioModel['protocol']) =>
                  updateForm({ protocol })
                }
              >
                <SelectTrigger className='w-full'>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value='openai_chat_compatible'>
                    OpenAI Chat Compatible
                  </SelectItem>
                  <SelectItem value='openai_responses'>
                    OpenAI Responses
                  </SelectItem>
                  <SelectItem value='anthropic_messages_compatible'>
                    Anthropic Messages Compatible
                  </SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div className='grid min-w-0 gap-2'>
              <Label htmlFor='studio-model-url'>接口地址</Label>
              <Input
                id='studio-model-url'
                required
                type='url'
                value={form.baseUrl}
                onChange={(event) =>
                  updateForm({ baseUrl: event.target.value })
                }
                placeholder={endpointPlaceholder}
              />
            </div>
          </div>
          <div className='grid gap-2'>
            <Label htmlFor='studio-model-id'>模型 ID</Label>
            <Input
              id='studio-model-id'
              required
              value={form.model}
              onChange={(event) => updateForm({ model: event.target.value })}
              placeholder='例如：claude-sonnet-4-5'
            />
          </div>
          <div className='grid gap-2'>
            <Label htmlFor='studio-model-key'>API Key</Label>
            <Input
              id='studio-model-key'
              required={!model?.has_api_key}
              type='password'
              autoComplete='new-password'
              value={form.apiKey}
              onChange={(event) => updateForm({ apiKey: event.target.value })}
              placeholder={
                model?.has_api_key
                  ? '已保存 API Key；留空保持不变'
                  : '请输入 API Key'
              }
            />
          </div>
          <h3 className='border-t pt-4 text-sm font-semibold'>使用设置</h3>
          <div className='grid gap-3 rounded-lg border bg-muted/30 p-3'>
            <ToggleRow
              label='设为默认模型'
              checked={form.default}
              onCheckedChange={(value) => updateForm({ default: value })}
            />
            <ToggleRow
              label='允许 Agent 使用'
              description={
                form.supportsTools
                  ? '让此模型出现在会话的模型列表中。'
                  : '先确认模型支持工具调用。'
              }
              checked={form.agentEnabled}
              disabled={!form.supportsTools}
              onCheckedChange={(agentEnabled) => updateForm({ agentEnabled })}
            />
          </div>
          <h3 className='border-t pt-4 text-sm font-semibold'>Token 限额</h3>
          <div className='grid gap-3 sm:grid-cols-3'>
            <div className='grid gap-2'>
              <Label htmlFor='studio-model-context-window'>
                上下文窗口 Token
              </Label>
              <Input
                id='studio-model-context-window'
                required
                min={1}
                type='number'
                inputMode='numeric'
                value={form.contextWindowTokens}
                onChange={(event) =>
                  updateForm({ contextWindowTokens: event.target.value })
                }
              />
            </div>
            <div className='grid gap-2'>
              <Label htmlFor='studio-model-max-input'>最大输入 Token</Label>
              <Input
                id='studio-model-max-input'
                required
                min={1}
                type='number'
                inputMode='numeric'
                value={form.maxInputTokens}
                onChange={(event) =>
                  updateForm({ maxInputTokens: event.target.value })
                }
              />
            </div>
            <div className='grid gap-2'>
              <Label htmlFor='studio-model-max-output'>最大输出 Token</Label>
              <Input
                id='studio-model-max-output'
                required
                min={1}
                type='number'
                inputMode='numeric'
                value={form.maxOutputTokens}
                onChange={(event) =>
                  updateForm({ maxOutputTokens: event.target.value })
                }
              />
            </div>
          </div>
          <h3 className='border-t pt-4 text-sm font-semibold'>模型能力</h3>
          <div className='grid gap-3 rounded-lg border bg-muted/30 p-3'>
            <ToggleRow
              label='支持工具调用'
              description='开启后，模型可以调用 Agent 工具。'
              checked={form.supportsTools}
              onCheckedChange={(supportsTools) =>
                updateForm({
                  supportsTools,
                  agentEnabled: supportsTools ? form.agentEnabled : false,
                })
              }
            />
            <ToggleRow
              label='支持图片输入'
              description='在会话中发送图片，让模型分析画面内容。'
              checked={form.vision}
              onCheckedChange={(vision) => updateForm({ vision })}
            />
            <ToggleRow
              label='支持图片输出'
              description='让模型生成插画、配图等图片内容。'
              checked={form.imageOutput}
              onCheckedChange={(imageOutput) => updateForm({ imageOutput })}
            />
            <ToggleRow
              label='支持流式响应'
              description='回复边生成边显示，减少等待。'
              checked={form.streaming}
              onCheckedChange={(streaming) => updateForm({ streaming })}
            />
            <ToggleRow
              label='启用思考'
              description='让模型先思考再回答，更好地处理复杂问题和多步骤任务。'
              checked={form.thinkingEnabled}
              onCheckedChange={(thinkingEnabled) =>
                updateForm({ thinkingEnabled })
              }
            />
            {form.thinkingEnabled ? (
              <div className='grid grid-cols-2 gap-3 pt-1'>
                <div className='grid gap-2'>
                  <Label htmlFor='studio-model-effort'>思考强度</Label>
                  <Select
                    value={form.thinkingEffort}
                    onValueChange={(thinkingEffort) =>
                      updateForm({ thinkingEffort })
                    }
                  >
                    <SelectTrigger id='studio-model-effort'>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value='low'>低</SelectItem>
                      <SelectItem value='medium'>中</SelectItem>
                      <SelectItem value='high'>高</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
                <div className='grid gap-2'>
                  <Label htmlFor='studio-model-budget'>Token 预算</Label>
                  <Input
                    id='studio-model-budget'
                    inputMode='numeric'
                    value={form.thinkingBudget}
                    onChange={(event) =>
                      updateForm({ thinkingBudget: event.target.value })
                    }
                  />
                </div>
              </div>
            ) : null}
          </div>
          <DialogFooter className='gap-2 sm:justify-between'>
            <div className='flex min-w-0 items-center gap-2'>
              <Button
                type='button'
                variant='outline'
                disabled={test.isPending || save.isPending}
                onClick={testCurrentConfig}
              >
                {test.isPending ? '正在测试…' : '测试配置'}
              </Button>
            </div>
            <div className='flex items-center gap-2'>
              <Button
                type='button'
                variant='outline'
                onClick={() => onOpenChange(false)}
              >
                取消
              </Button>
              <Button type='submit' disabled={save.isPending || test.isPending}>
                {save.isPending ? '正在保存…' : model ? '保存修改' : '保存模型'}
              </Button>
            </div>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

function ToggleRow({
  label,
  description,
  checked,
  disabled = false,
  onCheckedChange,
}: {
  label: string
  description?: string
  checked: boolean
  disabled?: boolean
  onCheckedChange: (checked: boolean) => void
}) {
  return (
    <div className='flex items-center justify-between gap-3'>
      <div>
        <p className='text-sm font-medium'>{label}</p>
        {description ? (
          <p className='text-xs text-muted-foreground'>{description}</p>
        ) : null}
      </div>
      <Switch
        aria-label={label}
        checked={checked}
        disabled={disabled}
        onCheckedChange={onCheckedChange}
      />
    </div>
  )
}

function SettingsEmpty({
  icon: Icon,
  title,
  description,
}: {
  icon: typeof BrainCircuit
  title: string
  description: string
}) {
  return (
    <Empty className='border bg-muted/10'>
      <EmptyHeader>
        <EmptyMedia variant='icon'>
          <Icon />
        </EmptyMedia>
        <EmptyTitle>{title}</EmptyTitle>
        <EmptyDescription>{description}</EmptyDescription>
      </EmptyHeader>
    </Empty>
  )
}
