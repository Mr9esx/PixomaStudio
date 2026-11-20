import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { TriangleAlert } from 'lucide-react'
import { createWorkflowVersion, listWorkflowVersions, setDefaultWorkflowVersion, updateWorkflow, type Workflow, type WorkflowVersion } from '@/lib/api/workflows'
import { Alert, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Field, FieldGroup, FieldLabel } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { Skeleton } from '@/components/ui/skeleton'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { Textarea } from '@/components/ui/textarea'
import { CodeEditor } from '@/components/code-editor'
import { WorkflowIntroduction } from './workflow-introduction'
import { WorkflowExecutionPanel } from './workflow-execution'
import { WorkflowVersionList, type WorkflowVersionAction } from './workflow-version-list'
import { WorkflowConfigView } from './sections/workflow-config-view'

export function WorkflowVersions({ record }: { record: Workflow }) {
  const { t } = useTranslation()
  const client = useQueryClient()
  const [selectedId, setSelectedId] = useState<string>()
  const [viewing, setViewing] = useState(false)
  const [creating, setCreating] = useState(false)
  const [testing, setTesting] = useState(false)
  const [restoring, setRestoring] = useState(false)
  const [versionNumber, setVersionNumber] = useState('')
  const [notes, setNotes] = useState('')
  const versions = useQuery({ queryKey: ['studio', 'workflow-versions', record.id], queryFn: () => listWorkflowVersions(record.id) })
  const selected = versions.data?.find((item) => item.id === selectedId)
  const refresh = async () => {
    await Promise.all([
      client.invalidateQueries({ queryKey: ['studio', 'workflow-versions', record.id] }),
      client.invalidateQueries({ queryKey: ['studio', 'workflow', record.id] }),
      client.invalidateQueries({ queryKey: ['studio', 'workflows'] }),
    ])
  }
  const create = useMutation({ mutationFn: () => createWorkflowVersion(record.id, { expected_revision: record.revision, version: versionNumber, notes }),
    onSuccess: async (version) => { setCreating(false); setSelectedId(version.id); setVersionNumber(''); setNotes(''); await refresh() },
  })
  const setDefault = useMutation({ mutationFn: (versionId: string) => setDefaultWorkflowVersion(record.id, versionId), onSuccess: refresh })
  const restore = useMutation({ mutationFn: (version: WorkflowVersion) => updateWorkflow(record.id, { expected_revision: record.revision, draft: version.definition }),
    onSuccess: async () => { setRestoring(false); await refresh() },
  })
  const busy = create.isPending || setDefault.isPending || restore.isPending
  const onAction = (action: WorkflowVersionAction, version: WorkflowVersion) => {
    if (busy) return
    setSelectedId(version.id)
    switch (action) {
      case 'view': setViewing(true); break
      case 'test': setTesting(true); break
      case 'default': setDefault.mutate(version.id); break
      case 'restore': restore.reset(); setRestoring(true); break
    }
  }
  if (selected && testing) return <WorkflowExecutionPanel key={selected.id} workflow={record} definition={selected.definition} versionId={selected.id} onClose={() => setTesting(false)} />
  return <section className='flex min-w-0 flex-col gap-3'>
    {versions.isPending ? <Skeleton className='h-40 w-full' /> : versions.isError ? <Alert variant='destructive'><TriangleAlert /><AlertTitle>{versions.error.message}</AlertTitle><Button size='sm' variant='outline' onClick={() => void versions.refetch()}>{t('common.retry')}</Button></Alert>
      : <WorkflowVersionList versions={versions.data} defaultVersionId={record.default_version_id} busy={busy} onAction={onAction} onCreate={() => { create.reset(); setCreating(true) }} />}
    {setDefault.error ? <p role='alert' className='text-sm text-destructive'>{setDefault.error.message}</p> : null}
    {selected && viewing ? <section aria-label={t('workflowMarket.versionContent')} className='flex min-w-0 flex-col gap-5 rounded-xl bg-muted p-5'>
      <header className='flex items-center justify-between gap-3'><h3 className='text-sm font-semibold'>{t('workflowMarket.versionContent')} · <span className='tabular-nums'>{selected.version}</span></h3><Button size='sm' variant='ghost' onClick={() => setViewing(false)}>收起</Button></header>
      <Tabs key={selected.id} defaultValue='configuration' className='min-h-0 min-w-0 gap-4'>
        <TabsList aria-label={t('workflowMarket.versionContent')}><TabsTrigger value='configuration'>{t('cases.sectionConfig')}</TabsTrigger><TabsTrigger value='document'>{t('workflowMarket.versionDocument')}</TabsTrigger></TabsList>
        <TabsContent value='configuration' className='min-h-0 overflow-auto'><div className='flex flex-col gap-4'>
          <WorkflowIntroduction workflow={{ ...selected.definition, name: record.name, categories: [record.category], tags: record.tags }} inputs={selected.definition.inputs} outputs={selected.definition.outputs} />
          <WorkflowConfigView definition={selected.definition} />
        </div></TabsContent>
        <TabsContent value='document' className='min-h-0 overflow-auto'><CodeEditor value={JSON.stringify(selected.definition, null, 2)} readOnly minHeight={300} maxHeight={600} /></TabsContent>
      </Tabs>
    </section> : null}
    {creating ? <section aria-label={t('workflowMarket.createVersion')} className='flex flex-col gap-5 rounded-xl bg-muted p-5'>
      <h3 className='text-sm font-semibold'>{t('workflowMarket.createVersion')}</h3>
      <form className='flex flex-col gap-4' onSubmit={(event) => { event.preventDefault(); create.mutate() }}>
        <FieldGroup>
          <Field><FieldLabel htmlFor='workflow-version-number'>{t('workflowMarket.version')}</FieldLabel><Input id='workflow-version-number' value={versionNumber} onChange={(event) => setVersionNumber(event.target.value)} placeholder={t('workflowMarket.versionPlaceholder')} required disabled={create.isPending} /></Field>
          <Field><FieldLabel htmlFor='workflow-version-notes'>{t('workflowMarket.versionNotes')}</FieldLabel><Textarea id='workflow-version-notes' value={notes} onChange={(event) => setNotes(event.target.value)} disabled={create.isPending} /></Field>
        </FieldGroup>
        {create.error ? <p role='alert' className='text-sm text-destructive'>{create.error.message}</p> : null}
        <div className='flex justify-end gap-2'><Button type='button' size='sm' variant='outline' disabled={create.isPending} onClick={() => setCreating(false)}>{t('common.cancel')}</Button><Button type='submit' size='sm' disabled={create.isPending}>{t('workflowMarket.createVersion')}</Button></div>
      </form>
    </section> : null}
    <Dialog open={restoring} onOpenChange={(open) => { if (!restore.isPending) setRestoring(open) }}><DialogContent>
      <DialogHeader><DialogTitle>{t('workflowMarket.restoreDraft')}</DialogTitle></DialogHeader>
      <p className='text-sm tabular-nums'>{selected?.version}</p>
      {restore.error ? <p role='alert' className='text-sm text-destructive'>{restore.error.message}</p> : null}
      <DialogFooter><Button size='sm' variant='outline' disabled={restore.isPending} onClick={() => setRestoring(false)}>{t('common.cancel')}</Button><Button size='sm' disabled={!selected || restore.isPending} onClick={() => restore.mutate(selected!)}>{t('workflowMarket.restoreDraft')}</Button></DialogFooter>
    </DialogContent></Dialog>
  </section>
}
