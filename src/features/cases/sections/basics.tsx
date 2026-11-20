import type { RefObject } from 'react'
import { useTranslation } from 'react-i18next'
import { SectionHead } from '@/components/section-head'
import { RequiredBadge } from '@/components/required-badge'
import { Button } from '@/components/ui/button'
import { Field, FieldError, FieldGroup, FieldLabel } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { MultiSelect } from '@/components/ui/multi-select'
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Switch } from '@/components/ui/switch'
import type { WorkflowEditorRecord } from '../lib/workflow-record'
import { MarkdownTextField } from './markdown-text-field'
import { MediaPreviewField } from './media-preview-field'

export type BasicsSlice = Pick<WorkflowEditorRecord, 'name' | 'description' | 'usage' | 'examples' | 'preview' | 'tags' | 'category' | 'enabled'>
type Props = {
  value: BasicsSlice; onChange: (next: BasicsSlice) => void; showEnabled?: boolean
  disabled?: boolean; nameError?: string; nameRef?: RefObject<HTMLInputElement | null>
}

export function BasicsSection({ value, onChange, showEnabled = false, disabled, nameError, nameRef }: Props) {
  const { t } = useTranslation()
  function patch(partial: Partial<BasicsSlice>) { onChange({ ...value, ...partial }) }
  return (
    <section className='flex flex-col gap-4' data-testid='case-section-basics'>
      <SectionHead title={t('cases.sectionBasics')} />
      <FieldGroup className='gap-4'>
        <Field data-invalid={Boolean(nameError)}>
          <FieldLabel htmlFor='case-name'>{t('cases.fieldName')}<RequiredBadge /></FieldLabel>
          <Input id='case-name' ref={nameRef} value={value.name} onChange={(e) => patch({ name: e.target.value })}
            disabled={disabled} required autoComplete='off' aria-invalid={Boolean(nameError) || undefined} aria-describedby={nameError ? 'case-name-error' : undefined} />
          {nameError ? <FieldError id='case-name-error'>{nameError}</FieldError> : null}
        </Field>
        <Field>
          <FieldLabel htmlFor='case-description'>{t('cases.fieldDescription')}</FieldLabel>
          <MarkdownTextField id='case-description' value={value.description ?? ''} onChange={(description) => patch({ description })} disabled={disabled} />
        </Field>
        <Field>
          <FieldLabel htmlFor='case-usage'>{t('workflowMarket.usage')}</FieldLabel>
          <MarkdownTextField id='case-usage' value={value.usage ?? ''} onChange={(usage) => patch({ usage })} disabled={disabled} />
        </Field>
        <Field>
          <div className='flex items-center justify-between gap-2'>
            <FieldLabel>{t('workflowMarket.examples')}</FieldLabel>
            <Button type='button' variant='outline' size='sm' disabled={disabled} onClick={() => patch({ examples: [...(value.examples ?? []), { title: '', prompt: '', preview: '' }] })}>{t('workflowMarket.addExample')}</Button>
          </div>
          {(value.examples ?? []).map((example, index) => {
            const update = (partial: Partial<typeof example>) => patch({ examples: value.examples?.map((current, currentIndex) => currentIndex === index ? { ...current, ...partial } : current) })
            return (
              <div key={index} className='flex flex-col gap-3 rounded-md border bg-muted p-3'>
                <div className='flex items-end gap-2'>
                  <Field className='flex-1'>
                    <FieldLabel htmlFor={`case-example-title-${index}`}>{t('workflowMarket.exampleTitle')}</FieldLabel>
                    <Input id={`case-example-title-${index}`} value={example.title} disabled={disabled} onChange={(event) => update({ title: event.target.value })} />
                  </Field>
                  <Button type='button' variant='outline' size='sm' disabled={disabled} onClick={() => patch({ examples: value.examples?.filter((_, currentIndex) => currentIndex !== index) })}>{t('common.delete')}</Button>
                </div>
                <Field>
                  <FieldLabel htmlFor={`case-example-prompt-${index}`}>{t('workflowMarket.examplePrompt')}</FieldLabel>
                  <MarkdownTextField id={`case-example-prompt-${index}`} value={example.prompt} disabled={disabled} onChange={(prompt) => update({ prompt })} />
                </Field>
                <Field><FieldLabel>{t('workflowMarket.examplePreview')}</FieldLabel><MediaPreviewField value={example.preview} disabled={disabled} onChange={(preview) => update({ preview: preview ?? '' })} /></Field>
              </div>
            )
          })}
        </Field>
        <Field><FieldLabel>{t('cases.fieldPreview')}</FieldLabel><MediaPreviewField value={value.preview} onChange={(preview) => patch({ preview: preview ?? '' })} disabled={disabled} /></Field>
        <div className='grid gap-4 sm:grid-cols-2'>
          <Field>
            <FieldLabel>{t('cases.fieldTags')}</FieldLabel>
            <MultiSelect value={value.tags ?? []} onValueChange={(tags) => patch({ tags })} disabled={disabled} placeholder={t('cases.listPlaceholder')}
              inputPlaceholder={t('cases.listInputPlaceholder')} createLabel={t('cases.listAdd')} emptyText={t('cases.listEmpty')} />
          </Field>
          <Field>
            <FieldLabel htmlFor='case-category'>{t('cases.fieldCategories')}</FieldLabel>
            <Select value={value.category} onValueChange={(category) => patch({ category })} disabled={disabled}>
              <SelectTrigger id='case-category'><SelectValue /></SelectTrigger>
              <SelectContent><SelectGroup><SelectItem value='image'>图片</SelectItem><SelectItem value='video'>视频</SelectItem>{value.category && !['image', 'video'].includes(value.category) ? <SelectItem value={value.category}>{value.category}</SelectItem> : null}</SelectGroup></SelectContent>
            </Select>
          </Field>
        </div>
        {showEnabled ? <Field orientation='horizontal'>
          <Label htmlFor='case-enabled' className='flex-1'>{t('cases.fieldEnabled')}</Label>
          <Switch id='case-enabled' checked={value.enabled} onCheckedChange={(enabled) => patch({ enabled })} disabled={disabled} />
        </Field> : null}
      </FieldGroup>
    </section>
  )
}
