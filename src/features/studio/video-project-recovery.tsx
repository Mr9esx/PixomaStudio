import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { videoShotDuration, videoReferenceRoleLabels, type VideoProject, type VideoShot } from './video-editor-model'
import type { VideoExportSnapshot } from './video-export-model'
import { getWorkflow, getWorkflowVersion } from '@/lib/api/workflows'
import { GenerationInputValue } from './video-generation-records'

function ShotConfiguration({ shot }: { shot: VideoShot }) {
  const workflow = useQuery({ queryKey: ['studio', 'video-recovery-workflow', shot.workflowId, shot.versionId], enabled: Boolean(shot.workflowId && shot.versionId), queryFn: async () => {
    const [workflow, version] = await Promise.all([getWorkflow(shot.workflowId!), getWorkflowVersion(shot.workflowId!, shot.versionId!)])
    return { workflow, version }
  } })
  return <div className='flex flex-col gap-2'>
    {shot.media ? <div>采用视频：<GenerationInputValue value={{ asset_id: shot.media.assetId, asset_version_id: shot.media.versionId }} /></div> : null}
    {shot.references?.map(reference => <div key={reference.id}>{videoReferenceRoleLabels[reference.role]}：<GenerationInputValue value={{ asset_id: reference.assetId, asset_version_id: reference.versionId }} /></div>)}
    {shot.workflowId ? <details><summary>生成设置</summary>
      {workflow.isPending ? <p>正在读取工作流版本</p> : workflow.error ? <p role='alert'>{workflow.error.message}</p> : <p>{workflow.data?.workflow.name} · {workflow.data?.version.version}</p>}
      <dl className='flex flex-col gap-2'>{Object.entries(shot.inputs).map(([key, value]) => <div key={key}><dt>{workflow.data?.version.definition.inputs.find(field => field.key === key)?.description || key}</dt><dd className='whitespace-pre-wrap break-words'><GenerationInputValue value={value} /></dd></div>)}</dl>
    </details> : null}
  </div>
}

export function VideoProjectRecovery({ snapshot, current, disabled, onRestore, onClose }: {
  snapshot: VideoExportSnapshot; current: VideoProject; disabled: boolean; onRestore: () => void; onClose: () => void
}) {
  const [error, setError] = useState('')
  return <Dialog open onOpenChange={open => { if (!open) onClose() }}><DialogContent aria-describedby={undefined} className='max-h-[85vh] overflow-y-auto shadow-lg sm:max-w-3xl'>
    <DialogHeader><DialogTitle>来源作品{snapshot.sourceVersion === undefined ? '' : ` · 修订 ${snapshot.sourceVersion}`}</DialogTitle></DialogHeader>
    <div className='grid gap-4 sm:grid-cols-2'>{[{ title: '当前作品', project: current }, { title: '导出来源作品', project: snapshot.project }].map(({ title, project }) => <section key={title} className='flex min-w-0 flex-col gap-3 rounded-md border bg-muted p-3'>
      <strong className='text-sm'>{title}</strong>
      <p className='text-sm'>{project.name} · {project.width}×{project.height} · {project.fps} fps</p>
      <p className='whitespace-pre-wrap text-sm'>{project.script?.blocks.map(block => block.text).join('\n\n') || '尚未编写脚本'}</p>
      <ol className='flex list-inside list-decimal flex-col gap-3 text-sm'>{project.shots.map(shot => <li key={shot.id}>
        <strong>{shot.name}</strong><p className='whitespace-pre-wrap'>{shot.storyboard}</p>
        <p className='text-muted-foreground'>{shot.media ? `视频使用范围：${shot.sourceIn.toFixed(3)}–${(shot.sourceIn + shot.duration).toFixed(3)} 秒` : videoShotDuration(shot) === undefined ? '时长未定' : `计划 ${videoShotDuration(shot)!.toFixed(3)} 秒`}</p>
        <ShotConfiguration shot={shot} />
        {shot.subtitle ? <p>字幕：{shot.subtitle}</p> : null}
      </li>)}</ol>
      {project.audio?.length ? <p className='text-sm'>包含 {project.audio.length} 项成片音频</p> : null}
    </section>)}</div>
    <p className='text-sm text-muted-foreground'>恢复后采用来源作品的脚本、分镜、素材版本、生成设置和剪辑范围，生成记录保留。</p>
    {error ? <p role='alert' className='text-sm'>{error}</p> : null}
    <DialogFooter><Button variant='outline' onClick={onClose}>关闭</Button><Button disabled={disabled} onClick={() => {
      try { onRestore(); onClose() }
      catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)) }
    }}>恢复为当前作品</Button></DialogFooter>
  </DialogContent></Dialog>
}
