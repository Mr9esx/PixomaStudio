import { Workflow } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import Markdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { resolveMediaKey } from '@/lib/api/media'
import type { CaseRecord } from '@/lib/api/types'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { useMediaObjectUrl } from './lib/use-media-object-url'

export type WorkflowIntroductionContent = Pick<CaseRecord, 'name' | 'description' | 'preview' | 'usage' | 'categories' | 'tags' | 'examples'>

const fieldTypeLabels: Record<string, string> = {
  string: 'cases.typeText', text: 'cases.typeText', image: 'cases.typeImage', video: 'cases.typeVideo',
  number: 'cases.typeNumber', boolean: 'workflowMarket.choice', enum: 'workflowMarket.choice',
  audio: 'workflowMarket.audio', file: 'cases.typeFile',
}

export function WorkflowCover({ preview, name, compact = false }: { preview?: string; name: string; compact?: boolean }) {
  const mediaURL = useMediaObjectUrl(preview)
  const isVideo = /\.(mp4|webm)$/i.test(resolveMediaKey(preview) ?? '')
  return (
    <div className={`flex items-center justify-center overflow-hidden rounded-lg bg-muted ${compact ? 'aspect-video' : 'aspect-video max-h-96'}`}>
      {mediaURL ? isVideo
        ? <video src={mediaURL} controls={!compact} muted={compact} playsInline preload='metadata' aria-label={name} className='size-full object-contain' />
        : <img src={mediaURL} alt={name} loading='lazy' className='size-full object-contain' />
        : <Workflow className='size-10 text-muted-foreground' aria-hidden='true' />}
    </div>
  )
}

export function WorkflowIntroduction({ workflow, inputs, outputs, onUseExample }: {
  workflow: WorkflowIntroductionContent
  inputs: Pick<CaseRecord['inputs'][number], 'type' | 'description' | 'required'>[]
  outputs: Pick<CaseRecord['outputs'][number], 'type' | 'description'>[]
  onUseExample?: (prompt: string) => void
}) {
  const { t } = useTranslation()
  const sections = [
    { title: t('workflowMarket.introduction'), content: workflow.description },
    { title: t('workflowMarket.usage'), content: workflow.usage },
  ]
  return (
    <div className='flex flex-col gap-6'>
      {workflow.preview ? <WorkflowCover preview={workflow.preview} name={workflow.name} /> : null}
      {((workflow.categories?.length ?? 0) + (workflow.tags?.length ?? 0)) > 0 ? <div className='flex flex-wrap gap-2'>
        {[...(workflow.categories ?? []), ...(workflow.tags ?? [])].map((tag, index) => <Badge key={`${tag}-${index}`} variant='secondary'>{tag}</Badge>)}
      </div> : null}
      {sections.filter((section) => section.content).map((section) => <section key={section.title} className='flex flex-col gap-3'>
        <h3 className='text-sm font-semibold'>{section.title}</h3>
        <div className='prose prose-sm max-w-none text-sm text-foreground prose-headings:text-sm prose-headings:font-semibold prose-headings:text-foreground prose-a:text-primary prose-code:text-xs prose-code:text-foreground prose-pre:border prose-pre:bg-background prose-table:text-sm'>
          <Markdown remarkPlugins={[remarkGfm]}>{section.content}</Markdown>
        </div>
      </section>)}
      <div className='grid gap-4 min-[920px]:grid-cols-2'>
        {[{ title: t('workflowMarket.inputs'), fields: inputs }, { title: t('workflowMarket.outputs'), fields: outputs }].map((group) => <section key={group.title} className='flex flex-col gap-4 rounded-xl bg-muted p-5'>
          <h3 className='text-sm font-semibold'>{group.title}</h3>
          <ul className='flex flex-col gap-3'>{group.fields.map((field, index) => <li key={index} className='flex flex-wrap items-start gap-2 text-sm'>
            <span className='min-w-0 flex-1'>{field.description || t(fieldTypeLabels[field.type] ?? 'cases.typeFile')}</span>
            {field.description ? <Badge variant='secondary'>{t(fieldTypeLabels[field.type] ?? 'cases.typeFile')}</Badge> : null}
            {'required' in field && field.required ? <Badge variant='outline'>{t('workflowMarket.required')}</Badge> : null}
          </li>)}</ul>
        </section>)}
      </div>
      {workflow.examples?.length ? <section className='flex flex-col gap-3'>
        <h3 className='text-sm font-semibold'>{t('workflowMarket.examples')}</h3>
        <div className='flex flex-col gap-4'>
          {workflow.examples.map((example, index) => <article key={`${example.title}-${index}`} className='flex flex-col gap-4 py-4'>
            <h3 className='text-sm font-semibold'>{example.title}</h3>
            {example.preview ? <WorkflowCover preview={example.preview} name={example.title} /> : null}
            <div className='prose prose-sm max-w-none text-sm text-foreground prose-headings:text-sm prose-headings:font-semibold prose-headings:text-foreground prose-a:text-primary prose-code:text-xs prose-code:text-foreground prose-pre:border prose-pre:bg-background prose-table:text-sm'><Markdown remarkPlugins={[remarkGfm]}>{example.prompt}</Markdown></div>
            {onUseExample ? <Button type='button' size='sm' variant='outline' className='self-start' onClick={() => onUseExample(example.prompt)}>{t('workflowMarket.useExample')}</Button> : null}
          </article>)}
        </div>
      </section> : null}
    </div>
  )
}
