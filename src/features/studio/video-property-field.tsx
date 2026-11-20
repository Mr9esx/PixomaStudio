import { Button } from '@/components/ui/button'
import { Field, FieldError, FieldLabel } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import type { VideoPropertyDraft } from './video-property-edit'

export function VideoPropertyField<T extends string | number>({ id, label, value, displayValue, placeholder, draft, disabled, onDraftChange, onApply }: {
  id: string
  label: string
  value: T
  displayValue?: string
  placeholder?: string
  draft?: VideoPropertyDraft<T>
  disabled?: boolean
  onDraftChange: (draft?: VideoPropertyDraft<T>) => void
  onApply: (draft: VideoPropertyDraft<T>) => void
}) {
  const conflict = Boolean(draft && draft.baseline !== value)
  const error = draft?.error ?? (conflict ? '该内容已更新，请选择使用最新内容或保留输入' : '')
  function apply(replace = false) {
    if (!draft || disabled) return
    const submitted = replace ? { ...draft, baseline: value } : draft
    try {
      onApply(submitted)
      onDraftChange(undefined)
    } catch (cause) {
      onDraftChange({ ...submitted, error: cause instanceof Error ? cause.message : String(cause) })
    }
  }
  return <Field data-invalid={Boolean(error)}>
    <FieldLabel htmlFor={id}>{label}</FieldLabel>
    <Input id={id} inputMode={typeof value === 'number' ? 'decimal' : 'text'} value={draft?.text ?? displayValue ?? String(value)} placeholder={placeholder} disabled={disabled} aria-invalid={Boolean(error)} aria-describedby={error ? `${id}-error` : undefined}
      onChange={event => onDraftChange({ text: event.target.value, baseline: draft?.baseline ?? value })}
      onBlur={() => apply()}
      onKeyDown={event => {
        if (event.nativeEvent.isComposing) return
        if (event.key === 'Enter') { event.preventDefault(); apply() }
        if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); onDraftChange(undefined) }
      }} />
    {error ? <FieldError id={`${id}-error`}>{error}</FieldError> : null}
    {draft && error ? <div className='flex flex-wrap gap-1'>
      <Button type='button' size='sm' variant='ghost' disabled={disabled} onClick={() => onDraftChange(undefined)}>{conflict ? '使用最新内容' : '取消修改'}</Button>
      {conflict ? <Button type='button' size='sm' variant='outline' disabled={disabled} onClick={() => apply(true)}>保留输入</Button> : null}
    </div> : null}
  </Field>
}
