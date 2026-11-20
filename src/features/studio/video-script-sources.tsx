import { useState } from 'react'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import type { VideoProject, VideoShot } from './video-editor-model'
import { setVideoScriptSources, videoScriptSuccessors, type VideoScript } from './video-script'

export function VideoScriptSources({ project, shot, disabled, onCommit, onLocate }: {
  project: VideoProject
  shot: VideoShot
  disabled: boolean
  onCommit: (project: VideoProject) => void
  onLocate: (blockId: string) => void
}) {
  const [baseline, setBaseline] = useState<{ sources: VideoShot['scriptSources']; script: VideoScript }>()
  const [selection, setSelection] = useState<string[]>([])
  const [error, setError] = useState('')
  const script = project.script ?? { revision: 0, blocks: [] }
  function open() {
    setBaseline({ sources: structuredClone(shot.scriptSources), script: structuredClone(script) })
    setSelection([...new Set(shot.scriptSources?.flatMap(source => videoScriptSuccessors(script, source.blockId).map(block => block.id)) ?? [])])
    setError('')
  }
  function apply() {
    if (!baseline) return
    if (JSON.stringify(shot.scriptSources) !== JSON.stringify(baseline.sources)) { setError('原文关联已修改，请重新读取'); return }
    if (selection.some(id => JSON.stringify(script.blocks.find(block => block.id === id)) !== JSON.stringify(baseline.script.blocks.find(block => block.id === id)))) { setError('选定原文已修改，请重新读取'); return }
    onCommit({ ...project, shots: project.shots.map(item => item.id === shot.id ? setVideoScriptSources(item, script, selection) : item) })
    setBaseline(undefined)
  }
  return <section className='flex flex-col gap-2' aria-label='分镜原文关联'>
    <div className='flex items-center justify-between gap-2'><strong className='text-sm'>脚本原文</strong><Button size='sm' variant='ghost' disabled={disabled} onClick={open}>选择原文</Button></div>
    {shot.scriptSources?.length ? shot.scriptSources.map(source => {
      const block = script.blocks.find(block => block.id === source.blockId)
      return <div key={source.blockId} className='flex flex-col gap-2 rounded-md border bg-muted p-2'>
        <p className='whitespace-pre-wrap text-sm'>{source.text}</p>
        {!block || block.text !== source.text ? <span className='text-xs text-muted-foreground'>{block ? '原文已修改' : videoScriptSuccessors(script, source.blockId).length ? '检查原文关联' : '原文已删除'}</span> : null}
        {block ? <Button size='sm' variant='ghost' className='self-start' onClick={() => onLocate(block.id)}>定位原文</Button> : null}
      </div>
    }) : <span className='text-sm text-muted-foreground'>手动创建</span>}
    <Dialog open={Boolean(baseline)} onOpenChange={open => { if (!open) setBaseline(undefined) }}><DialogContent className='max-h-[85vh] overflow-y-auto shadow-lg' aria-describedby={undefined}>
      <DialogHeader><DialogTitle>选择脚本原文 · {shot.name}</DialogTitle></DialogHeader>
      <div className='flex flex-col gap-3'>{baseline?.script.blocks.map((block, index) => <label key={block.id} className='flex items-start gap-3 rounded-md border p-3'>
        <Checkbox aria-label={`关联脚本内容 ${index + 1}`} checked={selection.includes(block.id)} onCheckedChange={checked => setSelection(current => checked ? [...current, block.id] : current.filter(id => id !== block.id))} />
        <span className='whitespace-pre-wrap text-sm'>{block.text || '空白内容'}</span>
      </label>)}</div>
      {error ? <div role='alert' className='flex items-center gap-2 text-sm'>{error}<Button size='sm' variant='outline' onClick={open}>重新读取</Button></div> : null}
      <DialogFooter><Button size='sm' variant='outline' onClick={() => setBaseline(undefined)}>取消</Button><Button size='sm' disabled={disabled} onClick={apply}>保存原文关联</Button></DialogFooter>
    </DialogContent></Dialog>
  </section>
}
