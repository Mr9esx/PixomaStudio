import { useEffect, useRef, useState } from 'react'
import { Button } from '@/components/ui/button'
import { Textarea } from '@/components/ui/textarea'
import { Checkbox } from '@/components/ui/checkbox'
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { videoShotDuration, type VideoProject } from './video-editor-model'
import { addVideoScriptBlock, removeVideoScriptBlock, updateVideoScriptBlock, videoScriptSourceState, videoScriptSourceLabels, splitVideoScriptBlock, mergeVideoScriptBlocks } from './video-script'
import { applyVideoCreativeProposal } from './video-creative-proposal'

type Props = {
  project: VideoProject
  disabled: boolean
  onCommit: (project: VideoProject, group?: string) => void
  onCreateScript: () => void
  onImportScript?: () => void
  onCreateStoryboard: (shotIds?: string[]) => void
  onSelectShot: (id: string) => void
  canRemoveShot?: (id: string) => boolean
  selectedShotId?: string
  focus?: { blockId: string; requestId: string }
}

export function VideoScriptPanel({ project, disabled, onCommit, onCreateScript, onImportScript, onCreateStoryboard, onSelectShot, canRemoveShot, selectedShotId, focus }: Props) {
  const root = useRef<HTMLElement>(null)
  const cursors = useRef(new Map<string, number>())
  const [structureError, setStructureError] = useState('')
  useEffect(() => {
    if (!focus) return
    const target = [...root.current!.querySelectorAll<HTMLTextAreaElement>('textarea[data-block-id]')].find(element => element.dataset.blockId === focus.blockId)
    target?.focus({ preventScroll: true })
    target?.scrollIntoView({ block: 'nearest' })
  }, [focus])
  const [proposalId, setProposalId] = useState<string>()
  const [selection, setSelection] = useState<string[]>([])
  const [applyOrder, setApplyOrder] = useState(false)
  const [error, setError] = useState('')
  const proposal = project.proposals?.find(item => item.id === proposalId)
  const script = project.script ?? { revision: 0, blocks: [] }
  function changeStructure(change: () => VideoProject) {
    try { onCommit(change()); setStructureError('') }
    catch (cause) { setStructureError(cause instanceof Error ? cause.message : String(cause)) }
  }
  function apply() {
    try {
      if (proposal?.kind === 'storyboard' && proposal.changes.some(change => change.action === 'remove' && selection.includes(change.shotId) && canRemoveShot && !canRemoveShot(change.shotId))) throw new Error('生成任务尚未登记，请读取任务状态后采用删除建议')
      onCommit(applyVideoCreativeProposal(project, proposalId!, proposal?.kind === 'storyboard' ? selection : undefined, applyOrder)); setProposalId(undefined); setError('')
    }
    catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)) }
  }
  return <section ref={root} className='flex h-full min-h-0 flex-col bg-card' aria-label='完整脚本'>
    <header className='flex flex-wrap items-center gap-2 border-b p-3'><strong className='flex-1 text-sm'>脚本</strong>{onImportScript ? <Button size='sm' variant='ghost' disabled={disabled} onClick={onImportScript}>选择文档</Button> : null}<Button size='sm' variant='ghost' disabled={disabled} onClick={onCreateScript}>对话创作</Button></header>
    <div className='flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto p-3'>
      {script.blocks.map((block, index) => {
        const shots = project.shots.filter(shot => shot.scriptSources?.some(source => source.blockId === block.id || block.ancestorIds?.includes(source.blockId)))
        return <div key={block.id} className='flex flex-col gap-2 rounded-md data-[selected=true]:ring-1 data-[selected=true]:ring-ring' data-selected={shots.some(shot => shot.id === selectedShotId)}>
          <Textarea data-block-id={block.id} aria-label={`脚本内容 ${index + 1}`} className='min-h-32 resize-y bg-background' value={block.text} disabled={disabled} placeholder='编写脚本内容' onSelect={event => cursors.current.set(block.id, event.currentTarget.selectionStart)} onChange={event => onCommit(updateVideoScriptBlock(project, block.id, event.target.value), `script:${block.id}`)} />
          <div className='flex flex-wrap gap-1'>{shots.map(shot => <Button key={shot.id} size='sm' variant='ghost' onClick={() => onSelectShot(shot.id)}>{shot.name}{videoScriptSourceLabels[videoScriptSourceState(shot, script)] ? ` · ${videoScriptSourceLabels[videoScriptSourceState(shot, script)]}` : ''}</Button>)}</div>
          <div className='flex flex-wrap gap-1'><Button size='sm' variant='ghost' disabled={disabled} aria-label={`分开脚本内容 ${index + 1}`} onClick={() => changeStructure(() => splitVideoScriptBlock(project, block.id, cursors.current.get(block.id) ?? 0, [crypto.randomUUID(), crypto.randomUUID()]))}>按光标分开</Button><Button size='sm' variant='ghost' disabled={disabled || index === script.blocks.length - 1} aria-label={`合并脚本内容 ${index + 1} 与下一段`} onClick={() => changeStructure(() => mergeVideoScriptBlocks(project, [block.id, script.blocks[index + 1].id], crypto.randomUUID()))}>合并下一段</Button><Button size='sm' variant='ghost' disabled={disabled} onClick={() => onCommit(removeVideoScriptBlock(project, block.id))} aria-label={`删除脚本内容 ${index + 1}`}>删除</Button></div>
          {shots.length ? <Button size='sm' variant='outline' disabled={disabled} onClick={() => onCreateStoryboard(shots.map(shot => shot.id))}>更新相关分镜</Button> : null}
        </div>
      })}
      {structureError ? <p role='alert' className='text-sm'>{structureError}</p> : null}
      <Button size='sm' variant='outline' disabled={disabled} onClick={() => onCommit(addVideoScriptBlock(project, crypto.randomUUID()))}>{script.blocks.length ? '继续编写' : '编写脚本'}</Button>
      {project.proposals?.map(item => <Button key={item.id} size='sm' variant='outline' onClick={() => { setProposalId(item.id); setSelection(item.kind === 'storyboard' ? item.changes.filter(change => change.action !== 'remove').map(change => change.shotId) : []); setApplyOrder(false); setError('') }}>查看{item.kind === 'script' ? '脚本提案' : `分镜提案 · ${item.changes.length + (item.reorder ? 1 : 0)}`}</Button>)}
    </div>
    <footer className='flex gap-2 border-t p-3'><Button size='sm' className='flex-1' disabled={disabled || !script.blocks.some(block => block.text.trim())} onClick={() => onCreateStoryboard()}>根据脚本生成分镜</Button></footer>
    <Dialog open={Boolean(proposal)} onOpenChange={open => { if (!open) setProposalId(undefined) }}><DialogContent className='max-h-[85vh] overflow-y-auto shadow-lg sm:max-w-2xl' aria-describedby={undefined}>
      <DialogHeader><DialogTitle>{proposal?.kind === 'script' ? '脚本提案' : '分镜提案'}</DialogTitle></DialogHeader>
      {proposal?.kind === 'script' ? <div className='flex flex-col gap-3'><div className='rounded-md border bg-muted p-3'><strong className='text-sm'>当前脚本</strong><p className='whitespace-pre-wrap text-sm'>{script.blocks.map(block => block.text).join('\n\n') || '尚未编写'}</p></div><div className='rounded-md border bg-background p-3'><strong className='text-sm'>生成内容</strong><p className='whitespace-pre-wrap text-sm'>{proposal.script.blocks.map(block => block.text).join('\n\n') || '空脚本'}</p></div></div> : proposal?.changes.map(change => <div key={change.shotId} className='flex gap-3 rounded-md border p-3'>
        <Checkbox aria-label={`采用 ${change.name}`} checked={selection.includes(change.shotId)} onCheckedChange={checked => setSelection(current => checked ? [...current, change.shotId] : current.filter(id => id !== change.shotId))} />
        <div className='flex min-w-0 flex-1 flex-col gap-2'><strong className='text-sm'>{{ add: '新增', update: '修改', remove: '删除' }[change.action]} · {change.name}</strong>{change.action === 'update' ? <p className='whitespace-pre-wrap text-sm text-muted-foreground'>当前：{project.shots.find(shot => shot.id === change.shotId)?.storyboard || '尚未填写'}</p> : null}<p className='whitespace-pre-wrap text-sm'>{change.storyboard}</p><p className='whitespace-pre-wrap text-xs text-muted-foreground'>{change.sources.map(source => source.text).join('\n')}</p>{change.action === 'remove' ? <div className='flex flex-col gap-1 text-sm text-muted-foreground'>
          <p>当前分镜从成片移除，生成任务与结果保留。</p>
          {project.shots.filter(shot => shot.id === change.shotId).map(shot => <p key={shot.id}>移除时长：{videoShotDuration(shot) === undefined ? '时长未定' : `${videoShotDuration(shot)!.toFixed(3)} 秒`}</p>)}
          {project.shots.filter(shot => shot.references?.some(reference => reference.sourceShotId === change.shotId)).map(shot => <p key={shot.id}>{shot.name} 的画面引用需要检查</p>)}
        </div> : null}</div>
      </div>)}
      {proposal?.kind === 'storyboard' && proposal.reorder ? <div className='flex gap-3 rounded-md border p-3'><Checkbox aria-label='采用分镜顺序建议' checked={applyOrder} onCheckedChange={checked => setApplyOrder(checked === true)} /><div className='flex min-w-0 flex-1 flex-col gap-2'><strong className='text-sm'>调整分镜顺序</strong><p className='text-sm text-muted-foreground'>当前：{project.shots.map(shot => shot.name).join(' → ')}</p><ol className='list-inside list-decimal text-sm'>{proposal.reorder.proposed.map(id => <li key={id}>{proposal.changes.find(change => change.shotId === id)?.name ?? project.shots.find(shot => shot.id === id)?.name ?? '分镜已删除'}</li>)}</ol></div></div> : null}
      {error ? <p role='alert' className='text-sm'>{error}</p> : null}
      <DialogFooter><Button size='sm' variant='ghost' disabled={disabled} onClick={() => { onCommit({ ...project, proposals: project.proposals?.filter(item => item.id !== proposalId) }); setProposalId(undefined) }}>删除提案</Button><Button size='sm' variant='outline' onClick={() => setProposalId(undefined)}>继续编辑</Button><Button size='sm' disabled={disabled || (proposal?.kind === 'storyboard' && !selection.length && !applyOrder)} onClick={apply}>{proposal?.kind === 'script' ? '采用脚本' : `采用 ${selection.length + (applyOrder ? 1 : 0)} 项修改`}</Button></DialogFooter>
    </DialogContent></Dialog>
  </section>
}
