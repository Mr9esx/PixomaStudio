import { Fragment, useEffect, useMemo, useRef, useState } from 'react'
import {
  DndContext, KeyboardSensor, PointerSensor, closestCenter, useSensor, useSensors,
  type DragEndEvent, type DraggableAttributes, type DraggableSyntheticListeners,
} from '@dnd-kit/core'
import {
  SortableContext, arrayMove, sortableKeyboardCoordinates, useSortable, verticalListSortingStrategy,
} from '@dnd-kit/sortable'
import { CSS } from '@dnd-kit/utilities'
import { ChevronsUpDown, CircleHelp, GripVertical, Plus, RefreshCw, Trash2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { cn } from '@/lib/utils'
import { RequiredBadge } from '@/components/required-badge'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import {
  Command, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList, CommandSeparator,
} from '@/components/ui/command'
import { Input } from '@/components/ui/input'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { ConfirmDialog } from '@/components/confirm-dialog'
import type { InputFieldDraft, OutputFieldDraft } from '../lib/derive'
import { inputKindFor, outputKindFor, nodeTitle, nodeVisualFor } from '../lib/node-catalog'
import type { WorkflowNode } from '../lib/workflow-parse'

const INPUT_TYPES: Array<{ value: string; labelKey: string }> = [
  { value: 'string', labelKey: 'cases.typeString' },
  { value: 'image', labelKey: 'cases.typeImage' },
  { value: 'audio', labelKey: 'cases.typeAudio' },
  { value: 'video', labelKey: 'cases.typeVideo' },
  { value: 'number', labelKey: 'cases.typeNumber' },
  { value: 'boolean', labelKey: 'cases.typeBoolean' },
  { value: 'enum', labelKey: 'cases.typeEnum' },
  { value: 'file', labelKey: 'cases.typeFile' },
]
const TYPE_LABEL_KEYS: Record<string, string> = {
  string: 'cases.typeString', image: 'cases.typeImage', audio: 'cases.typeAudio',
  video: 'cases.typeVideo', number: 'cases.typeNumber', boolean: 'cases.typeBoolean',
  enum: 'cases.typeEnum', text: 'cases.typeText', file: 'cases.typeFile',
}

function safeAutoType(classType: string, fieldPath: string): string {
  const kind = inputKindFor(classType, fieldPath)
  return kind === 'unknown' ? 'string' : kind
}

function OutputTypeBadge({ node, type }: { node?: WorkflowNode; type: string }) {
  const { t } = useTranslation()
  const labelKey = node
    ? (TYPE_LABEL_KEYS[outputKindFor(node.class_type)] ?? 'cases.typeFile')
    : (TYPE_LABEL_KEYS[type] ?? 'cases.typeFile')
  return <Badge data-testid='output-type' variant='secondary'>{t(labelKey)}</Badge>
}

function RequiredColumnLabel({ children }: { children: React.ReactNode }) {
  return <span className='inline-flex items-center gap-1.5'>{children}<RequiredBadge /></span>
}

function RequiredHint() {
  const { t } = useTranslation()
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button type='button' variant='ghost' size='icon-sm' aria-label={t('cases.fieldRequired')}>
          <CircleHelp />
        </Button>
      </TooltipTrigger>
      <TooltipContent side='top' sideOffset={6} className='max-w-64'>{t('cases.fieldRequiredHint')}</TooltipContent>
    </Tooltip>
  )
}

type BindNodePopoverProps = {
  nodes: WorkflowNode[]
  mode?: 'input' | 'output'
  node: WorkflowNode | undefined
  boundLabel?: string
  onPick: (nodeId: string, pick: string) => void
  disabled?: boolean
  compact?: boolean
}

function BindNodePopover({ nodes, mode = 'input', node, boundLabel, onPick, disabled, compact }: BindNodePopoverProps) {
  const { t } = useTranslation()
  const [search, setSearch] = useState('')
  const [open, setOpen] = useState(false)
  const bound = !!node
  const typeLabel = (() => {
    if (!node) return undefined
    if (mode === 'input') {
      const p = node.inputs.find((i) => i.name === boundLabel && !i.ref)
      return p ? t(TYPE_LABEL_KEYS[p.kind] ?? 'cases.typeString') : undefined
    }
    return t(TYPE_LABEL_KEYS[outputKindFor(node.class_type)] ?? 'cases.typeFile')
  })()
  const triggerText = bound && node
    ? [nodeTitle(node), boundLabel, typeLabel].filter(Boolean).join(' · ')
    : undefined
  const query = search.trim().toLowerCase()
  const inputGroups = useMemo(
    () => nodes.map((n) => ({ node: n, params: n.inputs.filter((i) => !i.ref) })).filter((g) => g.params.length > 0),
    [nodes]
  )
  const outputGroups = useMemo(() => nodes.filter((n) => n.outputCount > 0), [nodes])

  function pick(nodeId: string, pick: string) {
    onPick(nodeId, pick)
    setSearch('')
    setOpen(false)
  }

  function itemValue(n: WorkflowNode, field: string) {
    return `${nodeTitle(n)} ${n.id} ${field}`
  }

  function nodeHead(n: WorkflowNode) {
    const { Icon, className } = nodeVisualFor(n.class_type)
    return (
      <span className='flex items-center gap-1.5'>
        <span className={cn('grid size-5 shrink-0 place-items-center rounded-sm', className)}><Icon className='size-3' /></span>
        <span className='truncate font-semibold'>{nodeTitle(n)}</span>
        <span className='font-mono text-xs font-normal text-muted-foreground'>#{n.id}</span>
      </span>
    )
  }

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <Tooltip>
        <TooltipTrigger asChild>
          <PopoverTrigger asChild>
            <Button
              type='button' variant='outline' size='sm'
              disabled={disabled || nodes.length === 0} aria-label={t('cases.fieldBind')}
              className={cn('flex w-full items-center justify-between text-left', compact ? 'gap-1.5 px-2' : 'gap-2 px-3')}
            >
              {bound ? (
                <span className='flex min-w-0 flex-1 items-center gap-1.5'>
                  {(() => {
                    const { Icon, className } = nodeVisualFor(node.class_type)
                    return <span className={cn('grid size-5 shrink-0 place-items-center rounded-sm', className)}><Icon className='size-3' /></span>
                  })()}
                  <span className='min-w-0 truncate'>
                    <span className='font-normal'>{nodeTitle(node)}</span>
                    {boundLabel ? <span className='font-mono text-muted-foreground'> · {boundLabel}</span> : null}
                  </span>
                  {mode === 'output' && typeLabel ? <Badge data-testid='selected-output-type' variant='secondary' className='ms-auto'>{typeLabel}</Badge> : null}
                </span>
              ) : (
                <span className='min-w-0 truncate text-sm font-normal text-muted-foreground'>
                  {nodes.length === 0 ? t('cases.emptyWorkflowLock') : t('cases.bindPickPlaceholder')}
                </span>
              )}
              <ChevronsUpDown className='size-3.5 shrink-0 text-muted-foreground' />
            </Button>
          </PopoverTrigger>
        </TooltipTrigger>
        {bound && triggerText ? <TooltipContent sideOffset={6}>{triggerText}</TooltipContent> : null}
      </Tooltip>
      <PopoverContent align='start' side='bottom' className='w-80 p-0'>
        <Command>
          <CommandInput autoFocus value={search} onValueChange={setSearch} placeholder={t('cases.bindSearchPlaceholder')} autoComplete='off' />
          <CommandList onWheel={(e) => {
            const el = e.currentTarget
            const max = el.scrollHeight - el.clientHeight
            const next = el.scrollTop + e.deltaY
            el.scrollTop = Math.max(0, Math.min(next, max))
            e.preventDefault()
          }}>
            <CommandEmpty className='py-4 text-center text-sm'>{query ? t('cases.bindNoResults') : t('cases.bindNoParams')}</CommandEmpty>
            {mode === 'output' ? outputGroups.map((n, gi) => (
              <Fragment key={n.id}>
                {gi > 0 ? <CommandSeparator /> : null}
                <CommandGroup heading={nodeHead(n)}>
                  {n.outputCount > 1 ? Array.from({ length: n.outputCount }, (_, i) => (
                    <CommandItem key={i} value={itemValue(n, String(i))} onSelect={() => pick(n.id, String(i))} className='text-sm'>
                      <span className='font-mono'>{i}</span>
                      <span className='ms-auto shrink-0 rounded-sm bg-muted px-1.5 py-0.5 text-xs whitespace-nowrap text-muted-foreground'>{t(TYPE_LABEL_KEYS[outputKindFor(n.class_type)])}</span>
                    </CommandItem>
                  )) : (
                    <CommandItem value={itemValue(n, '0')} onSelect={() => pick(n.id, '0')} className='text-sm'>
                      <span className='truncate'>{t(TYPE_LABEL_KEYS[outputKindFor(n.class_type)])}</span>
                    </CommandItem>
                  )}
                </CommandGroup>
              </Fragment>
            )) : inputGroups.map(({ node: n, params }, gi) => (
              <Fragment key={n.id}>
                {gi > 0 ? <CommandSeparator /> : null}
                <CommandGroup heading={nodeHead(n)}>
                  {params.map((p) => (
                    <CommandItem key={p.name} value={itemValue(n, p.name)} onSelect={() => pick(n.id, p.name)} className='text-sm'>
                      <span className='min-w-0 flex-1 truncate font-mono'>{p.name}</span>
                      <span className='ms-auto shrink-0 rounded-sm bg-muted px-1.5 py-0.5 text-xs whitespace-nowrap text-muted-foreground'>{t(TYPE_LABEL_KEYS[p.kind] ?? 'cases.typeString')}</span>
                    </CommandItem>
                  ))}
                </CommandGroup>
              </Fragment>
            ))}
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  )
}

type RemoveFieldButtonProps = { fieldKey: string; disabled?: boolean; onRemove: () => void; compact?: boolean }

function RemoveFieldButton({ fieldKey, disabled, onRemove, compact }: RemoveFieldButtonProps) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  return (
    <>
      <Button type='button' variant='ghost' size='icon-sm' disabled={disabled} aria-label={t('cases.removeRow')}
        className={cn('text-muted-foreground hover:text-destructive', compact && 'shrink-0')} onClick={() => setOpen(true)}>
        <Trash2 className={compact ? 'size-3.5' : 'size-4'} /><span className='sr-only'>{t('cases.removeRow')}</span>
      </Button>
      <ConfirmDialog open={open} onOpenChange={setOpen} destructive title={t('cases.deleteFieldTitle')}
        desc={t('cases.deleteFieldBody', { key: fieldKey })} confirmText={t('common.delete')} cancelBtnText={t('common.cancel')} handleConfirm={onRemove} />
    </>
  )
}

function RestoreTypeButton({ onClick, disabled }: { onClick: () => void; disabled?: boolean }) {
  const { t } = useTranslation()
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button type='button' variant='ghost' size='icon-sm' onClick={onClick} disabled={disabled} aria-label={t('cases.restoreSuggestedType')} className='shrink-0'>
          <RefreshCw className='size-3.5' />
        </Button>
      </TooltipTrigger>
      <TooltipContent sideOffset={6}>{t('cases.restoreSuggestedType')}</TooltipContent>
    </Tooltip>
  )
}

type InputCardProps = {
  nodes: WorkflowNode[]
  value: InputFieldDraft
  onChange: (next: InputFieldDraft) => void
  onRemove: () => void
  disabled?: boolean
  hasError?: boolean
  setNodeRef?: (el: HTMLLIElement | null) => void
  dragHandleRef?: (el: HTMLButtonElement | null) => void
  dragAttributes?: DraggableAttributes
  dragListeners?: DraggableSyntheticListeners
  dragStyle?: React.CSSProperties
  isDragging?: boolean
  showDragHint?: boolean
  hintOpen?: boolean
  onHintOpenChange?: (open: boolean) => void
}

function InputFieldCard({ nodes, value, onChange, onRemove, disabled, hasError, setNodeRef, dragHandleRef,
  dragAttributes, dragListeners, dragStyle, isDragging, showDragHint, hintOpen, onHintOpenChange }: InputCardProps) {
  const { t } = useTranslation()
  const node = nodes.find((n) => n.id === value.node_id)
  const bound = !!(value.node_id && value.field_path)
  const autoType = bound && node ? safeAutoType(node.class_type, value.field_path) : ''
  const isCustom = bound && value.type !== autoType
  const dragButton = (
    <Button ref={dragHandleRef} type='button' size='icon-sm' variant='ghost' disabled={disabled}
      aria-label={t('a11y.reorder')} className='cursor-grab touch-none text-muted-foreground hover:bg-transparent' {...dragAttributes} {...dragListeners}>
      <GripVertical className='size-4' />
    </Button>
  )
  return (
    <li ref={setNodeRef} data-testid='input-field-card' style={dragStyle} className={cn('flex flex-col gap-2 rounded-md border p-3', isDragging && 'opacity-50')}>
      {dragListeners ? showDragHint ? (
        <Tooltip open={hintOpen} onOpenChange={onHintOpenChange}>
          <TooltipTrigger asChild>{dragButton}</TooltipTrigger>
          <TooltipContent side='top' sideOffset={8}>{t('a11y.reorder')}</TooltipContent>
        </Tooltip>
      ) : dragButton : null}
      <div className='flex flex-wrap items-center gap-2'>
        <span className='text-sm text-foreground'><RequiredColumnLabel>{t('cases.fieldKey')}</RequiredColumnLabel></span>
        <Input className={cn('h-8 w-36', hasError && 'border-destructive focus-visible:ring-destructive/30')}
          value={value.key} onChange={(e) => onChange({ ...value, key: e.target.value })} disabled={disabled}
          autoComplete='off' aria-label={t('cases.fieldKey')} aria-invalid={hasError || undefined} />
      </div>
      <div className='flex items-center gap-2'>
        <span className='shrink-0 text-sm text-foreground'>{t('cases.fieldDescription')}</span>
        <Input className='h-8 flex-1' value={value.description ?? ''} onChange={(e) => onChange({ ...value, description: e.target.value })}
          disabled={disabled} autoComplete='off' aria-label={t('cases.fieldDescription')} />
      </div>
      <div>
        <span className='text-sm text-foreground'><RequiredColumnLabel>{t('cases.fieldBind')}</RequiredColumnLabel></span>
        <BindNodePopover mode='input' nodes={nodes} node={bound ? node : undefined} boundLabel={value.field_path} disabled={disabled}
          onPick={(nodeId, fieldPath) => {
            const picked = nodes.find((n) => n.id === nodeId)
            onChange({ ...value, node_id: nodeId, field_path: fieldPath, type: picked ? safeAutoType(picked.class_type, fieldPath) : value.type })
          }} />
      </div>
      <div className='flex flex-wrap items-center gap-2'>
        <span className='text-sm text-foreground'><RequiredColumnLabel>{t('cases.fieldType')}</RequiredColumnLabel></span>
        <Select value={value.type} onValueChange={(type) => onChange({ ...value, type })} disabled={disabled}>
          <SelectTrigger size='sm' className='w-28' aria-label={t('cases.fieldType')}><SelectValue /></SelectTrigger>
          <SelectContent><SelectGroup>{INPUT_TYPES.map((type) => <SelectItem key={type.value} value={type.value}>{t(type.labelKey)}</SelectItem>)}</SelectGroup></SelectContent>
        </Select>
        {bound && isCustom ? <RestoreTypeButton disabled={disabled} onClick={() => onChange({ ...value, type: autoType })} /> : null}
        {bound && !isCustom ? <>
          <Badge variant='secondary'>{t('cases.typeAuto')}</Badge>
          <span className='text-xs text-muted-foreground'>{t('cases.typeAutoSource', { node: nodeTitle(node ?? { class_type: '' }), param: value.field_path })}</span>
        </> : null}
      </div>
      {value.type === 'enum' ? (
        <div className='flex items-center gap-2'>
          <span className='text-sm text-foreground'>{t('cases.fieldEnumOptions')}</span>
          <Input className='h-8 flex-1' value={(value.enum_values ?? []).join(', ')}
            onChange={(e) => onChange({ ...value, enum_values: e.target.value.split(',').map((s) => s.trim()).filter(Boolean) })}
            disabled={disabled} autoComplete='off' aria-label={t('cases.fieldEnumOptions')} />
        </div>
      ) : null}
      <div className='flex items-center justify-between border-t pt-2'>
        <label className='flex items-center gap-1.5 text-sm'>
          <Checkbox checked={value.required} onCheckedChange={(v) => onChange({ ...value, required: v === true })} disabled={disabled} />
          {t('cases.fieldRequired')}<RequiredHint />
        </label>
        <RemoveFieldButton fieldKey={value.key} disabled={disabled} onRemove={onRemove} />
      </div>
    </li>
  )
}

type OutputCardProps = {
  nodes: WorkflowNode[]; value: OutputFieldDraft; onChange: (next: OutputFieldDraft) => void; onRemove: () => void; disabled?: boolean
}

function OutputFieldCard({ nodes, value, onChange, onRemove, disabled }: OutputCardProps) {
  const { t } = useTranslation()
  const node = nodes.find((n) => n.id === value.node_id)
  const bound = !!value.node_id
  return (
    <li data-testid='output-field-card' className='flex flex-col gap-2 rounded-md border p-3'>
      <div className='flex flex-wrap items-center gap-2'>
        <span className='text-sm text-foreground'><RequiredColumnLabel>{t('cases.fieldKey')}</RequiredColumnLabel></span>
        <Input className='h-8 w-36' value={value.key} onChange={(e) => onChange({ ...value, key: e.target.value })}
          disabled={disabled} autoComplete='off' aria-label={t('cases.fieldKey')} />
        <span className='text-sm text-foreground'>{t('cases.fieldType')}</span>
        <OutputTypeBadge node={bound ? node : undefined} type={value.type} />
      </div>
      <div className='flex items-center gap-2'>
        <span className='shrink-0 text-sm text-foreground'>{t('cases.fieldDescription')}</span>
        <Input className='h-8 flex-1' value={value.description ?? ''} onChange={(e) => onChange({ ...value, description: e.target.value })}
          disabled={disabled} autoComplete='off' aria-label={t('cases.fieldDescription')} />
      </div>
      <div>
        <span className='text-sm text-foreground'><RequiredColumnLabel>{t('cases.fieldBind')}</RequiredColumnLabel></span>
        <BindNodePopover mode='output' nodes={nodes} node={bound ? node : undefined}
          boundLabel={node && node.outputCount > 1 ? `[${value.index ?? 0}]` : undefined} disabled={disabled}
          onPick={(nodeId, pick) => {
            const picked = nodes.find((n) => n.id === nodeId)
            onChange({ ...value, node_id: nodeId, index: Number.parseInt(pick, 10) || 0, type: picked ? outputKindFor(picked.class_type) : value.type })
          }} />
      </div>
      <div className='flex justify-end border-t pt-2'><RemoveFieldButton fieldKey={value.key} disabled={disabled} onRemove={onRemove} /></div>
    </li>
  )
}

function SortableInputCard({ id, nodes, value, index, onChange, onRemove, disabled, hasError, hintOpen, onHintOpenChange }: {
  id: number; nodes: WorkflowNode[]; value: InputFieldDraft; index: number
  onChange: (index: number, next: InputFieldDraft) => void; onRemove: (index: number) => void
  disabled?: boolean; hasError?: boolean; hintOpen?: boolean; onHintOpenChange?: (open: boolean) => void
}) {
  const { setNodeRef, setActivatorNodeRef, attributes, listeners, transform, transition, isDragging } = useSortable({ id })
  return <InputFieldCard nodes={nodes} value={value} onChange={(next) => onChange(index, next)} onRemove={() => onRemove(index)}
    disabled={disabled} hasError={hasError} setNodeRef={setNodeRef} dragHandleRef={setActivatorNodeRef}
    dragAttributes={attributes} dragListeners={listeners} dragStyle={{ transform: CSS.Transform.toString(transform), transition }}
    isDragging={isDragging} showDragHint={index === 0} hintOpen={hintOpen} onHintOpenChange={onHintOpenChange} />
}

function SortableInputTableRow({ id, nodes, value, index, onChange, onRemove, disabled, hasError, flash, hintOpen, onHintOpenChange, hideActions }: {
  id: number; nodes: WorkflowNode[]; value: InputFieldDraft; index: number
  onChange: (index: number, next: InputFieldDraft) => void; onRemove: (index: number) => void
  disabled?: boolean; hasError?: boolean; flash?: boolean; hintOpen?: boolean; onHintOpenChange?: (open: boolean) => void; hideActions?: boolean
}) {
  const { t } = useTranslation()
  const { setNodeRef, setActivatorNodeRef, attributes, listeners, transform, transition, isDragging } = useSortable({ id })
  const node = nodes.find((n) => n.id === value.node_id)
  const bound = !!(value.node_id && value.field_path)
  const dragButton = (
    <Button ref={setActivatorNodeRef} type='button' size='icon-sm' variant='ghost' disabled={disabled}
      aria-label={t('a11y.reorder')} className='cursor-grab touch-none text-muted-foreground hover:bg-transparent' {...attributes} {...listeners}>
      <GripVertical className='size-4' />
    </Button>
  )
  return (
    <TableRow ref={setNodeRef} data-testid='input-table-row' style={{ transform: CSS.Transform.toString(transform), transition }}
      className={cn(flash && 'flash-highlight', isDragging && 'opacity-50')}>
      {!hideActions ? <TableCell className='w-10'>
        {index === 0 ? <Tooltip open={hintOpen} onOpenChange={onHintOpenChange}>
          <TooltipTrigger asChild>{dragButton}</TooltipTrigger>
          <TooltipContent side='top' sideOffset={8}>{t('a11y.reorder')}</TooltipContent>
        </Tooltip> : dragButton}
      </TableCell> : null}
      <TableCell>
        <div className='flex w-full items-center gap-1'>
          <Input className={cn('h-8 min-w-0 flex-1', hasError && 'border-destructive focus-visible:ring-destructive/30')}
            value={value.key} onChange={(e) => onChange(index, { ...value, key: e.target.value })} disabled={disabled}
            autoComplete='off' aria-label={t('cases.fieldKey')} aria-invalid={hasError || undefined} />
          {value.required ? <RequiredBadge /> : null}
        </div>
      </TableCell>
      <TableCell className='w-28'>
        <div className='flex w-full items-center gap-1.5'>
          <Select value={value.type} onValueChange={(type) => onChange(index, { ...value, type })} disabled={disabled}>
            <SelectTrigger size='sm' className='w-full min-w-0' aria-label={t('cases.fieldType')}><SelectValue /></SelectTrigger>
            <SelectContent><SelectGroup>{INPUT_TYPES.map((type) => <SelectItem key={type.value} value={type.value}>{t(type.labelKey)}</SelectItem>)}</SelectGroup></SelectContent>
          </Select>
          {bound && value.type !== (node ? safeAutoType(node.class_type, value.field_path) : '') ? (
            <RestoreTypeButton disabled={disabled} onClick={() => onChange(index, { ...value, type: node ? safeAutoType(node.class_type, value.field_path) : value.type })} />
          ) : null}
        </div>
      </TableCell>
      <TableCell className='w-[180px]'>
        <div className='flex w-full items-center gap-2'>
          <BindNodePopover compact mode='input' nodes={nodes} node={bound ? node : undefined} boundLabel={value.field_path} disabled={disabled}
            onPick={(nodeId, fieldPath) => {
              const picked = nodes.find((n) => n.id === nodeId)
              onChange(index, { ...value, node_id: nodeId, field_path: fieldPath, type: picked ? safeAutoType(picked.class_type, fieldPath) : value.type })
            }} />
        </div>
      </TableCell>
      <TableCell className='w-16 text-center'>
        <Checkbox checked={value.required} onCheckedChange={(v) => onChange(index, { ...value, required: v === true })} disabled={disabled} aria-label={t('cases.fieldRequired')} />
      </TableCell>
      <TableCell>
        <Input className='h-8 w-full min-w-0' aria-label={t('cases.fieldDescription')} value={value.description ?? ''}
          onChange={(e) => onChange(index, { ...value, description: e.target.value })} disabled={disabled} autoComplete='off' />
      </TableCell>
      {!hideActions ? <TableCell className='w-10 text-right'><RemoveFieldButton compact fieldKey={value.key} disabled={disabled} onRemove={() => onRemove(index)} /></TableCell> : null}
    </TableRow>
  )
}

type InputTableProps = {
  nodes: WorkflowNode[]; fields: InputFieldDraft[]
  onChange: (index: number, next: InputFieldDraft) => void; onRemove: (index: number) => void
  disabled?: boolean; fieldErrors?: Record<number, boolean>; hintOpen?: boolean
  onHintOpenChange?: (open: boolean) => void; hideActions?: boolean
}

function useNewRowFlash(count: number) {
  const scrollRef = useRef<HTMLDivElement | null>(null)
  const prevCount = useRef(count)
  const [flashIndex, setFlashIndex] = useState(-1)
  useEffect(() => {
    const prev = prevCount.current
    prevCount.current = count
    if (count > prev && count > 0) {
      const idx = count - 1
      setFlashIndex(idx)
      const scroller = scrollRef.current
      if (scroller) scroller.scrollTo({ top: scroller.scrollHeight, behavior: 'smooth' })
      const timer = window.setTimeout(() => setFlashIndex(-1), 1600)
      return () => window.clearTimeout(timer)
    }
    return undefined
  }, [count])
  return { scrollRef, flashIndex }
}

export function InputFieldsTable({ nodes, fields, onChange, onRemove, disabled, fieldErrors, hintOpen, onHintOpenChange, hideActions }: InputTableProps) {
  const { t } = useTranslation()
  const { scrollRef, flashIndex } = useNewRowFlash(fields.length)
  return (
    <div className='overflow-hidden rounded-lg border bg-background'>
      <Table data-testid='input-fields-table' className='table-fixed' wrapperClassName='max-h-[300px] overflow-y-auto' wrapperRef={scrollRef}>
        <TableHeader className='sticky top-0 z-10 bg-background [&_th]:bg-background'>
          <TableRow>
            {!hideActions ? <TableHead className='w-10' /> : null}
            <TableHead><RequiredColumnLabel>{t('cases.fieldKey')}</RequiredColumnLabel></TableHead>
            <TableHead className='w-28'><RequiredColumnLabel>{t('cases.fieldType')}</RequiredColumnLabel></TableHead>
            <TableHead className='w-[180px]'><RequiredColumnLabel>{t('cases.fieldBind')}</RequiredColumnLabel></TableHead>
            <TableHead className='w-16'><span className='inline-flex items-center gap-1'>{t('cases.fieldRequired')}<RequiredHint /></span></TableHead>
            <TableHead>{t('cases.fieldDescription')}</TableHead>
            {!hideActions ? <TableHead className='w-10' /> : null}
          </TableRow>
        </TableHeader>
        <TableBody>
          {fields.map((value, index) => <SortableInputTableRow key={`input-${index}`} id={index} nodes={nodes} value={value} index={index}
            onChange={onChange} onRemove={onRemove} disabled={disabled} hasError={Boolean(fieldErrors?.[index])} flash={flashIndex === index}
            hintOpen={index === 0 ? hintOpen : undefined} onHintOpenChange={index === 0 ? onHintOpenChange : undefined} hideActions={hideActions} />)}
          {fields.length === 0 ? <TableRow data-testid='input-table-empty'><TableCell colSpan={hideActions ? 5 : 7} className='py-6 text-center text-sm text-muted-foreground'>{t('cases.noRows')}</TableCell></TableRow> : null}
        </TableBody>
      </Table>
    </div>
  )
}

type OutputTableProps = {
  nodes: WorkflowNode[]; fields: OutputFieldDraft[]
  onChange: (index: number, next: OutputFieldDraft) => void; onRemove: (index: number) => void
  disabled?: boolean; hideActions?: boolean
}

export function OutputFieldsTable({ nodes, fields, onChange, onRemove, disabled, hideActions }: OutputTableProps) {
  const { t } = useTranslation()
  const { scrollRef, flashIndex } = useNewRowFlash(fields.length)
  return (
    <div className='overflow-hidden rounded-lg border bg-background'>
      <Table data-testid='output-fields-table' className='table-fixed' wrapperClassName='max-h-[300px] overflow-y-auto' wrapperRef={scrollRef}>
        <TableHeader className='sticky top-0 z-10 bg-background [&_th]:bg-background'>
          <TableRow>
            <TableHead><RequiredColumnLabel>{t('cases.fieldKey')}</RequiredColumnLabel></TableHead>
            <TableHead className='w-[180px]'><RequiredColumnLabel>{t('cases.fieldBind')}</RequiredColumnLabel></TableHead>
            <TableHead>{t('cases.fieldDescription')}</TableHead>
            {!hideActions ? <TableHead className='w-10' /> : null}
          </TableRow>
        </TableHeader>
        <TableBody>
          {fields.map((value, index) => {
            const node = nodes.find((n) => n.id === value.node_id)
            const bound = !!value.node_id
            return (
              <TableRow key={`output-${index}`} data-testid='output-table-row' className={flashIndex === index ? 'flash-highlight' : undefined}>
                <TableCell><Input className='h-8 w-full min-w-0' aria-label={t('cases.fieldKey')} value={value.key}
                  onChange={(e) => onChange(index, { ...value, key: e.target.value })} disabled={disabled} autoComplete='off' /></TableCell>
                <TableCell className='w-[180px]'>
                  <div className='flex w-full items-center gap-2'>
                    <BindNodePopover compact mode='output' nodes={nodes} node={bound ? node : undefined}
                      boundLabel={node && node.outputCount > 1 ? `[${value.index ?? 0}]` : undefined} disabled={disabled}
                      onPick={(nodeId, pick) => {
                        const picked = nodes.find((n) => n.id === nodeId)
                        onChange(index, { ...value, node_id: nodeId, index: Number.parseInt(pick, 10) || 0, type: picked ? outputKindFor(picked.class_type) : value.type })
                      }} />
                  </div>
                </TableCell>
                <TableCell><Input className='h-8 w-full min-w-0' aria-label={t('cases.fieldDescription')} value={value.description ?? ''}
                  onChange={(e) => onChange(index, { ...value, description: e.target.value })} disabled={disabled} autoComplete='off' /></TableCell>
                {!hideActions ? <TableCell className='w-10 text-right'><RemoveFieldButton compact fieldKey={value.key} disabled={disabled} onRemove={() => onRemove(index)} /></TableCell> : null}
              </TableRow>
            )
          })}
          {fields.length === 0 ? <TableRow data-testid='output-table-empty'><TableCell colSpan={hideActions ? 3 : 4} className='py-6 text-center text-sm text-muted-foreground'>{t('cases.noRows')}</TableCell></TableRow> : null}
        </TableBody>
      </Table>
    </div>
  )
}

type EditableInputFieldsProps = {
  nodes: WorkflowNode[]; fields: InputFieldDraft[]
  onChange: (index: number, next: InputFieldDraft) => void; onRemove: (index: number) => void; onAdd: () => void
  onReorder?: (next: InputFieldDraft[]) => void
  wide: boolean; disabled?: boolean; lockReason?: string; fieldErrors?: Record<number, boolean>
}

function AddFieldButton({ label, disabled, lockReason, onClick }: { label: string; disabled?: boolean; lockReason?: string; onClick: () => void }) {
  const button = <Button type='button' size='sm' variant='outline' disabled={disabled || Boolean(lockReason)} onClick={onClick} className='w-full gap-1.5'><Plus className='size-3.5' />{label}</Button>
  if (!lockReason) return button
  return <Tooltip><TooltipTrigger asChild><span className='flex w-full'>{button}</span></TooltipTrigger><TooltipContent side='top' sideOffset={6}>{lockReason}</TooltipContent></Tooltip>
}

export function EditableInputFields({ nodes, fields, onChange, onRemove, onAdd, onReorder, wide, disabled, lockReason, fieldErrors }: EditableInputFieldsProps) {
  const { t } = useTranslation()
  const [hintOpen, setHintOpen] = useState(false)
  const prevLen = useRef(0)
  const mounted = useRef(false)
  const hintTimer = useRef<number | undefined>(undefined)
  useEffect(() => {
    const prev = prevLen.current
    prevLen.current = fields.length
    if (!mounted.current) {
      mounted.current = true
      return
    }
    if (prev === 0 && fields.length > 0) {
      window.clearTimeout(hintTimer.current)
      setHintOpen(true)
      hintTimer.current = window.setTimeout(() => setHintOpen(false), 2500)
    }
  }, [fields.length])
  useEffect(() => () => window.clearTimeout(hintTimer.current), [])
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 8 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates })
  )
  const handleDragEnd = (event: DragEndEvent) => {
    const { active, over } = event
    if (!over || active.id === over.id) return
    const oldIndex = fields.findIndex((_, i) => i === Number(active.id))
    const newIndex = fields.findIndex((_, i) => i === Number(over.id))
    if (oldIndex < 0 || newIndex < 0) return
    onReorder?.(arrayMove(fields, oldIndex, newIndex))
  }
  return (
    <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={handleDragEnd}>
      <SortableContext items={fields.map((_, i) => i)} strategy={verticalListSortingStrategy}>
        <div className='flex flex-col gap-3'>
          {wide ? <InputFieldsTable nodes={nodes} fields={fields} onChange={onChange} onRemove={onRemove} disabled={disabled}
            fieldErrors={fieldErrors} hintOpen={hintOpen} onHintOpenChange={setHintOpen} /> : (
            <ul className='flex flex-col gap-3'>
              {fields.map((field, index) => <SortableInputCard key={`input-${index}`} id={index} nodes={nodes} value={field} index={index}
                onChange={onChange} onRemove={onRemove} disabled={disabled} hasError={Boolean(fieldErrors?.[index])}
                hintOpen={index === 0 ? hintOpen : undefined} onHintOpenChange={index === 0 ? setHintOpen : undefined} />)}
            </ul>
          )}
          <AddFieldButton label={t('cases.addInput')} disabled={disabled} lockReason={lockReason} onClick={onAdd} />
        </div>
      </SortableContext>
    </DndContext>
  )
}

type EditableOutputFieldsProps = {
  nodes: WorkflowNode[]; fields: OutputFieldDraft[]
  onChange: (index: number, next: OutputFieldDraft) => void; onRemove: (index: number) => void; onAdd: () => void
  wide: boolean; disabled?: boolean; lockReason?: string
}

export function EditableOutputFields({ nodes, fields, onChange, onRemove, onAdd, wide, disabled, lockReason }: EditableOutputFieldsProps) {
  const { t } = useTranslation()
  return (
    <div className='flex flex-col gap-3'>
      {wide ? <OutputFieldsTable nodes={nodes} fields={fields} onChange={onChange} onRemove={onRemove} disabled={disabled} /> : (
        <ul className='flex flex-col gap-3'>
          {fields.map((field, index) => <OutputFieldCard key={`output-${index}`} nodes={nodes} value={field}
            onChange={(next) => onChange(index, next)} onRemove={() => onRemove(index)} disabled={disabled} />)}
        </ul>
      )}
      <AddFieldButton label={t('cases.addOutput')} disabled={disabled} lockReason={lockReason} onClick={onAdd} />
    </div>
  )
}
